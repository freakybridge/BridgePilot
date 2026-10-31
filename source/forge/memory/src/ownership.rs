use super::{MemoryResult, MemorySyncError, atomic_write};
use serde::de::{Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MANAGED_ID_KEY: &str = "bridgeforgeCodexId";
pub type ManagedLooking<'a> = Option<&'a dyn Fn(&Map<String, Value>) -> bool>;

#[derive(Clone, Debug, PartialEq)]
pub struct ExpectedGroup {
    pub id: String,
    pub event: String,
    pub matcher: String,
    pub handler_sha256: String,
    pub handler: Value,
    pub group: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnershipReceipt {
    pub id: String,
    pub event: String,
    pub matcher: String,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalizedHooks {
    pub document: Value,
    pub external: Value,
    pub receipts: Vec<OwnershipReceipt>,
}

pub enum ExpectedHooksState<'a> {
    Any,
    Missing,
    Exact(&'a [u8]),
}

struct CheckedValue(Value);

impl<'de> Deserialize<'de> for CheckedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(CheckedValueVisitor)
    }
}

struct CheckedValueVisitor;

impl<'de> Visitor<'de> for CheckedValueVisitor {
    type Value = CheckedValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .map(CheckedValue)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Null))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(CheckedValue(Value::Null))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        CheckedValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<CheckedValue>()? {
            values.push(value.0);
        }
        Ok(CheckedValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut value = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if value.contains_key(&key) {
                return Err(A::Error::custom(format!("duplicate JSON key: {key}")));
            }
            value.insert(key, map.next_value::<CheckedValue>()?.0);
        }
        Ok(CheckedValue(Value::Object(value)))
    }
}

pub fn load_json_value(payload: &[u8], label: &str) -> MemoryResult<Value> {
    let payload = payload.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(payload);
    let text = std::str::from_utf8(payload)
        .map_err(|error| MemorySyncError::new(format!("invalid hooks JSON: {label}: {error}")))?;
    serde_json::from_str::<CheckedValue>(text)
        .map(|value| value.0)
        .map_err(|error| MemorySyncError::new(format!("invalid hooks JSON: {label}: {error}")))
}

pub fn load_json_object(payload: &[u8], label: &str) -> MemoryResult<Value> {
    let value = load_json_value(payload, label)?;
    if !value.is_object() {
        return Err(MemorySyncError::new(format!(
            "JSON root must be an object: {label}"
        )));
    }
    Ok(value)
}

pub fn load_document(payload: &[u8], label: &str) -> MemoryResult<Value> {
    let value = load_json_object(payload, label)?;
    if !value.get("hooks").is_some_and(Value::is_object) {
        return Err(MemorySyncError::new(format!(
            "hooks JSON has no hooks object: {label}"
        )));
    }
    Ok(value)
}

