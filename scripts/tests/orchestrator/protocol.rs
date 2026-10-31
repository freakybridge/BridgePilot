use super::*;
#[test]
fn executable_and_workspace_are_explicit_and_no_shell_fallback_exists() {
    assert!(Rpc::start(Path::new("codex"), Path::new(".")).is_err());
    let root = std::env::current_dir().unwrap();
    assert!(Rpc::start(&root.join("missing-codex.exe"), &root).is_err());
}
