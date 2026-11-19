use super::*;
use std::os::unix::fs::PermissionsExt;

fn home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    (dir, home)
}

fn credentials(error: &Error) -> bool {
    error.code == Code::Credentials
}

#[test]
fn a_plain_file_the_person_owns_reads_through_its_descriptor() {
    let (_dir, home) = home();
    let file = home.join("note.txt");
    std::fs::write(&file, "hello").unwrap();

    let reading = read(&file).unwrap();
    assert_eq!(reading.path(), file);
    assert_eq!(reading.read_up_to(64).unwrap(), b"hello");
}

#[test]
fn a_file_in_atlas_own_folder_is_refused() {
    let file = crate::config::home()
        .unwrap()
        .join(crate::own_folder::FOLDER)
        .join(format!("checked-test-{}", crate::files::token()));
    std::fs::create_dir_all(&file).unwrap();
    let inside = file.join("secret");
    std::fs::write(&inside, "SECRET").unwrap();
    let outcome = read(&inside);
    std::fs::remove_dir_all(&file).unwrap();
    let error = outcome.unwrap_err();
    assert!(credentials(&error), "{error}");
    assert_eq!(error.error, crate::own_folder::REFUSAL);
}

#[test]
fn a_symlink_source_is_refused() {
    let (_dir, home) = home();
    let target = home.join("real.txt");
    std::fs::write(&target, "x").unwrap();
    let link = home.join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = read(&link).unwrap_err();
    assert!(credentials(&error), "{error}");
    assert!(error.error.contains("a link itself"), "{error}");
}

#[test]
fn a_file_under_a_non_sticky_world_writable_folder_is_refused() {
    let (_dir, home) = home();
    let open = home.join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o0777)).unwrap();
    let file = open.join("note.txt");
    std::fs::write(&file, "x").unwrap();

    let error = read(&file).unwrap_err();
    assert!(credentials(&error), "{error}");
    assert!(
        error.error.contains("can be changed by another account"),
        "{error}"
    );
}

#[test]
fn a_file_in_a_sticky_world_writable_folder_reads() {
    let (_dir, home) = home();
    let sticky = home.join("shared");
    std::fs::create_dir(&sticky).unwrap();
    std::fs::set_permissions(&sticky, std::fs::Permissions::from_mode(0o1777)).unwrap();
    let file = sticky.join("note.txt");
    std::fs::write(&file, "hi").unwrap();

    assert_eq!(read(&file).unwrap().read_up_to(8).unwrap(), b"hi");
}

#[test]
fn a_writer_replaces_an_entry_through_the_folder_without_following_a_link() {
    let (_dir, home) = home();
    let folder_path = home.join("out");
    std::fs::create_dir(&folder_path).unwrap();
    // A link planted at the destination name must be replaced, not written
    // through: its target, a secret, stays as it was.
    let secret = home.join("secret");
    std::fs::write(&secret, "SECRET").unwrap();
    std::os::unix::fs::symlink(&secret, folder_path.join("out.txt")).unwrap();

    let writer = folder(&folder_path).unwrap();
    writer
        .replace(std::ffi::OsStr::new("out.txt"), b"fresh", 0o644)
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(folder_path.join("out.txt")).unwrap(),
        "fresh"
    );
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "SECRET");
    assert!(
        !folder_path
            .join("out.txt")
            .symlink_metadata()
            .unwrap()
            .is_symlink()
    );
}

#[test]
fn a_handover_source_tree_of_the_persons_own_is_proven_and_its_path_returned() {
    let (_dir, home) = home();
    let app = home.join("App");
    std::fs::create_dir_all(app.join("Contents")).unwrap();
    std::fs::write(app.join("Contents/info"), "x").unwrap();

    assert_eq!(handover_path(&app).unwrap(), app);
}

