use super::*;
use std::path::PathBuf;

const HOME: &str = "/Users/person";

fn found(path: &str) -> Option<Store> {
    let (end, _) = steps(Path::new(path)).pop()?;
    store(&end, Some(Path::new(HOME)))
}

fn refused(path: &str) -> Error {
    refuse_in([Path::new(path)], Some(Path::new(HOME))).unwrap_err()
}

#[test]
fn every_kind_of_store_is_refused() {
    for path in [
        "/Users/person/Library/Keychains/login.keychain-db",
        "/Users/person/Library/Application Support/Google/Chrome/Default",
        "/Users/person/Library/Application Support/Google/Chrome Beta/Default/Preferences",
        "/Users/person/Library/Application Support/Chromium/Default",
        "/Users/person/Library/Application Support/BraveSoftware/Brave-Browser",
        "/Users/person/Library/Application Support/Microsoft Edge Dev/Default",
        "/Users/person/Library/Application Support/Arc/User Data/Default",
        "/Users/person/Library/Application Support/Vivaldi/Default",
        "/Users/person/Library/Application Support/com.operasoftware.OperaGX",
        "/Users/person/Library/Application Support/Firefox/Profiles/abc.default",
        "/Users/person/Library/Cookies/Cookies.binarycookies",
        "/Users/person/Library/Safari/History.db",
        "/Users/person/Library/Containers/com.apple.Safari/Data",
        "/Users/person/Library/Application Support/Slack/storage",
        "/Users/person/Library/Containers/com.tinyspeck.slackmacgap/Data",
        "/Users/person/Library/Application Support/discord/Local Storage",
        "/Users/person/.ssh/id_ed25519",
        "/Users/person/.ssh",
        "/Users/person/.gnupg/private-keys-v1.d",
        "/Users/person/.aws/credentials",
        "/Users/person/.config/gh/hosts.yml",
        "/Users/person/.netrc",
        "/Users/person/.docker/config.json",
        "/tmp/copy/Cookies",
        "/tmp/copy/Login Data",
        "/tmp/copy/Web Data",
        "/tmp/copy/cookies.sqlite",
        "/tmp/copy/logins.json",
        "/tmp/copy/key4.db",
        "/tmp/copy/cert9.db",
        "/tmp/copy/work.keychain",
        "/Volumes/Backup/login.keychain-db",
        "/Users/person/dev/app/.env",
        "/Users/person/dev/app/.env.local",
        "/Users/person/dev/app/.env.production",
        "/Users/person/dev/app/.npmrc",
    ] {
        assert!(found(path).is_some(), "{path} was not refused");
    }
}

#[test]
fn names_that_only_look_like_a_store_pass() {
    for path in [
        "/tmp/copy/my-cookies.sqlite.txt",
        "/tmp/copy/Cookies-test",
        "/Users/person/dev/app/.env.example",
        "/Users/person/dev/app/.env.sample",
        "/Users/person/dev/app/.env.template",
        "/Users/person/dev/app/.npmrc.bak",
        "/Users/person/dev/app/environment.json",
        "/Users/person/.ssh/known_hosts",
        "/Users/person/.ssh/known_hosts.old",
        "/Users/person/.ssh/config",
        "/Users/person/.aws/config",
        "/Users/person/Library/Application Support/Google/Drive",
        "/Users/person/Library/Application Support/Firefox/Crash Reports",
        "/Users/other/.ssh/id_ed25519",
    ] {
        assert_eq!(found(path), None, "{path} was refused");
    }
}

#[test]
fn names_are_compared_as_the_macs_disks_compare_them_ignoring_case() {
    assert!(found("/users/PERSON/library/keychains/login.keychain-db").is_some());
    assert!(found("/Users/person/Library/Application Support/google/chrome/Default").is_some());
    assert!(found("/tmp/COOKIES.SQLITE").is_some());
    assert!(found("/tmp/.ENV").is_some());
}

#[test]
fn a_folder_that_holds_a_store_is_refused_as_holding_it() {
    for path in [
        "/Users/person/Library",
        "/Users/person/Library/Application Support",
        "/Users/person/Library/Application Support/Google",
        "/Users/person/.config/gh",
        "/Users/person",
        "/Users",
        "/",
    ] {
        let found = found(path).unwrap_or_else(|| panic!("{path} was not refused"));
        assert!(found.within, "{path} is a folder holding a store");
    }
    assert_eq!(
        found("/Users/person/dev"),
        None,
        "a project folder holds no known folder"
    );
}

