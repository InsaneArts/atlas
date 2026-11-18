use super::*;

/// An Applications folder of the test's own.
fn applications() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// The names an `installed` read asks after.
fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// `folder` alone, as the folders looked in.
fn only(folder: &Path) -> Vec<PathBuf> {
    vec![folder.to_owned()]
}

#[test]
fn a_real_app_folder_is_installed() {
    let folder = applications();
    std::fs::create_dir_all(folder.path().join("Firefox.app/Contents")).unwrap();

    let installed =
        in_applications(&only(folder.path()), &names(&["Firefox", "Google Chrome"])).unwrap();

    assert_eq!(
        installed,
        BTreeMap::from([
            ("Firefox".to_owned(), true),
            ("Google Chrome".to_owned(), false)
        ])
    );
}

#[test]
fn an_app_in_either_folder_is_installed() {
    let (system, own) = (applications(), applications());
    std::fs::create_dir(system.path().join("Firefox.app")).unwrap();
    std::fs::create_dir(own.path().join("Probe.app")).unwrap();
    let both = vec![system.path().to_owned(), own.path().to_owned()];

    let installed = in_applications(&both, &names(&["Firefox", "Probe", "Notes"])).unwrap();

    assert!(installed["Firefox"]);
    assert!(installed["Probe"]);
    assert!(!installed["Notes"]);
}

#[test]
fn a_linked_app_pointing_outside_applications_is_not_installed_in_either_folder() {
    let (system, own, elsewhere) = (applications(), applications(), applications());
    let app = elsewhere.path().join("Safari.app");
    std::fs::create_dir(&app).unwrap();
    std::os::unix::fs::symlink(&app, system.path().join("Fake.app")).unwrap();
    std::os::unix::fs::symlink(&app, own.path().join("Other.app")).unwrap();
    let both = vec![system.path().to_owned(), own.path().to_owned()];

    let installed = in_applications(&both, &names(&["Fake", "Other"])).unwrap();

    assert!(
        system.path().join("Fake.app").is_dir(),
        "a link to a folder"
    );
    assert!(!installed["Fake"]);
    assert!(!installed["Other"]);
}

#[test]
fn a_plain_file_named_like_an_app_is_not_installed() {
    let folder = applications();
    std::fs::write(folder.path().join("Firefox.app"), b"not an app").unwrap();

    let installed = in_applications(&only(folder.path()), &names(&["Firefox"])).unwrap();

    assert!(!installed["Firefox"]);
}

#[test]
fn a_name_reaching_outside_the_folder_is_never_installed() {
    let parent = applications();
    let folder = parent.path().join("Applications");
    std::fs::create_dir(&folder).unwrap();
    std::fs::create_dir(parent.path().join("Outside.app")).unwrap();
    std::fs::create_dir(folder.join("..app")).unwrap();

    let installed =
        in_applications(&only(&folder), &names(&["../Outside", ".", "..", ""])).unwrap();

    assert!(
        installed.values().all(|installed| !installed),
        "{installed:?}"
    );
}

#[test]
fn a_missing_or_linked_applications_folder_holds_nothing_that_counts() {
    let (system, real, links) = (applications(), applications(), applications());
    std::fs::create_dir(system.path().join("Firefox.app")).unwrap();
    std::fs::create_dir(real.path().join("Probe.app")).unwrap();
    let linked = links.path().join("Applications");
    std::os::unix::fs::symlink(real.path(), &linked).unwrap();
    let missing = links.path().join("Missing");
    let folders = vec![system.path().to_owned(), linked, missing];

    let installed = in_applications(&folders, &names(&["Firefox", "Probe"])).unwrap();

    assert!(installed["Firefox"]);
    assert!(!installed["Probe"], "a linked folder is never looked in");
}

#[test]
fn an_answer_says_each_name_asked_by_its_name() {
    let folder = applications();
    std::fs::create_dir(folder.path().join("Google Chrome.app")).unwrap();

    let answered = answer(&only(folder.path()), &names(&["Google Chrome", "Firefox"]));

    assert_eq!(
        answered,
        json!({"ok": true, "op": "installed",
            "installed": {"Google Chrome": true, "Firefox": false}})
    );
}

#[test]
fn an_accounts_apps_are_in_this_macs_applications_folder_and_its_own() {
    assert_eq!(
        folders(Some(Path::new("/Users/guest"))),
        [
            PathBuf::from("/Applications"),
            PathBuf::from("/Users/guest/Applications")
        ]
    );
    assert_eq!(folders(None), [PathBuf::from("/Applications")]);
}

#[test]
fn only_one_entrys_name_is_plain() {
    for name in [
        "Firefox",
        "Google Chrome",
        "...",
        ".hidden",
        "Name.with.dots",
    ] {
        assert!(plain(name), "{name}");
    }
    for name in ["", ".", "..", "a/b", "../x", "/abs", "nul\0inside"] {
        assert!(!plain(name), "{name:?}");
    }
}

#[test]
fn an_applications_folder_that_is_there_but_cannot_be_read_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let (system, own) = (applications(), applications());
    let set = |mode| {
        std::fs::set_permissions(own.path(), std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set(0o000);
    let folders = vec![system.path().to_owned(), own.path().to_owned()];

    let answered = answer(&folders, &names(&["Firefox"]));

    set(0o700);
    assert_eq!(answered["code"], "apps", "{answered}");
    assert_eq!(answered["ok"], false);
}

#[test]
fn an_app_in_a_folder_that_may_be_read_but_not_searched_is_not_known() {
    use std::os::unix::fs::PermissionsExt;
    let (system, home) = (applications(), applications());
    let own = home.path().join("Applications");
    std::fs::create_dir_all(own.join("Firefox.app")).unwrap();
    std::fs::create_dir(system.path().join("Probe.app")).unwrap();
    let set = |mode| std::fs::set_permissions(&own, std::fs::Permissions::from_mode(mode)).unwrap();
    set(0o400);
    let folders = vec![system.path().to_owned(), own.clone()];

    let unknown = in_applications(&folders, &names(&["Firefox"]));
    let found = in_applications(&folders, &names(&["Probe"]));
    let answered = answer(&folders, &names(&["Firefox"]));

    set(0o700);
    assert_eq!(
        unknown.unwrap_err().raw_os_error(),
        Some(nix::libc::EACCES),
        "never said absent"
    );
    assert!(found.unwrap()["Probe"], "one folder that has it is enough");
    assert_eq!(answered["ok"], false, "{answered}");
    assert_eq!(answered["code"], "apps", "{answered}");
}
