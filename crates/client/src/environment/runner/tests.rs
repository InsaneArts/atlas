use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn profile_file_is_private_bounded_and_never_evaluated_as_shell() {
    let root = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(root.path()).unwrap();
    let path = home.join(".config/atlas/environment.json");
    let literal = "$(touch should-not-run); private value";
    atomic_write(
        &path,
        &serde_json::to_vec(&json!({"version": 1, "variables": {"SELECTED": literal}})).unwrap(),
        0o600,
    )
    .unwrap();

    assert_eq!(load(&home).unwrap()["SELECTED"], literal);
    assert!(!home.join("should-not-run").exists());

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load(&home).is_err());

    for name in [
        "HOME", "USER", "LOGNAME", "SHELL", "PATH", "UID", "EUID", "TMPDIR", "1BAD", "BAD-NAME",
    ] {
        atomic_write(
            &path,
            &serde_json::to_vec(&json!({"version": 1, "variables": {name: "secret"}})).unwrap(),
            0o600,
        )
        .unwrap();
        let error = load(&home).unwrap_err();
        assert!(!error.error.contains("secret"));
    }

    assert!(validate_argv(&[]).is_err());
    assert!(validate_argv(&["tool\0name".into()]).is_err());
}

#[test]
fn identity_is_rechecked_after_loading_profile_before_exec() {
    let root = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(root.path()).unwrap();
    atomic_write(
        &home.join(".config/atlas/environment.json"),
        b"{\"version\":1,\"variables\":{}}",
        0o600,
    )
    .unwrap();

    let error = execute(&home, &["/usr/bin/true".into()], &|| {
        Err(problem("session changed"))
    })
    .unwrap_err();
    assert_eq!(error.error, "session changed");
}