pub fn canonical_json_sha256(value: &Value) -> MemoryResult<String> {
    let canonical = canonical_value(value);
    let payload = serde_json::to_vec(&canonical)?;
    let digest = Sha256::digest(payload);
    Ok(format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

pub fn render_document(value: &Value) -> MemoryResult<Vec<u8>> {
    let mut payload = serde_json::to_vec_pretty(value)?;
    payload.push(b'\n');
    Ok(payload)
}

pub fn expected_groups(document: &Value, managed_prefix: &str) -> MemoryResult<Vec<ExpectedGroup>> {
    let hooks = document
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or_else(|| MemorySyncError::new("canonical hooks document has no hooks object"))?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (event, groups) in hooks {
        let groups = groups.as_array().ok_or_else(|| {
            MemorySyncError::new("canonical hooks document has invalid event groups")
        })?;
        for group in groups {
            let group_object = group.as_object().ok_or_else(|| {
                MemorySyncError::new("canonical hooks document has invalid matcher group")
            })?;
            let matcher = group_object
                .get("matcher")
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        MemorySyncError::new("canonical hooks document has invalid matcher")
                    })
                })
                .transpose()?
                .unwrap_or("");
            let handlers = group_object
                .get("hooks")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    MemorySyncError::new("canonical hooks document has invalid matcher group")
                })?;
            let marked: Vec<&Value> = handlers
                .iter()
                .filter(|handler| {
                    handler
                        .get(MANAGED_ID_KEY)
                        .and_then(Value::as_str)
                        .is_some_and(|id| id.starts_with(managed_prefix))
                })
                .collect();
            if marked.is_empty() {
                continue;
            }
            if handlers.len() != 1 || marked.len() != 1 {
                return Err(MemorySyncError::new(
                    "canonical managed hook group must contain exactly one handler",
                ));
            }
            let handler = marked[0];
            let id = handler[MANAGED_ID_KEY].as_str().unwrap().to_string();
            if !seen.insert(id.clone()) {
                return Err(MemorySyncError::new(format!(
                    "duplicate canonical managed id: {id}"
                )));
            }
            result.push(ExpectedGroup {
                id,
                event: event.clone(),
                matcher: matcher.to_string(),
                handler_sha256: canonical_json_sha256(handler)?,
                handler: handler.clone(),
                group: group.clone(),
            });
        }
    }
    if result.is_empty() {
        return Err(MemorySyncError::new(
            "canonical hooks document has no managed handlers",
        ));
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub fn canonicalize(
    document: &Value,
    expected: &[ExpectedGroup],
    managed_prefixes: &[&str],
    label: &str,
    managed_looking: ManagedLooking<'_>,
    replace_marked_drift: bool,
    managed_top_level: Option<&Map<String, Value>>,
) -> MemoryResult<CanonicalizedHooks> {
    validate_shape(document, label)?;
    let expected_by_id: BTreeMap<&str, &ExpectedGroup> = expected
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    if expected_by_id.len() != expected.len() {
        return Err(MemorySyncError::new(
            "expected hooks contain duplicate managed ids",
        ));
    }

    let mut external = document.clone();
    let external_hooks = external["hooks"].as_object_mut().unwrap();
    let original_hooks = document["hooks"].as_object().unwrap();
    let mut receipts = Vec::new();
    let mut found: BTreeMap<String, usize> = BTreeMap::new();

    for (event, groups) in original_hooks {
        let mut external_groups = Vec::new();
        for group in groups.as_array().unwrap() {
            let object = group.as_object().unwrap();
            let matcher = object.get("matcher").and_then(Value::as_str).unwrap_or("");
            let handlers = object["hooks"].as_array().unwrap();
            let mut kept_handlers = Vec::new();
            let mut managed_count = 0;
            for handler in handlers {
                let handler_object = handler.as_object().unwrap();
                let digest = canonical_json_sha256(handler)?;
                let raw_id = handler_object.get(MANAGED_ID_KEY).and_then(Value::as_str);
                let managed_id = raw_id.filter(|id| expected_by_id.contains_key(*id));
                if managed_id.is_none()
                    && raw_id.is_some_and(|id| {
                        managed_prefixes.iter().any(|prefix| id.starts_with(prefix))
                    })
                {
                    return Err(MemorySyncError::new(format!(
                        "unknown managed hook id: {}: {label}",
                        raw_id.unwrap()
                    )));
                }
                let Some(managed_id) = managed_id else {
                    if managed_looking.is_some_and(|predicate| predicate(handler_object)) {
                        return Err(MemorySyncError::new(format!(
                            "managed-looking handler has no trusted ownership: {event}/{matcher}: {label}"
                        )));
                    }
                    kept_handlers.push(handler.clone());
                    continue;
                };
                let spec = expected_by_id[managed_id];
                if event != &spec.event || matcher != spec.matcher {
                    return Err(MemorySyncError::new(format!(
                        "managed hook is registered in the wrong group: {managed_id}: {label}"
                    )));
                }
                if digest != spec.handler_sha256 && !replace_marked_drift {
                    return Err(MemorySyncError::new(format!(
                        "managed hook content drifted: {managed_id}: {label}"
                    )));
                }
                *found.entry(managed_id.to_string()).or_default() += 1;
                managed_count += 1;
                receipts.push(OwnershipReceipt {
                    id: managed_id.to_string(),
                    event: event.clone(),
                    matcher: matcher.to_string(),
                    action: "canonicalize".to_string(),
                });
            }
            if !kept_handlers.is_empty() {
                let mut kept_group = group.clone();
                kept_group
                    .as_object_mut()
                    .unwrap()
                    .insert("hooks".to_string(), Value::Array(kept_handlers));
                external_groups.push(kept_group);
            } else if managed_count == 0 {
                external_groups.push(group.clone());
            }
        }
        if external_groups.is_empty() {
            external_hooks.remove(event);
        } else {
            external_hooks.insert(event.clone(), Value::Array(external_groups));
        }
    }

    let mut canonical = external.clone();
    let canonical_hooks = canonical["hooks"].as_object_mut().unwrap();
    for spec in expected {
        let bucket = canonical_hooks
            .entry(spec.event.clone())
            .or_insert_with(|| Value::Array(Vec::new()));
        let bucket = bucket.as_array_mut().ok_or_else(|| {
            MemorySyncError::new(format!("invalid canonical event: {}: {label}", spec.event))
        })?;
        bucket.push(spec.group.clone());
        if found.get(&spec.id).copied().unwrap_or(0) == 0 {
            receipts.push(OwnershipReceipt {
                id: spec.id.clone(),
                event: spec.event.clone(),
                matcher: spec.matcher.clone(),
                action: "add-missing".to_string(),
            });
        }
    }
    if let Some(fields) = managed_top_level {
        for (key, value) in fields {
            if document.get(key).is_some_and(|current| current != value) {
                return Err(MemorySyncError::new(format!(
                    "managed top-level field has no trusted ownership: {key}: {label}"
                )));
            }
            canonical
                .as_object_mut()
                .unwrap()
                .insert(key.clone(), value.clone());
            external.as_object_mut().unwrap().remove(key);
        }
    }
    Ok(CanonicalizedHooks {
        document: canonical,
        external,
        receipts,
    })
}

pub fn merge_managed_document(
    payload: Option<&[u8]>,
    expected: &[ExpectedGroup],
    managed_prefixes: &[&str],
    label: &str,
    managed_looking: ManagedLooking<'_>,
) -> MemoryResult<Vec<u8>> {
    let document = match payload {
        Some(payload) => load_document(payload, label)?,
        None => serde_json::json!({"hooks": {}}),
    };
    let canonical = canonicalize(
        &document,
        expected,
        managed_prefixes,
        label,
        managed_looking,
        false,
        None,
    )?;
    render_document(&canonical.document)
}

pub fn managed_document_healthy(
    payload: &[u8],
    expected: &[ExpectedGroup],
    managed_prefixes: &[&str],
    label: &str,
    managed_looking: ManagedLooking<'_>,
) -> bool {
    let Ok(document) = load_document(payload, label) else {
        return false;
    };
    canonicalize(
        &document,
        expected,
        managed_prefixes,
        label,
        managed_looking,
        false,
        None,
    )
    .is_ok_and(|canonical| canonical.document == document)
}

pub fn merge_hooks_file(
    path: &std::path::Path,
    expected: &[ExpectedGroup],
    managed_prefixes: &[&str],
    managed_looking: ManagedLooking<'_>,
    expected_before: ExpectedHooksState<'_>,
) -> MemoryResult<bool> {
    let initial = if path.is_file() {
        Some(std::fs::read(path)?)
    } else {
        None
    };
    let cas_matches = match expected_before {
        ExpectedHooksState::Any => true,
        ExpectedHooksState::Missing => initial.is_none(),
        ExpectedHooksState::Exact(expected) => initial.as_deref() == Some(expected),
    };
    if !cas_matches {
        return Err(MemorySyncError::new(
            "user hooks changed before the locked CAS",
        ));
    }
    let desired = merge_managed_document(
        initial.as_deref(),
        expected,
        managed_prefixes,
        &path.display().to_string(),
        managed_looking,
    )?;
    if initial.as_deref() == Some(desired.as_slice()) {
        return Ok(false);
    }
    let current = if path.is_file() {
        Some(std::fs::read(path)?)
    } else {
        None
    };
    if current != initial {
        return Err(MemorySyncError::new(
            "user hooks changed during the locked CAS",
        ));
    }
    atomic_write(path, &desired)?;
    Ok(true)
}

pub fn hooks_file_healthy(
    path: &std::path::Path,
    expected: &[ExpectedGroup],
    managed_prefixes: &[&str],
    managed_looking: ManagedLooking<'_>,
) -> bool {
    std::fs::read(path).is_ok_and(|payload| {
        managed_document_healthy(
            &payload,
            expected,
            managed_prefixes,
            &path.display().to_string(),
            managed_looking,
        )
    })
}

fn validate_shape(document: &Value, label: &str) -> MemoryResult<()> {
    let hooks = document
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or_else(|| MemorySyncError::new(format!("hooks JSON has no hooks object: {label}")))?;
    for (event, groups) in hooks {
        let groups = groups.as_array().ok_or_else(|| {
            MemorySyncError::new(format!("invalid hook groups: {event}: {label}"))
        })?;
        for group in groups {
            let group = group.as_object().ok_or_else(|| {
                MemorySyncError::new(format!("invalid matcher group: {event}: {label}"))
            })?;
            if group
                .get("matcher")
                .is_some_and(|matcher| !matcher.is_string())
            {
                return Err(MemorySyncError::new(format!(
                    "invalid matcher: {event}: {label}"
                )));
            }
            let handlers = group
                .get("hooks")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    MemorySyncError::new(format!("invalid matcher group: {event}: {label}"))
                })?;
            if handlers.iter().any(|handler| !handler.is_object()) {
                return Err(MemorySyncError::new(format!(
                    "invalid hook handler: {event}: {label}"
                )));
            }
        }
    }
    Ok(())
}

fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<&String, &Value> = object.iter().collect();
            let mut result = Map::new();
            for (key, value) in sorted {
                result.insert(key.clone(), canonical_value(value));
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_value).collect()),
        _ => value.clone(),
    }
}