#[test]
fn the_folder_rule_judges_ownership_and_writability_as_other_accounts_see_it() {
    let me = 501;
    let other = 502;
    let facts = |uid, gid, mode| Facts { uid, gid, mode };

    assert!(facts(me, 20, 0o040755).owned_by(me));
    assert!(facts(0, 20, 0o040755).owned_by(me));
    assert!(!facts(other, 20, 0o040755).owned_by(me));
    // Admin-group write (80) is trusted (/Applications is root:admin 0775).
    assert!(!facts(0, 80, 0o040775).loosely_writable());
    // An untrusted group's write, or world write, is loose.
    assert!(facts(0, other, 0o040775).loosely_writable());
    assert!(facts(0, 0, 0o040777).loosely_writable());
    assert!(facts(0, 0, 0o041777).sticky());
    assert!(facts(me, 20, 0o100644).owned_by(me) && !facts(me, 20, 0o100644).is_dir());
    assert!(facts(me, 20, 0o040755).is_dir());
}

#[test]
fn a_folder_named_as_dot_or_dot_dot_is_the_folder_it_names() {
    let (_dir, home) = home();
    let inner = home.join("inner");
    std::fs::create_dir(&inner).unwrap();

    let dot = folder(&inner.join(".")).unwrap();
    let up = folder(&inner.join("..")).unwrap();

    assert_eq!(dot.path, std::fs::canonicalize(&inner).unwrap());
    assert_eq!(up.path, std::fs::canonicalize(&home).unwrap());
    up.replace(std::ffi::OsStr::new("AGENTS.md"), b"taught", 0o644)
        .unwrap();
    assert_eq!(std::fs::read(home.join("AGENTS.md")).unwrap(), b"taught");
}

#[test]
fn a_fifo_is_refused_without_waiting_for_a_writer() {
    let (_dir, home) = home();
    let fifo = home.join("pipe.toml");
    nix::unistd::mkfifo(&fifo, Mode::S_IRWXU).unwrap();

    let error = read(&fifo).unwrap_err();
    assert!(credentials(&error), "{error}");
    assert!(error.error.contains("not a plain file"), "{error}");
}

#[test]
fn a_file_another_account_may_write_is_never_read() {
    let (_dir, home) = home();
    let file = home.join("open.toml");
    std::fs::write(&file, "x").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();

    let error = read(&file).unwrap_err();
    assert!(credentials(&error), "{error}");
    assert!(
        error.error.contains("another account may change it"),
        "{error}"
    );
}

#[test]
fn a_writer_destination_another_account_may_add_to_is_refused_sticky_or_not() {
    let (_dir, home) = home();
    for mode in [0o0777, 0o1777] {
        let folder_path = home.join(format!("shared-{mode:o}"));
        std::fs::create_dir(&folder_path).unwrap();
        std::fs::set_permissions(&folder_path, std::fs::Permissions::from_mode(mode)).unwrap();

        let error = folder(&folder_path).unwrap_err();
        assert!(credentials(&error), "{mode:o}: {error}");
    }
}

