use super::*;

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
