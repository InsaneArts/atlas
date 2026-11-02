use super::*;
#[test]
fn a_selected_account_is_never_replaced_while_its_configuration_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let helper = setup::Account {
        uid: 7001,
        name: "selected".into(),
        home: dir.path().join("selected"),
    };
    let expected = setup::identity::Identity {
        uid: helper.uid,
        name: helper.name.clone(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let configuration = proposed(&helper).unwrap();
    write_config(&dir.path().join("atlas.toml"), &configuration).unwrap();
    let replacement = setup::Account {
        name: "replacement".into(),
        home: dir.path().join("replacement"),
        ..helper.clone()
    };
    let recorded = std::cell::RefCell::new(None);
    let record = |_: &setup::Account, identity: &setup::identity::Identity| {
        recorded.replace(Some(identity.clone()));
        Ok(())
    };
    let outcome = record_selected(&helper, &expected, |_| Ok(Some(replacement)), record);
    assert!(
        outcome.is_err(),
        "a reused UID must require another explicit selection"
    );
    assert!(
        recorded.borrow().is_none(),
        "a replacement must never be recorded"
    );
    record_selected(&helper, &expected, |_| Ok(Some(helper.clone())), record).unwrap();
    assert_eq!(recorded.into_inner(), Some(expected));
}

use std::path::PathBuf;

/// A configuration is written once, privately, and never over another.
#[test]
fn a_configuration_is_never_written_over_another() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Atlas/atlas.toml");
    let helper = setup::Account {
        name: "atlas".into(),
        uid: 503,
        home: "/Users/atlas".into(),
    };
    let config = proposed(&helper).unwrap();
    write_config(&path, &config).unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let before = fs::read(&path).unwrap();
    assert!(
        write_config(&path, &config).is_err(),
        "an existing file stays"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    let leftovers = fs::read_dir(path.parent().unwrap()).unwrap().count();
    assert_eq!(leftovers, 1, "no temporary file is left behind");
}

/// A new configuration points everything at the helper's private layout.
#[test]
fn a_new_configuration_uses_the_managed_layout() {
    let helper = setup::Account {
        name: "atlas".into(),
        uid: 503,
        home: "/Users/atlas".into(),
    };
    let config = proposed(&helper).unwrap();
    assert_eq!(config.helper_uid, 503);
    assert_eq!(
        config.socket_path,
        PathBuf::from("/Users/atlas/.atlas/helper/503.sock")
    );
    assert_eq!(
        config.input_socket,
        PathBuf::from("/Users/atlas/.atlas/input/input.sock")
    );
    assert_eq!(
        config.identity_probe,
        PathBuf::from("/Users/Shared/Atlas/atlas")
    );
    assert!(
        config
            .output_dir
            .ends_with("Library/Application Support/Atlas/runs")
    );
}

fn configuration_at(root: &Path) -> Config {
    Config {
        helper_uid: 60001,
        socket_path: root.join("helper.sock"),
        identity_probe: root.join("atlas"),
        input_socket: root.join("input.sock"),
        output_dir: root.join("runs"),
    }
}

#[test]
fn replacing_matching_settings_preserves_semantic_comparison_and_private_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let path = root.join("atlas.toml");
    let previous = configuration_at(&root);
    write_config(&path, &previous).unwrap();
    let formatted = format!(
        "# Local formatting does not change the settings.\n{}",
        previous.to_toml().unwrap()
    );
    fs::write(&path, formatted).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let replacement = Config {
        helper_uid: 60002,
        ..previous.clone()
    };

    replace_config(&path, &previous, &replacement).unwrap();

    assert_eq!(Config::load(&path).unwrap(), replacement);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "no temporary file remains"
    );
}

#[test]
fn replacing_stale_settings_refuses_without_changing_the_file_or_leaving_a_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let path = root.join("atlas.toml");
    let current = configuration_at(&root);
    write_config(&path, &current).unwrap();
    let before = fs::read(&path).unwrap();
    let stale = Config {
        helper_uid: 60002,
        ..current.clone()
    };
    let replacement = Config {
        helper_uid: 60003,
        ..current
    };

    let error = replace_config(&path, &stale, &replacement).unwrap_err();

    assert_eq!(error.code, "init");
    assert_eq!(
        error.error,
        format!(
            "{} changed while Atlas was updating it; run atlas init again",
            config::tilde(&path)
        )
    );
    assert!(!error.uncertain);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "the refused replacement leaves no temporary file"
    );
}

#[test]
fn replacing_a_link_refuses_without_changing_the_link_or_its_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let target = root.join("target.toml");
    let link = root.join("atlas.toml");
    let previous = configuration_at(&root);
    write_config(&target, &previous).unwrap();
    symlink(&target, &link).unwrap();
    let before = fs::read(&target).unwrap();
    let replacement = Config {
        helper_uid: 60002,
        ..previous.clone()
    };

    let error = replace_config(&link, &previous, &replacement).unwrap_err();

    assert_eq!(error.code, "init");
    assert_eq!(
        error.error,
        format!(
            "{} is a link, and its helper account changed. Update the file it points to (or remove the link), then run atlas init again.",
            config::tilde(&link)
        )
    );
    assert!(!error.uncertain);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read(&target).unwrap(), before);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
}

#[test]
fn a_new_configuration_creates_private_nested_directories() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    let path = root.join("private/nested/atlas.toml");
    let config = configuration_at(&root);

    write_config(&path, &config).unwrap();

    for directory in [root.join("private"), root.join("private/nested")] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(Config::load(&path).unwrap(), config);
}
