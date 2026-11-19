use super::*;

fn launched_from(path: &std::path::Path) -> Launched {
    let started = std::fs::metadata(path).unwrap();
    Launched::new(path.to_owned(), started.dev(), started.ino())
}

#[test]
fn a_program_is_replaced_once_its_launch_path_names_another_file_or_none() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas");
    std::fs::write(&path, b"A").unwrap();
    let launched = launched_from(&path);
    assert!(!launched.replaced(), "the file it was started from");

    // An update swaps the bundle: the same path names the new build's file.
    let staged = folder.path().join("atlas.new");
    std::fs::write(&staged, b"B").unwrap();
    std::fs::rename(&staged, &path).unwrap();
    assert!(launched.replaced(), "another file at its path");

    std::fs::remove_file(&path).unwrap();
    assert!(launched.replaced(), "no file at its path");
}

#[test]
fn a_program_whose_file_cannot_be_told_is_never_taken_for_one_not_replaced() {
    let unknown = Program::default();

    assert_eq!(unknown.replaced(), None);
    assert!(!unknown.signed());
    assert_eq!(unknown.said()["replaced"], Value::Null);
}

#[test]
fn this_keepers_own_program_is_read_whole_at_its_start() {
    let own = Program::own();

    assert!(own.signed());
    assert_eq!(own.replaced(), Some(false));
    assert_eq!(own.said()["code"].as_str().map(str::len), Some(40));
}

#[test]
fn this_keepers_build_is_the_digest_of_the_very_file_its_image_was_mapped_from() {
    let own = Program::own();

    assert_eq!(
        own.build().digest,
        atlas_native::native::service::own_build()
    );
    assert_eq!(own.said()["build"].as_str().map(str::len), Some(64));
    assert_eq!(own.build().version.as_deref(), Some(atlas_core::VERSION));
}

#[test]
fn a_launch_path_that_names_another_file_than_the_image_gives_no_build() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas");
    std::fs::write(&path, b"A").unwrap();
    let launched = launched_from(&path);
    assert!(launched.digest().is_some(), "the image's own file");

    std::fs::write(folder.path().join("atlas.new"), b"B").unwrap();
    std::fs::rename(folder.path().join("atlas.new"), &path).unwrap();

    assert_eq!(launched.digest(), None);
}

/// An app bundle whose executable is a copy of a signed program.
fn signed_bundle() -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let app = folder.path().join("Atlas.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::write(app.join("Contents/Info.plist"), b"<plist/>").unwrap();
    let executable = app.join("Contents/MacOS/atlas");
    std::fs::copy("/usr/bin/true", &executable).unwrap();
    (folder, executable)
}

#[test]
fn a_keepers_own_bundle_is_held_only_while_its_executable_is_the_image_it_runs() {
    let (_folder, executable) = signed_bundle();
    let program = Program::signed_at(&executable);

    let (bundle, held) = program.bundle().unwrap();

    assert_eq!(
        bundle,
        std::fs::canonicalize(executable.ancestors().nth(3).unwrap()).unwrap()
    );
    assert!(
        held.files()
            .any(|(entry, _, _)| entry.path == ["Contents", "MacOS", "atlas"])
    );
    // The same file, other bytes: not the build it read at its start.
    std::fs::write(&executable, b"#!/bin/sh\n").unwrap();
    assert_eq!(program.bundle().unwrap_err(), Unheld::NotOwn);
}

#[test]
fn a_keeper_whose_file_was_replaced_or_cannot_be_told_never_holds_a_bundle() {
    let (_folder, executable) = signed_bundle();
    let program = Program::signed_at(&executable);
    let other = executable.with_file_name("atlas.new");
    std::fs::copy("/usr/bin/true", &other).unwrap();
    std::fs::rename(&other, &executable).unwrap();

    assert_eq!(program.bundle().unwrap_err(), Unheld::Replaced);
    assert_eq!(Program::default().bundle().unwrap_err(), Unheld::Replaced);
    assert_eq!(Program::built("b1").bundle().unwrap_err(), Unheld::Replaced);
}

#[test]
fn a_program_not_in_an_app_bundle_or_unsigned_holds_none() {
    let folder = tempfile::tempdir().unwrap();
    let loose = folder.path().join("atlas");
    std::fs::copy("/usr/bin/true", &loose).unwrap();
    assert_eq!(
        Program::signed_at(&loose).bundle().unwrap_err(),
        Unheld::NotOwn
    );

    let (_bundle, executable) = signed_bundle();
    let mut unsigned = Program::signed_at(&executable);
    unsigned.code = Some("00".repeat(20));
    assert_eq!(unsigned.bundle().unwrap_err(), Unheld::NotOwn);
}
