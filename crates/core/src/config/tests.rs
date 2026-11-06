use super::*;
use crate::files::{save_private, write_atomic, write_beside};

#[test]
fn only_output_placement_preserves_the_helper_on_reload() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = Config::parse(
        crate::test_support::SETTINGS.as_bytes(),
        &directory.path().join("u.toml"),
    )?;
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

#[test]
fn pictures_before_actions_are_kept_unless_the_settings_turn_them_off() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let parse = |text: &str| Config::parse(text.as_bytes(), &path);

    let absent = parse(crate::test_support::SETTINGS)?;
    assert!(absent.activity.pictures_before_actions);
    assert!(
        !absent.to_toml()?.contains("activity"),
        "a default setting is not written"
    );

    let off = parse(&format!(
        "{}[activity]\npictures_before_actions = false\n",
        crate::test_support::SETTINGS
    ))?;
    assert!(!off.activity.pictures_before_actions);
    assert!(
        absent.same_helper(&off),
        "it changes nothing about the helper"
    );

    for invalid in [
        "[activity]\npictures_before_actions = \"no\"\n",
        "[activity]\nvideo = true\n",
        "activity = true\n",
    ] {
        let error = parse(&format!("{}{invalid}", crate::test_support::SETTINGS)).unwrap_err();
        assert_eq!(error.code, Code::Config, "{invalid}");
    }
    Ok(())
}

#[test]
fn turning_pictures_off_and_on_keeps_every_other_setting_as_written() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    fs::write(&path, crate::test_support::SETTINGS)?;
    let original = Config::load(&path)?;

    keep_pictures(&path, false)?;
    let off = Config::load(&path)?;
    assert!(!off.activity.pictures_before_actions);
    assert_eq!(
        Config {
            activity: Activity::default(),
            ..off
        },
        original
    );
    assert!(
        fs::read_to_string(&path)?.contains("output_dir = \"out\""),
        "a relative path stays as written"
    );

    keep_pictures(&path, true)?;
    assert_eq!(Config::load(&path)?, original);
    assert!(!fs::read_to_string(&path)?.contains("activity"));
    let entries = fs::read_dir(directory.path())?.count();
    assert_eq!(entries, 1, "no temporary file is left behind");
    Ok(())
}

#[test]
fn a_picture_setting_changes_only_its_own_line_in_the_settings_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let written = format!(
        "# Atlas settings\n{}\n[activity]\n# how long\nkeep_days = 3 # days\n",
        crate::test_support::SETTINGS
    );
    fs::write(&path, &written)?;

    keep_days(&path, 5)?;
    assert_eq!(
        fs::read_to_string(&path)?,
        written.replace("keep_days = 3", "keep_days = 5")
    );

    keep_pictures(&path, false)?;
    typed_text_limit(&path, Some(80))?;
    keep_pictures(&path, true)?;
    typed_text_limit(&path, None)?;
    keep_days(&path, 3)?;
    assert_eq!(fs::read_to_string(&path)?, written);
    Ok(())
}

#[test]
fn pictures_keep_a_week_and_all_typed_text_unless_the_settings_say_otherwise() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    fs::write(&path, crate::test_support::SETTINGS)?;
    let original = Config::load(&path)?;
    assert_eq!(original.activity.keep_days, 7);
    assert_eq!(original.activity.typed_text_limit, None);

    keep_days(&path, 3)?;
    typed_text_limit(&path, Some(80))?;
    let changed = Config::load(&path)?.activity;
    assert_eq!((changed.keep_days, changed.typed_text_limit), (3, Some(80)));
    assert_eq!(
        changed.keep(),
        std::time::Duration::from_secs(3 * 24 * 60 * 60)
    );

    keep_days(&path, 7)?;
    typed_text_limit(&path, None)?;
    assert_eq!(Config::load(&path)?, original);
    assert!(!fs::read_to_string(&path)?.contains("activity"));

    for invalid in [0, KEEP_DAYS_MAX + 1] {
        assert_eq!(keep_days(&path, invalid).unwrap_err().code, Code::Config);
    }
    assert_eq!(
        typed_text_limit(&path, Some(0)).unwrap_err().code,
        Code::Config
    );
    assert_eq!(
        Config::load(&path)?,
        original,
        "a refused change writes nothing"
    );
    Ok(())
}
