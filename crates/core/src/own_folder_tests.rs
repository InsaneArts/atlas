use super::*;

/// A home of its own, by its real path, with Atlas's own folder in it.
fn home() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(directory.path()).unwrap();
    fs::create_dir_all(home.join(FOLDER)).unwrap();
    (directory, home)
}

fn refused(path: &Path, home: &Path) -> bool {
    match refuse_in(path, home) {
        Ok(()) => false,
        Err(error) => {
            assert_eq!(error.code, Code::Credentials);
            assert_eq!(error.error, REFUSAL);
            true
        }
    }
}

#[test]
fn a_path_inside_atlas_own_folder_is_refused_whether_or_not_it_exists() {
    let (_directory, home) = home();
    let share = home.join(FOLDER).join("VMs/0123456789abcdef01234567/Share");
    fs::create_dir_all(&share).unwrap();
    fs::write(share.join("pairing-secret"), b"secret").unwrap();

    assert!(refused(&share.join("pairing-secret"), &home));
    assert!(refused(&share.join("not-made-yet"), &home));
    assert!(refused(
        &home.join(FOLDER).join("Station/vm-1/keeper"),
        &home
    ));
}

#[test]
fn atlas_own_folder_itself_is_refused_however_it_is_written() {
    let (_directory, home) = home();

    assert!(refused(&home.join(FOLDER), &home));
    assert!(refused(
        &home.join("Library/application support/ATLAS/runs"),
        &home
    ));
    assert!(refused(
        &home.join("Documents/../Library/./Application Support/Atlas"),
        &home
    ));
    let through_data = Path::new(DATA).join(home.strip_prefix("/").unwrap());
    assert!(refused(
        &through_data.join(FOLDER).join("sign-ins.json"),
        &home
    ));
}

#[test]
fn a_folder_that_holds_atlas_own_folder_is_refused() {
    let (_directory, home) = home();

    for holding in [
        home.join("Library/Application Support"),
        home.join("Library"),
        home.clone(),
        PathBuf::from("/"),
    ] {
        assert!(refused(&holding, &home), "{}", holding.display());
    }
}

#[test]
fn a_link_that_leads_into_atlas_own_folder_is_refused() {
    let (_directory, home) = home();
    let secret = home.join(FOLDER).join("pairing-secret");
    fs::write(&secret, b"secret").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let to_file = elsewhere.path().join("notes.txt");
    std::os::unix::fs::symlink(&secret, &to_file).unwrap();
    let to_folder = elsewhere.path().join("folder");
    std::os::unix::fs::symlink(home.join(FOLDER), &to_folder).unwrap();
    let to_library = elsewhere.path().join("library");
    std::os::unix::fs::symlink(home.join("Library"), &to_library).unwrap();

    assert!(refused(&to_file, &home));
    assert!(refused(&to_folder.join("pairing-secret"), &home));
    assert!(refused(&to_folder.join("missing"), &home));
    assert!(refused(&to_library, &home), "it holds the folder");
}

#[test]
fn atlas_own_folder_kept_through_a_link_is_refused_where_it_really_is() {
    let directory = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(directory.path()).unwrap();
    let real = home.join("Elsewhere/Atlas");
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(home.join("Library/Application Support")).unwrap();
    std::os::unix::fs::symlink(&real, home.join(FOLDER)).unwrap();

    assert!(refused(&real.join("VMs"), &home));
    assert!(refused(&home.join("Elsewhere"), &home));
}

#[test]
fn a_path_outside_atlas_own_folder_is_let_through() {
    let (_directory, home) = home();
    let settings = home.join(".config/atlas.toml");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, b"").unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let link = elsewhere.path().join("settings");
    std::os::unix::fs::symlink(&settings, &link).unwrap();

    for allowed in [
        settings,
        link,
        home.join("Library/Application Support/Atlas Notes/plan.txt"),
        home.join("Library/Application Support/Other/Atlas"),
        home.join("Documents/project/.env.example"),
    ] {
        assert!(!refused(&allowed, &home), "{}", allowed.display());
    }
}

#[test]
fn atlas_sockets_folder_outside_the_home_and_anything_in_it_or_holding_it_are_refused() {
    let (_home, home) = home();
    let sockets = sockets().unwrap();

    assert!(sockets.as_os_str().len() < 64, "{}", sockets.display());
    for path in [
        sockets.clone(),
        sockets.join("0123456789abcdef01234567.keeper"),
        sockets.join("frames/new"),
        sockets.parent().unwrap().to_path_buf(),
    ] {
        assert!(refused(&path, &home), "{}", path.display());
    }
    let elsewhere = tempfile::tempdir().unwrap();
    let link = elsewhere.path().join("user");
    std::os::unix::fs::symlink(sockets.parent().unwrap(), &link).unwrap();
    assert!(refused(&link.join("atlas/frames"), &home));
    assert!(!refused(&elsewhere.path().join("frames"), &home));
}