#[test]
fn a_private_folder_is_made_through_its_parent_and_removed_only_when_made() {
    let (_dir, home) = home();
    let path = home.join("frames");
    let made = private_folder(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    made.remove_if_made();
    assert!(!path.exists());

    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    private_folder(&path).unwrap().remove_if_made();
    assert!(path.exists(), "a folder it did not make stays");

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = private_folder(&path).unwrap_err();
    assert!(error.error.contains("private (0700)"), "{error}");
}

#[test]
fn an_entry_of_an_own_folder_is_read_through_its_descriptor_within_a_bound() {
    let (_dir, home) = home();
    let canary = home.join("canary");
    std::fs::create_dir(&canary).unwrap();
    std::fs::write(canary.join("state.json"), b"{\"ok\":true}").unwrap();

    let folder = own_folder(&canary).unwrap();

    assert_eq!(
        folder
            .read_entry(OsStr::new("state.json"), 11)
            .unwrap()
            .as_deref(),
        Some(&b"{\"ok\":true}"[..])
    );
    assert!(folder.read_entry(OsStr::new("state.json"), 10).is_err());
    assert_eq!(folder.read_entry(OsStr::new("missing"), 64).unwrap(), None);
}

#[test]
fn an_own_folder_replaced_by_a_link_after_it_was_opened_is_still_the_one_read() {
    let (_dir, home) = home();
    let (canary, elsewhere) = (home.join("canary"), home.join("elsewhere"));
    std::fs::create_dir(&canary).unwrap();
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(canary.join("state.json"), b"ours").unwrap();
    std::fs::write(elsewhere.join("state.json"), b"theirs").unwrap();

    let folder = own_folder(&canary).unwrap();
    std::fs::rename(&canary, home.join("moved")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &canary).unwrap();

    assert_eq!(
        folder
            .read_entry(OsStr::new("state.json"), 64)
            .unwrap()
            .as_deref(),
        Some(&b"ours"[..])
    );
    let error = own_folder(&canary).unwrap_err();
    assert!(credentials(&error), "{error}");
}

#[test]
fn an_entry_that_is_a_link_a_fifo_or_a_folder_is_never_read() {
    let (_dir, home) = home();
    let canary = home.join("canary");
    std::fs::create_dir(&canary).unwrap();
    std::fs::write(home.join("outside.json"), b"outside").unwrap();
    std::os::unix::fs::symlink(home.join("outside.json"), canary.join("state.json")).unwrap();
    nix::unistd::mkfifo(&canary.join("events.jsonl"), Mode::S_IRWXU).unwrap();
    std::fs::create_dir(canary.join("fixture.log")).unwrap();

    let folder = own_folder(&canary).unwrap();

    for name in ["state.json", "events.jsonl", "fixture.log"] {
        let error = folder.read_entry(OsStr::new(name), 64).unwrap_err();
        assert!(credentials(&error), "{name}: {error}");
    }
    let link = folder.read_entry(OsStr::new("state.json"), 64).unwrap_err();
    assert!(link.error.contains("a link"), "{link}");
    let fifo = folder
        .read_entry(OsStr::new("events.jsonl"), 64)
        .unwrap_err();
    assert!(fifo.error.contains("not a plain file"), "{fifo}");
}

#[test]
fn an_own_folder_is_refused_when_shared_by_its_mode_or_not_this_accounts() {
    let (_dir, home) = home();
    for mode in [0o775, 0o757, 0o1777] {
        let shared = home.join(format!("shared-{mode:o}"));
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(mode)).unwrap();

        let error = own_folder(&shared).unwrap_err();
        assert!(credentials(&error), "{mode:o}: {error}");
    }
    let file = home.join("file");
    std::fs::write(&file, b"x").unwrap();

    let roots = own_folder(Path::new("/usr")).unwrap_err();
    let plain = own_folder(&file).unwrap_err();

    assert!(roots.error.contains("another account"), "{roots}");
    assert!(plain.error.contains("not a folder"), "{plain}");
}

#[test]
fn only_a_plain_file_beside_an_own_folders_descriptor_is_a_file_it_holds() {
    let (_dir, home) = home();
    let canary = home.join("canary");
    std::fs::create_dir(&canary).unwrap();
    std::fs::write(canary.join("atlas-fixture.txt"), b"saved").unwrap();
    std::os::unix::fs::symlink(canary.join("atlas-fixture.txt"), canary.join("linked.txt"))
        .unwrap();
    std::fs::create_dir(canary.join("folder.txt")).unwrap();

    let folder = own_folder(&canary).unwrap();

    assert!(folder.holds_file(OsStr::new("atlas-fixture.txt")).unwrap());
    for name in ["linked.txt", "folder.txt", "missing.txt"] {
        assert!(!folder.holds_file(OsStr::new(name)).unwrap(), "{name}");
    }
}