#[test]
fn a_symlink_to_a_store_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let store = folder.path().join("cookies.sqlite");
    fs::write(&store, b"").unwrap();
    let link = folder.path().join("innocent.txt");
    std::os::unix::fs::symlink(&store, &link).unwrap();
    let error = refuse_in([link.as_path()], None).unwrap_err();
    assert_eq!(error.code, Code::Credentials);
}

#[test]
fn a_path_that_does_not_exist_is_checked_as_written_and_otherwise_passes() {
    assert!(refuse_in([Path::new("/nowhere/.env")], None).is_err());
    assert!(refuse_in([Path::new("/nowhere/notes.txt")], None).is_ok());
}

#[test]
fn the_first_store_among_several_paths_is_refused() {
    let paths = [
        PathBuf::from("/tmp/notes.txt"),
        PathBuf::from("/tmp/.npmrc"),
    ];
    let error = refuse_in(paths.iter().map(PathBuf::as_path), None).unwrap_err();
    assert!(error.error.contains(".npmrc"));
}

#[test]
fn refusals_say_what_the_store_is_and_never_the_home_folder() {
    let cases = [
        (
            "/Users/person/Library/Application Support/Firefox/Profiles/x/cookies.sqlite",
            "Atlas won't copy Firefox's cookies.sqlite into the helper: it holds your sign-ins",
        ),
        (
            "/Users/person/dev/app/.env",
            "Atlas won't copy .env into the helper: it often holds passwords and tokens",
        ),
        (
            "/Users/person/Library/Application Support/Google/Chrome/Default",
            "Atlas won't copy Default into the helper: it is part of Chrome's profiles, which holds your sign-ins",
        ),
        (
            "/Users/person/Library",
            "Atlas won't copy Library into the helper: it holds your keychains, with your sign-ins",
        ),
        (
            "/Users/person/Library/Keychains/login.keychain-db",
            "Atlas won't copy the keychain login.keychain-db into the helper: it holds your sign-ins",
        ),
    ];
    for (path, said) in cases {
        let error = refused(path);
        assert_eq!(error.code, Code::Credentials);
        assert_eq!(error.error, said);
    }
}

#[test]
fn a_store_folder_linked_to_another_place_is_still_refused_there() {
    let (_root, home, elsewhere) =
        linked_home("Library/Application Support/Google/Chrome", "Preferences");
    let selected = elsewhere.join("Preferences");
    let error = refuse_in([selected.as_path()], Some(&home)).unwrap_err();
    assert_eq!(error.code, Code::Credentials);
}

/// A home folder whose `rel` folder is a link to `outside`, which holds `file`.
fn linked_home(rel: &str, file: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let outside = root.path().join("volume/moved");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join(file), b"").unwrap();
    let shown = home.join(rel);
    fs::create_dir_all(shown.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&outside, &shown).unwrap();
    (root, home, outside)
}

#[test]
fn a_folder_above_a_store_linked_to_another_place_is_refused_as_holding_it() {
    let (root, home, _) = linked_home(
        "Library/Application Support/Firefox/Profiles",
        "logins.json.bak",
    );
    let error = refuse_in([root.path().join("volume").as_path()], Some(&home)).unwrap_err();
    assert_eq!(error.code, Code::Credentials);
    assert!(
        error.error.contains("Firefox's profiles"),
        "{}",
        error.error
    );
}

#[test]
fn a_link_named_like_a_store_is_refused_whatever_it_leads_to() {
    let folder = tempfile::tempdir().unwrap();
    let settings = folder.path().join("settings");
    fs::write(&settings, b"").unwrap();
    let link = folder.path().join(".env");
    std::os::unix::fs::symlink(&settings, &link).unwrap();
    assert!(refuse_in([link.as_path()], None).is_err());
}

#[test]
fn a_path_through_dot_dot_is_judged_by_where_it_leads() {
    assert_eq!(found("/Users/person/.ssh/../notes.txt"), None);
    assert!(found("/Users/person/dev/../.ssh/id_ed25519").is_some());
    assert!(found("/Users/person/./.aws/credentials").is_some());
}

