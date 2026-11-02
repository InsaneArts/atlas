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
fn the_command_replaces_only_a_missing_dead_or_atlas_link() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let bin = root.join("bin");
    let executable = root.join("Applications/Atlas.app/Contents/MacOS/atlas");
    let older = root.join("Old/Atlas.app/Contents/MacOS/atlas");
    for path in [&executable, &older] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
    let link = bin.join("atlas");

    assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
    assert_eq!(
        link_command_in(&bin, &executable).unwrap()["changed"],
        false
    );

    for target in [older, root.join("gone")] {
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(link_command_in(&bin, &executable).unwrap()["changed"], true);
        assert_eq!(fs::canonicalize(&link).unwrap(), executable);
    }

    let other = root.join("other-atlas");
    fs::write(&other, b"another program").unwrap();
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&other, &link).unwrap();
    assert!(link_command_in(&bin, &executable).is_err());
    assert_eq!(fs::canonicalize(&link).unwrap(), other);

    fs::remove_file(&link).unwrap();
    fs::write(&link, b"a script").unwrap();
    assert!(link_command_in(&bin, &executable).is_err());
    assert_eq!(fs::read(&link).unwrap(), b"a script");
}
