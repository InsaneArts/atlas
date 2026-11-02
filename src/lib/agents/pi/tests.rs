use super::*;
use serde_json::json;

#[test]
fn pi_resolves_local_sources_without_confusing_remote_packages() {
    let home = Path::new("/Users/person/.pi/agent");
    let person = Some(Path::new("/Users/person"));
    for (source, path) in [
        (
            "../../../Applications/Atlas.app/Contents/Resources/agents/atlas",
            "/Users/Applications/Atlas.app/Contents/Resources/agents/atlas",
        ),
        ("~/Applications/atlas", "/Users/person/Applications/atlas"),
        (
            "file:///Applications/Atlas%20Test.app/Contents/Resources/agents/atlas",
            "/Applications/Atlas Test.app/Contents/Resources/agents/atlas",
        ),
        ("file://localhost/Applications/atlas", "/Applications/atlas"),
    ] {
        assert_eq!(
            local_path(source, home, person),
            Some(PathBuf::from(path)),
            "{source}"
        );
    }
    for source in [
        "npm:atlas",
        "git:atlas",
        "github:a/atlas",
        "https://host/atlas",
        "ssh://host/atlas",
        "builtin:atlas",
        "git@host:atlas",
        "file://host/atlas",
    ] {
        assert_eq!(local_path(source, home, person), None, "{source}");
    }
}

#[test]
fn pi_reads_object_packages_and_preserves_their_exact_removal_source() {
    let home = Path::new("/config");
    let value = json!({"packages": ["npm:elsewhere", {"source": "../old/atlas", "extensions": []}, "../other"]});
    let entries = packages_in(&value.to_string(), home).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].stored, "../old/atlas");
    assert_eq!(entries[0].folder, Path::new("/old/atlas"));
    assert_eq!(source_of(&entries), Source::Elsewhere("/old/atlas".into()));
}

#[test]
fn pi_distinguishes_old_atlas_bundles_from_unrelated_packages() {
    let value = json!({"packages": ["/missing/Atlas.app/Contents/Resources/agents/atlas", "/someone/atlas"]});
    let entries = packages_in(&value.to_string(), Path::new("/config")).unwrap();
    assert!(owned(&entries[0]));
    assert!(!owned(&entries[1]));
    assert_eq!(
        source_of(&entries),
        Source::Elsewhere("/someone/atlas".into())
    );
    assert_eq!(
        source_of(&entries[..1]),
        Source::Folder("/missing/Atlas.app/Contents/Resources/agents".into())
    );
}

#[test]
fn pi_refuses_malformed_settings_and_treats_absence_as_empty() {
    for text in [
        "not json",
        "[]",
        r#"{"packages":false}"#,
        r#"{"packages":[{}]}"#,
    ] {
        assert!(packages_in(text, Path::new("/config")).is_err(), "{text}");
    }
    assert!(
        packages_in("\u{feff}{}", Path::new("/config"))
            .unwrap()
            .is_empty()
    );
    let temp = tempfile::tempdir().unwrap();
    assert!(packages(temp.path()).unwrap().is_empty());
    fs::write(temp.path().join("settings.json"), "{").unwrap();
    assert!(
        packages(temp.path())
            .unwrap_err()
            .error
            .contains("settings.json")
    );
}
