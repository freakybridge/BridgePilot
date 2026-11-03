//! Build provenance compiled into both public executables. Receipts are evidence,
//! never the source of their own expected identity.
use serde_json::{Value, json};

pub fn identity() -> Value {
    json!({"protocol":1,
        "source_tree_sha256":option_env!("BRIDGEFORGE_BUILD_SOURCE_SHA"),
        "lockfile_sha256":option_env!("BRIDGEFORGE_BUILD_LOCK_SHA")})
}

pub fn require() -> Result<Value, String> {
    let value = identity();
    for field in ["source_tree_sha256", "lockfile_sha256"] {
        let valid = value[field].as_str().is_some_and(|s| s.len() == 71
            && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_hexdigit()));
        if !valid { return Err("runtime lacks compiled build provenance; prepare official public artifacts".into()); }
    }
    Ok(value)
}
