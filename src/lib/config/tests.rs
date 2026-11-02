use super::*;
use crate::files::{save_private, write_atomic, write_beside};

#[test]
fn only_output_placement_preserves_the_helper_on_reload() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = crate::test_support::client_in(directory.path()).config;
    let moved = Config {
        output_dir: directory.path().join("elsewhere"),
        ..original.clone()
    };
    assert!(original.same_helper(&moved));
    assert_ne!(original, moved);
    for changed in [
        Config {
            helper_uid: original.helper_uid + 1,
            ..original.clone()
        },
        Config {
            socket_path: directory.path().join("another-daemon"),
            ..original.clone()
        },
        Config {
            identity_probe: directory.path().join("another-probe"),
            ..original.clone()
        },
        Config {
            input_socket: directory.path().join("another-input"),
            ..original.clone()
        },
    ] {
        assert!(!original.same_helper(&changed), "{changed:?}");
    }
    Ok(())
}

#[test]
fn a_settings_open_failure_names_only_the_step_and_cause() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.toml");
    let cause = fs::File::open(&path).unwrap_err();
    let error = Config::read(&path).unwrap_err();
    assert_eq!(error.code, Code::Io);
    assert!(!error.uncertain);
    assert_eq!(error.error, format!("Open settings: {cause}"));
}

#[test]
fn a_settings_read_failure_names_the_path_once_and_keeps_the_cause() {
    let directory = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(directory.path()).unwrap();
    let cause = fs::read(&path).unwrap_err();
    let error = Config::load(&path).unwrap_err();
    assert_eq!(error.code, Code::ConfigInvalid);
    assert!(!error.uncertain);
    assert_eq!(
        error.error,
        format!(
            "Atlas could not read its settings in {}: Read settings: {cause}",
            tilde(&path)
        )
    );
}

/// A probe that is a link keeps its own path, so install replaces the
/// link and never the executable it points to.
#[test]
fn the_probe_path_is_not_followed_through_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("App/atlas");
    fs::create_dir_all(real.parent().unwrap()).unwrap();
    fs::write(&real, b"").unwrap();
    let link = dir.path().join("shared/atlas");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let text = format!(
        "helper_uid = 502\nsocket_path = \"s.sock\"\nidentity_probe = \"{}\"\n\
             input_socket = \"i.sock\"\noutput_dir = \"out\"\n",
        link.display()
    );
    let config = Config::parse(text.as_bytes(), &dir.path().join("c.toml")).unwrap();
    let shared = fs::canonicalize(link.parent().unwrap()).unwrap();
    assert_eq!(config.identity_probe, shared.join("atlas"));
}

#[test]
fn atomic_writes_replace_whole_files_and_leave_nothing_behind() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/record.json");
    save_private(&path, b"first").unwrap();
    save_private(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let error = write_beside(
        &path,
        0o600,
        |_| Err(std::io::Error::from_raw_os_error(libc::EACCES).into()),
        |_| unreachable!("nothing is placed after a failed write"),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Io);
    assert!(!error.uncertain);
    assert!(error.error.starts_with("Write replacement for"), "{error}");
    assert!(error.error.contains(&path.display().to_string()), "{error}");
    assert!(error.error.contains("os error 13"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"second");
    let entries = fs::read_dir(path.parent().unwrap()).unwrap().count();
    assert_eq!(entries, 1, "no temporary file is left behind");
}

/// The umask (022 at least, in tests) never narrows the mode asked for.
#[test]
fn atomic_writes_get_exactly_the_mode_asked_for() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.json");
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

    write_atomic(&path, b"{}", 0o666).unwrap();
    assert_eq!(mode(&path), 0o666);

    write_atomic(&path, b"{}", 0o644).unwrap();
    assert_eq!(mode(&path), 0o644);

    write_atomic(&path, b"{}", 0o600).unwrap();
    assert_eq!(mode(&path), 0o600, "a private file stays private");
}
