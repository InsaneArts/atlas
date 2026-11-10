use super::*;
use crate::environment::{
    MAX_FILE,
    tests::{Sandbox, foreign_provenance},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::BTreeMap, os::unix::fs::symlink};

#[test]
fn unsupported_metadata_and_links_are_refused_without_modification() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    fs::write(&path, b"same").unwrap();
    fs::hard_link(&path, sandbox.home.join("alias")).unwrap();
    assert!(snapshot(&path).is_err());

    fs::remove_file(sandbox.home.join("alias")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o4600)).unwrap();
    assert!(snapshot(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"same");
}

#[test]
fn aliases_reserved_state_and_special_files_are_rejected() {
    for target in [
        ".config//atlas/provisioning/lock",
        ".CONFIG/atlas/./PROVISIONING/files.json",
        "../escape",
    ] {
        assert!(unique_targets([Path::new(target)].into_iter()).is_err());
    }

    assert!(
        unique_targets([Path::new(".config/Tool"), Path::new(".CONFIG/tool")].into_iter()).is_err()
    );
    assert!(unique_targets([Path::new("tool"), Path::new("tool/child")].into_iter()).is_err());

    let sandbox = Sandbox::new();
    let path = sandbox.root.join("data");
    fs::write(&path, b"too long").unwrap();
    assert!(read_limited(&path, 2).is_err());

    symlink(&path, sandbox.root.join("link")).unwrap();
    assert!(read_limited(&sandbox.root.join("link"), MAX_FILE).is_err());

    symlink(&sandbox.root, sandbox.home.join("escape")).unwrap();
    assert!(atomic_install(&sandbox.home.join("escape/file"), b"data", 0o600, true).is_err());
    assert!(atomic_install(&path, b"replacement", 0o600, true).is_err());
    assert_eq!(fs::read(path).unwrap(), b"too long");
}

#[test]
#[cfg(target_os = "macos")]
fn an_existing_acl_is_refused_without_removing_its_entries() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("acl-file");
    atomic_write(&path, b"owned test data", 0o600).unwrap();

    assert!(
        atlas_core::spawn::status(
            std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&path)
        )
        .unwrap()
        .success()
    );
    assert!(has_acl(&path).unwrap());
    assert!(snapshot(&path).is_err());
    assert!(has_acl(&path).unwrap());
    assert_eq!(fs::read(path).unwrap(), b"owned test data");
    assert!(!sandbox.home.join(STATE).exists());
}

#[test]
#[cfg(target_os = "macos")]
fn an_inherited_folder_acl_is_cleared() {
    let sandbox = Sandbox::new();
    let folder = sandbox.root.join("stage");
    fs::create_dir(&folder).unwrap();

    assert!(
        atlas_core::spawn::status(
            std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&folder)
        )
        .unwrap()
        .success()
    );
    assert!(has_acl(&folder).unwrap());
    remove_inherited_acl(&folder).unwrap();
    assert!(!has_acl(&folder).unwrap());
    assert!(remove_inherited_acl(&sandbox.root.join("missing")).is_err());
}

/// macOS sets com.apple.provenance itself and ignores attempts to change it,
/// so restores leave it alone and fingerprints do not compare it.
#[test]
fn provenance_is_never_restored_or_compared() {
    let sandbox = Sandbox::new();
    let path = sandbox.home.join("file");
    atomic_write(&path, b"data", 0o600).unwrap();

    restore_attributes(&path, &foreign_provenance()).unwrap();
    restore_attributes(&path, &BTreeMap::new()).unwrap();

    let own = fingerprint(&path).unwrap().unwrap();
    let mut written_by_an_app = own.clone();
    written_by_an_app.xattrs = foreign_provenance();
    assert_eq!(own, written_by_an_app);

    written_by_an_app
        .xattrs
        .insert("user.atlas-test".into(), STANDARD.encode([1_u8]));
    assert_ne!(own, written_by_an_app, "other attributes still count");
}
