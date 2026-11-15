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
