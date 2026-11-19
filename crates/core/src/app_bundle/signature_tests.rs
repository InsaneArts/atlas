use super::*;
/// The installer verifies downloads against exactly setup's requirement.
#[test]
fn the_installer_checks_the_same_requirement() {
    let script = include_str!("../../../../install.sh");
    let line = script
        .lines()
        .find_map(|line| line.strip_prefix("REQUIREMENT=\""))
        .and_then(|line| line.strip_suffix('"'))
        .expect("install.sh sets REQUIREMENT");

    let expanded = line
        .replace("\\\"", "\"")
        .replace("$IDENTIFIER", IDENTIFIER)
        .replace("$TEAM", TEAM);
    assert_eq!(expanded, requirement_for(IDENTIFIER));
    assert!(script.contains(&format!("IDENTIFIER=\"{IDENTIFIER}\"")));
    assert!(script.contains(&format!("TEAM=\"{RELEASE_TEAM}\"")));
}

/// Only Atlas signed by its own Developer ID team passes, as a file
/// or as a running process.
#[test]
fn signatures_are_checked_against_atlass_identity() {
    assert!(
        !developer_id(Path::new("/bin/ls")),
        "Apple-signed, not ours"
    );

    let test = std::env::current_exe().unwrap();
    assert!(!developer_id(&test), "ad-hoc signed test build");
    assert!(
        !running_developer_id(std::process::id()),
        "this ad-hoc test process is not Atlas's Developer ID"
    );
    assert!(
        !runs_current(std::process::id(), &test),
        "only Atlas's own signed build is current"
    );
    assert!(!runs_current(std::process::id(), Path::new("/bin/ls")));
}

#[test]
fn only_the_apps_own_executable_signed_as_atlas_runs_the_app() {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("Atlas.app");
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    let test = std::env::current_exe().unwrap();
    std::os::unix::fs::symlink(&test, macos.join(super::super::APP_EXECUTABLE)).unwrap();

    assert!(
        !runs_app(std::process::id(), &app),
        "its path is the app's, but its code is not signed as Atlas"
    );
    assert!(!runs_app(std::process::id(), folder.path()));
}

#[test]
fn codesign_diagnostics_parse_without_inventing_fields() {
    let text = "Executable=/x\nIdentifier=com.example.sessiond\nCDHash=0c61eed3\n\
                    Signature=adhoc\nTeamIdentifier=not set\n";
    assert_eq!(
        parse_signature(text),
        Signature {
            identifier: Some("com.example.sessiond".into()),
            cdhash: Some("0c61eed3".into()),
            kind: Some("adhoc".into()),
            team: None,
        }
    );
    assert_eq!(parse_signature(""), Signature::default());
}
