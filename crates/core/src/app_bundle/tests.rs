use super::*;

#[test]
fn installed_application_identity_uses_the_fixed_path_and_refuses_unreachable_builds() {
    let expected = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
    let executable = app_executable_using(|path| {
        assert_eq!(path, expected);
        true
    })
    .unwrap();
    assert_eq!(executable, expected);

    let error = app_executable_using(|path| {
        assert_eq!(path, expected);
        false
    })
    .unwrap_err();
    assert_eq!(error.code, "init");
    assert_eq!(
        error.error,
        "Atlas.app must be in /Applications so the helper account can run it"
    );
    assert!(!error.uncertain);
}

#[test]
fn the_bundle_ships_the_app_beside_atlas() {
    let dir = tempfile::tempdir().unwrap();
    let macos = dir.path().join("Atlas.app/Contents/MacOS");
    fs::create_dir_all(&macos).unwrap();
    let executable = macos.join("atlas");
    assert_eq!(bundled(&executable), None, "no app yet");
    fs::write(macos.join(APP_EXECUTABLE), b"").unwrap();
    assert_eq!(bundled(&executable), Some(macos.join(APP_EXECUTABLE)));

    let loose = dir.path().join("bin");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join(APP_EXECUTABLE), b"").unwrap();
    assert_eq!(bundled(&loose.join("atlas")), None, "only inside the app");

    fs::remove_file(macos.join(APP_EXECUTABLE)).unwrap();
    std::os::unix::fs::symlink(loose.join(APP_EXECUTABLE), macos.join(APP_EXECUTABLE)).unwrap();
    assert_eq!(bundled(&executable), None, "never through a link");
    assert!(
        !app_signed(&loose.join(APP_EXECUTABLE)),
        "an unsigned file is not the app"
    );
}

/// Artifact qualification, independent of the source checkout's bundle fixtures.
#[test]
#[ignore = "requires an installed Atlas.app signed by the team this build trusts"]
fn the_installed_app_meets_this_builds_signing_requirement() {
    let installed = Path::new("/Applications/Atlas.app/Contents/MacOS").join(APP_EXECUTABLE);
    assert!(app_signed(&installed));
}
