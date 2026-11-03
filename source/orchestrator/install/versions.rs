//! Three product versions; Cargo package versions remain native build identities.
use crate::Result;
use bridgeforge_core::SemVer;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const BP_VERSION: &str = include_str!("../../../VERSION");

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub version: String,
    pub delivery: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub forge: Component,
    pub orchestrator: Component,
}

impl Components {
    pub fn validate(&self) -> Result<()> {
        SemVer::from_str(&self.forge.version)?;
        SemVer::from_str(&self.orchestrator.version)?;
        if self.forge.delivery != "source-reference" || self.orchestrator.delivery != "embedded" {
            return Err("unsupported component delivery identity".into());
        }
        Ok(())
    }
}

pub fn compiled() -> Result<Components> {
    SemVer::from_str(BP_VERSION.trim())?;
    let forge = bridgeforge_core::product_version::explicit(include_str!("../../forge/cli/Cargo.toml"))?;
    let manifest = include_str!("../cli/Cargo.toml").parse::<toml_edit::DocumentMut>()
        .map_err(|e| e.to_string())?;
    let orchestrator = manifest["package"]["version"].as_str().ok_or("orchestrator version missing")?;
    let components = Components {
        forge: Component { version: forge, delivery: "source-reference".into() },
        orchestrator: Component { version: orchestrator.into(), delivery: "embedded".into() },
    };
    components.validate()?;
    Ok(components)
}
