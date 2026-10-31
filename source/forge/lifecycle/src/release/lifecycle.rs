//! Explicit package lineage, independent of native names, versions, and paths.
use super::model::{Language, NativePackage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub id: String,
    pub language: Language,
    pub manifest: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Transition {
    pub id: String,
    pub base: String,
    pub operation: String,
    pub compatibility: String,
    pub from: Vec<Identity>,
    pub to: Vec<Identity>,
}

impl Transition {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || !matches!(self.base.len(), 40 | 64)
            || !self.base.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("lifecycle transition needs an id and immutable release base".into());
        }
        let valid = match self.operation.as_str() {
            "rename" | "move" => {
                self.from.len() == 1 && self.to.len() == 1 && self.from[0].id == self.to[0].id
            }
            "remove" => self.from.len() == 1 && self.to.is_empty(),
            "split" => self.from.len() == 1 && self.to.len() >= 2,
            "merge" => self.from.len() >= 2 && self.to.len() == 1,
            _ => false,
        };
        if !valid
            || !matches!(
                self.compatibility.as_str(),
                "mechanical" | "compatible" | "breaking"
            )
        {
            return Err(format!(
                "{}: invalid lifecycle cardinality or compatibility",
                self.id
            ));
        }
        if self.compatibility == "mechanical" && self.operation != "move" {
            return Err(format!("{}: only a pure move can be mechanical", self.id));
        }
        for side in [&self.from, &self.to] {
            let mut ids = BTreeSet::new();
            let mut manifests = BTreeSet::new();
            for p in side {
                if p.id.is_empty()
                    || p.name.is_empty()
                    || !ids.insert(&p.id)
                    || !manifests.insert(&p.manifest)
                {
                    return Err(format!(
                        "{}: duplicate or empty lifecycle endpoint",
                        self.id
                    ));
                }
            }
        }
        for old in &self.from {
            if let Some(new) = self.to.iter().find(|p| p.id == old.id) {
                if old.language != new.language {
                    return Err(
                        "version inheritance across languages requires a new identity".into(),
                    );
                }
            }
        }
        if self.operation == "move" && self.from[0].name != self.to[0].name {
            return Err("move must keep the package name; declare rename instead".into());
        }
        Ok(())
    }
}

pub(super) fn matches(identity: &Identity, package: &NativePackage) -> bool {
    identity.language == package.language
        && identity.manifest == package.manifest
        && identity.name == package.name
        && identity
            .workspace
            .as_ref()
            .is_none_or(|root| root == &package.workspace)
}

#[cfg(test)]
pub(super) fn next_ids(id: &str, transitions: &[Transition]) -> BTreeSet<String> {
    let mut result = BTreeSet::from([id.to_owned()]);
    for transition in transitions {
        let sources = transition
            .from
            .iter()
            .map(|p| p.id.clone())
            .collect::<BTreeSet<_>>();
        if !result.is_disjoint(&sources) {
            result.retain(|p| !sources.contains(p));
            result.extend(transition.to.iter().map(|p| p.id.clone()));
        }
    }
    result
}

/// Validate a forward snapshot. Declarations may be committed after the business
/// change; every declaration must still match one real consecutive state.
pub(super) fn validate_step(
    before: &BTreeMap<String, NativePackage>,
    after: &BTreeMap<String, NativePackage>,
    transitions: &[Transition],
    consumed: &mut BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let mut expected = before.clone();
    let mut order = Vec::new();
    loop {
        let mut progress = false;
        for t in transitions
            .iter()
            .filter(|t| !consumed.contains(&t.id))
            .collect::<Vec<_>>()
        {
            let changed = t.from.iter().any(|p| {
                after.get(&p.id).is_none_or(|a| {
                    !matches(p, a)
                        || before
                            .get(&p.id)
                            .is_some_and(|b| b.workspace != a.workspace)
                })
            }) || t.to.iter().any(|p| !before.contains_key(&p.id));
            if !changed
                || !t
                    .from
                    .iter()
                    .all(|p| expected.get(&p.id).is_some_and(|a| matches(p, a)))
            {
                continue;
            }
            // Every endpoint must be witnessed. Never fabricate an intermediate
            // package or version from another declaration in the same commit.
            let mut replacements = Vec::new();
            for endpoint in &t.to {
                let native = after
                    .get(&endpoint.id)
                    .filter(|p| matches(endpoint, p))
                    .cloned();
                let Some(native) = native else {
                    replacements.clear();
                    break;
                };
                if let Some(inherited) = expected.get(&endpoint.id) {
                    if inherited.version != native.version {
                        return Err(format!(
                            "{}: lifecycle changes native version outside managed release",
                            t.id
                        ));
                    }
                }
                replacements.push((endpoint.id.clone(), native));
            }
            if replacements.len() != t.to.len() {
                continue;
            }
            for old in &t.from {
                expected.remove(&old.id);
            }
            for (id, native) in replacements {
                if expected.insert(id, native).is_some() {
                    return Err(format!(
                        "{}: lifecycle destination overwrites a live identity",
                        t.id
                    ));
                }
            }
            consumed.insert(t.id.clone());
            order.push(t.id.clone());
            progress = true;
        }
        if !progress {
            break;
        }
    }
    for (id, old) in &expected {
        let new = after.get(id).ok_or_else(|| {
            format!("{id}: package removed without a matching lifecycle declaration")
        })?;
        if old.manifest != new.manifest
            || old.name != new.name
            || old.language != new.language
            || old.workspace != new.workspace
        {
            return Err(format!(
                "{id}: package identity changed without a matching lifecycle declaration"
            ));
        }
        if old.version != new.version {
            return Err(format!(
                "{id}: native version changed outside managed release"
            ));
        }
    }
    Ok(order)
}

#[cfg(all(test, bridgeforge_factory_tests))]
#[path = "../../../../../scripts/tests/unit/core_release_lifecycle.rs"]
mod tests;