#[test]
fn the_folder_this_runs_in_and_its_parent_are_judged_where_they_are() {
    // The tests run in their crate's folder, which holds no store, under no home of this test.
    let home = Path::new("/Users/nobody-here");
    for given in [".", "./", ".."] {
        assert!(
            refuse_in([Path::new(given)], Some(home)).is_ok(),
            "{given} was refused"
        );
    }
}

#[test]
fn a_dot_dot_after_a_link_is_judged_where_the_link_leads() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let deep = root.path().join("project/deep");
    fs::create_dir_all(&deep).unwrap();
    fs::create_dir_all(root.path().join("project/.ssh")).unwrap();
    fs::write(root.path().join("project/.ssh/notes.txt"), b"").unwrap();
    fs::create_dir_all(&home).unwrap();
    // A relative link: it leads on from the folder it sits in.
    std::os::unix::fs::symlink("../project/deep", home.join("alias")).unwrap();
    let through = home.join("alias/../.ssh/notes.txt");
    assert!(
        refuse_in([through.as_path()], Some(&home)).is_ok(),
        "judged as the home's own .ssh"
    );
    let named = home.join("alias/../.ssh/.env");
    fs::write(root.path().join("project/.ssh/.env"), b"").unwrap();
    assert!(
        refuse_in([named.as_path()], Some(&home)).is_err(),
        "its own name still counts"
    );
}

#[test]
fn a_linked_key_reached_through_dot_dot_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::create_dir_all(home.join("dev")).unwrap();
    let key = root.path().join("volume/key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, b"").unwrap();
    std::os::unix::fs::symlink(&key, home.join(".ssh/id_ed25519")).unwrap();
    let through = home.join("dev/../.ssh/id_ed25519");
    assert!(refuse_in([through.as_path()], Some(&home)).is_err());
}

#[test]
fn a_linked_key_reached_through_a_folder_alias_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    let key = root.path().join("volume/key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, b"").unwrap();
    std::os::unix::fs::symlink(&key, home.join(".ssh/id_ed25519")).unwrap();
    std::os::unix::fs::symlink(home.join(".ssh"), home.join("alias")).unwrap();
    assert!(refuse_in([home.join("alias/id_ed25519").as_path()], Some(&home)).is_err());
}

#[test]
fn a_linked_key_in_a_moved_ssh_folder_reached_through_an_alias_is_refused() {
    let (root, home, outside) = linked_home(".ssh", "config");
    let key = root.path().join("keys/key");
    fs::create_dir_all(key.parent().unwrap()).unwrap();
    fs::write(&key, b"").unwrap();
    std::os::unix::fs::symlink(&key, outside.join("id_ed25519")).unwrap();
    std::os::unix::fs::symlink(&outside, root.path().join("alias")).unwrap();
    assert!(
        refuse_in(
            [root.path().join("alias/id_ed25519").as_path()],
            Some(&home)
        )
        .is_err()
    );
    assert!(refuse_in([root.path().join("alias/config").as_path()], Some(&home)).is_ok());
}

#[test]
fn the_home_through_the_systems_data_volume_is_the_home() {
    assert!(found_through_homes(
        "/System/Volumes/Data/Users/person/.ssh/id_ed25519"
    ));
    assert!(found_through_homes(
        "/System/Volumes/Data/Users/person/Library/Keychains/login.keychain-db"
    ));
    assert!(!found_through_homes(
        "/System/Volumes/Data/Users/person/dev/notes.txt"
    ));
}

/// Whether `path` is refused with `/Users/person` as the home, every path of it included.
fn found_through_homes(path: &str) -> bool {
    refuse_in([Path::new(path)], Some(Path::new(HOME))).is_err()
}

#[test]
fn a_key_behind_a_second_link_inside_a_linked_ssh_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    let secrets = root.path().join("volume/secrets");
    fs::create_dir_all(&secrets).unwrap();
    fs::write(secrets.join("private-key"), b"").unwrap();
    std::os::unix::fs::symlink(&secrets, home.join(".ssh/keys")).unwrap();
    std::os::unix::fs::symlink(home.join(".ssh"), home.join("alias")).unwrap();
    let through = home.join("alias/keys/private-key");
    assert!(refuse_in([through.as_path()], Some(&home)).is_err());
}

