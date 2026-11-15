use super::*;
use std::os::unix::fs::PermissionsExt;

/// A staged Atlas.app with one program and one plist, in a folder of its own.
fn staged() -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    let bundle = folder.path().join(BUNDLE);
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::write(bundle.join("Contents/MacOS/atlas"), "new").unwrap();
    std::fs::set_permissions(
        bundle.join("Contents/MacOS/atlas"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::write(bundle.join("Contents/Info.plist"), "plist").unwrap();
    std::fs::set_permissions(
        bundle.join("Contents/Info.plist"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    folder
}

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}

#[test]
fn a_bundle_is_made_whole_from_its_snapshot_with_modes_the_installer_chooses() {
    let source = staged();
    let bundle = source.path().join(BUNDLE);
    let manifest = Snapshot::of(&bundle).unwrap();
    let into = tempfile::tempdir().unwrap();
    let folder = File::open(into.path()).unwrap();

    copy_in(
        &manifest,
        &bundle,
        &folder,
        &FileName::new("Atlas.app").unwrap(),
        false,
    )
    .unwrap();

    let made = into.path().join(BUNDLE);
    assert_eq!(
        std::fs::read_to_string(made.join("Contents/MacOS/atlas")).unwrap(),
        "new"
    );
    assert_eq!(mode(&made.join("Contents/MacOS/atlas")), 0o755);
    assert_eq!(
        mode(&made.join("Contents/Info.plist")),
        0o644,
        "never writable by others"
    );
    assert_eq!(mode(&made.join("Contents")), 0o755);
}

#[test]
fn a_file_changed_after_its_snapshot_is_refused() {
    let source = staged();
    let bundle = source.path().join(BUNDLE);
    let manifest = Snapshot::of(&bundle).unwrap();
    std::fs::write(bundle.join("Contents/Info.plist"), "swapped after staging").unwrap();
    let into = tempfile::tempdir().unwrap();

    let copied = copy_in(
        &manifest,
        &bundle,
        &File::open(into.path()).unwrap(),
        &FileName::new("Atlas.app").unwrap(),
        false,
    );

    assert!(copied.is_err());
}

#[test]
fn a_link_out_of_the_bundle_is_refused_and_one_inside_it_is_made() {
    for (target, kept) in [
        ("MacOS/atlas", true),
        ("../../../etc/hosts", false),
        ("/etc/hosts", false),
        ("MacOS/../Info.plist", false),
    ] {
        let source = staged();
        let bundle = source.path().join(BUNDLE);
        std::os::unix::fs::symlink(target, bundle.join("Contents/Link")).unwrap();
        let manifest = Snapshot::of(&bundle).unwrap();
        let into = tempfile::tempdir().unwrap();

        let copied = copy_in(
            &manifest,
            &bundle,
            &File::open(into.path()).unwrap(),
            &FileName::new("Atlas.app").unwrap(),
            false,
        );

        assert_eq!(copied.is_ok(), kept, "{target}");
    }
}

#[test]
fn a_manifest_claiming_a_set_id_mode_is_refused_before_anything_is_made() {
    let source = staged();
    let bundle = source.path().join(BUNDLE);
    let honest = Snapshot::of(&bundle).unwrap();
    let mut entries = honest.entries.clone();
    for entry in &mut entries {
        if entry.path.last().is_some_and(|name| name == "atlas") {
            entry.mode = 0o4755;
        }
    }
    let forged = Snapshot { entries, ..honest };
    let into = tempfile::tempdir().unwrap();

    let copied = copy_in(
        &forged,
        &bundle,
        &File::open(into.path()).unwrap(),
        &FileName::new("Atlas.app").unwrap(),
        false,
    );

    assert!(copied.is_err());
    assert!(!into.path().join("Atlas.app").exists());
}

#[test]
fn only_one_atlas_app_folder_is_installed() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("Other.app")).unwrap();
    let manifest = Snapshot::of(&folder.path().join("Other.app")).unwrap();
    let into = tempfile::tempdir().unwrap();

    let copied = copy_in(
        &manifest,
        &folder.path().join("Other.app"),
        &File::open(into.path()).unwrap(),
        &FileName::new("Atlas.app").unwrap(),
        false,
    );

    assert!(copied.is_err());
}