#[test]
fn a_walk_that_ends_at_the_root_or_after_a_last_dot_dot_is_judged_where_it_ends() {
    let root = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(root.path()).unwrap().join("home");
    fs::create_dir_all(home.join("dev")).unwrap();
    assert!(
        refuse_in([Path::new("/")], Some(&home)).is_err(),
        "the root holds every home"
    );
    let back = home.join("dev/..");
    assert!(
        refuse_in([back.as_path()], Some(&home)).is_err(),
        "it ends at the home folder"
    );
}

#[test]
fn a_linked_ssh_folder_named_as_usual_keeps_its_files_that_hold_no_key() {
    let (_root, home, outside) = linked_home(".ssh", "config");
    fs::write(outside.join("known_hosts"), b"").unwrap();
    fs::write(outside.join("id_ed25519"), b"").unwrap();
    for allowed in ["config", "known_hosts"] {
        let selected = home.join(".ssh").join(allowed);
        assert!(
            refuse_in([selected.as_path()], Some(&home)).is_ok(),
            "{allowed} was refused"
        );
    }
    let key = home.join(".ssh/id_ed25519");
    assert!(refuse_in([key.as_path()], Some(&home)).is_err());
}

#[test]
fn a_moved_store_folder_named_through_the_data_volume_is_refused() {
    let (_root, home, outside) = linked_home(".ssh", "config");
    fs::write(outside.join("id_ed25519"), b"").unwrap();
    let real = fs::canonicalize(&outside).unwrap();
    let aliased = Path::new(DATA)
        .join(real.strip_prefix("/").unwrap())
        .join("id_ed25519");
    // Only where the data volume shows this folder too, as it does for the temporary folders.
    if aliased.exists() {
        assert!(refuse_in([aliased.as_path()], Some(&home)).is_err());
    }
    assert!(
        relocated(&home)
            .iter()
            .any(|(name, _, _)| name.starts_with(DATA))
    );
}

#[test]
fn a_store_folder_linked_through_the_data_volume_is_refused_by_its_plain_spelling() {
    let root = tempfile::tempdir().unwrap();
    let real = fs::canonicalize(root.path()).unwrap();
    let home = real.join("home");
    let keys = real.join("keys");
    fs::create_dir_all(&keys).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(keys.join("id_ed25519"), b"").unwrap();
    let through_data = Path::new(DATA).join(keys.strip_prefix("/").unwrap());
    // Only where the data volume shows this folder too, as it does for the temporary folders.
    if !through_data.exists() {
        return;
    }
    std::os::unix::fs::symlink(&through_data, home.join(".ssh")).unwrap();
    assert!(refuse_in([keys.join("id_ed25519").as_path()], Some(&home)).is_err());
}

#[test]
fn a_linked_store_folder_whose_parent_cannot_be_listed_is_still_refused_where_it_leads() {
    use std::os::unix::fs::PermissionsExt;
    let (_root, home, _) = linked_home("Library/Application Support/Google/Chrome", "Preferences");
    let google = home.join("Library/Application Support/Google");
    // Can be passed through, not listed.
    fs::set_permissions(&google, fs::Permissions::from_mode(0o311)).unwrap();
    let through = google.join("Chrome/Preferences");
    let refused = refuse_in([through.as_path()], Some(&home)).is_err();
    fs::set_permissions(&google, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(refused);
}

#[test]
fn a_store_behind_a_linked_library_and_an_unlistable_folder_is_refused_by_its_path_as_written() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let library = root.path().join("elsewhere/Library");
    let google = library.join("Application Support/Google");
    fs::create_dir_all(&google).unwrap();
    let profile = root.path().join("profile");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("Preferences"), b"{}").unwrap();
    std::os::unix::fs::symlink(&profile, google.join("Chrome")).unwrap();
    std::os::unix::fs::symlink(&library, home.join("Library")).unwrap();
    fs::set_permissions(&google, fs::Permissions::from_mode(0o311)).unwrap();
    fs::create_dir_all(home.join("dev")).unwrap();
    let selected = home.join("Library/Application Support/Google/Chrome/Preferences");
    let through_dev = home.join("dev/../Library/Application Support/Google/Chrome/Preferences");
    let refused =
        [selected, through_dev].map(|path| refuse_in([path.as_path()], Some(&home)).is_err());
    fs::set_permissions(&google, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(refused, [true, true]);
}
