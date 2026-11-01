//! Shared configuration construction and persistence for setup workflows.
use crate::{
    config::{self, Config},
    error::Result,
    setup::{self, helper::Layout},
    ui::text,
};
use std::{fs, io::Write, os::unix::fs::DirBuilderExt, path::Path};

use super::account::problem;

/// The configuration for `helper` in the managed private layout.
pub(crate) fn proposed(helper: &setup::Account) -> Result<Config> {
    let layout = Layout::new(helper.uid, helper.home.clone());

    Ok(Config {
        helper_uid: helper.uid,
        socket_path: layout.socket(),
        identity_probe: "/Users/Shared/Atlas/atlas".into(),
        input_socket: layout.input_socket(),
        output_dir: config::home()?.join("Library/Application Support/Atlas/runs"),
    })
}

/// Write a new configuration atomically, readable only by this account.
pub(crate) fn write_config(path: &Path, config: &Config) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| problem("The configuration needs a directory"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;

    let toml = config.to_toml()?;
    config::write_beside(
        path,
        0o600,
        |file| Ok(file.write_all(toml.as_bytes())?),
        // Never replaces a configuration that appeared meanwhile.
        |temporary| Ok(fs::hard_link(temporary, path)?),
    )
}

/// Replace the configuration that named `previous` (its helper account is
/// gone or changed), atomically, and only while the file still says so.
pub(crate) fn replace_config(path: &Path, previous: &Config, config: &Config) -> Result<()> {
    // Replacing a link would leave the file it points to unchanged.
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(problem(format!(
            "{} is a link, and its helper account changed. Update the file it points to (or \
             remove the link), then run atlas init again.",
            text::tilde(path)
        )));
    }

    let toml = config.to_toml()?;
    config::write_beside(
        path,
        0o600,
        |file| Ok(file.write_all(toml.as_bytes())?),
        |temporary| {
            if Config::load(path).ok().as_ref() != Some(previous) {
                return Err(problem(format!(
                    "{} changed while Atlas was updating it; run atlas init again",
                    text::tilde(path)
                )));
            }
            Ok(fs::rename(temporary, path)?)
        },
    )
}

/// Keep the selected account through configuration, refusing UID reassignment
/// before recording it. The recorder also checks its permanent identity.
fn record_selected(
    helper: &setup::Account,
    expected: &setup::identity::Identity,
    lookup: impl FnOnce(u32) -> Result<Option<setup::Account>>,
    record: impl FnOnce(&setup::Account, &setup::identity::Identity) -> Result<()>,
) -> Result<()> {
    if lookup(helper.uid)?.as_ref() != Some(helper) {
        return Err(problem(
            "The chosen helper account changed; choose it again",
        ));
    }
    record(helper, expected)
}

/// The settings and the standard helper account they name: chosen (or
/// created) when there are none or their account is gone, retaining its
/// permanent identity throughout configuration and setup.
pub fn configure_helper(
    config_path: &Path,
    mut choose_helper: impl FnMut() -> Result<(setup::Account, setup::identity::Identity)>,
    say: impl Fn(&str),
) -> Result<(Config, setup::Account, setup::identity::Identity)> {
    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    let absent = matches!(
        fs::symlink_metadata(config_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    // Whether the person chose the helper in this run: only then may its
    // record replace one of another account.
    let mut choice = None;
    let mut config = if absent {
        let (helper, identity) = choose_helper()?;
        let config = proposed(&helper)?;
        choice = Some((helper, identity));
        write_config(config_path, &config)?;
        config
    } else {
        Config::load(config_path)?
    };

    // The account the settings name was deleted, or its user ID now belongs
    // to another account than the helper Atlas recorded: choose (or create)
    // another, before anything is changed or recorded.
    let account = setup::account_with_uid(config.helper_uid)?;
    let gone = account.is_none();
    let other = match &account {
        Some(account) if choice.is_none() => replaced(account)?,
        _ => false,
    };
    if choice.is_none() && (gone || other) {
        say(&format!(
            "  {} (user ID {}) {}.",
            if gone {
                format!("The helper account in {}", text::tilde(config_path))
            } else {
                "The account".to_owned()
            },
            config.helper_uid,
            if gone {
                "no longer exists"
            } else {
                "is not the helper Atlas set up"
            }
        ));

        let (helper, identity) = choose_helper()?;
        let replacement = proposed(&helper)?;
        replace_config(config_path, &config, &replacement)?;
        config = replacement;
        choice = Some((helper, identity));
    }

    let chosen = choice.is_some();
    let (helper, identity) = match choice {
        Some(selected) => selected,
        None => {
            let helper = account.ok_or_else(|| problem("The helper account was not created"))?;
            let identity = match setup::identity::standing_of(&helper) {
                setup::identity::Standing::Same(identity) => identity,
                _ => {
                    return Err(problem(
                        "Choose the helper account again before setting it up",
                    ));
                }
            };
            (helper, identity)
        }
    };
    record_selected(
        &helper,
        &identity,
        setup::account_with_uid,
        |helper, identity| setup::identity::remember_as(helper, identity, chosen),
    )?;

    // Keep the socket paths in the selected account's home.
    if !config.socket_path.starts_with(&helper.home) {
        let fresh = proposed(&helper)?;
        let current = Config {
            socket_path: fresh.socket_path,
            input_socket: fresh.input_socket,
            ..config.clone()
        };
        replace_config(config_path, &config, &current)?;
        config = current;
    }

    let name = &helper.name;
    match setup::administrator(name) {
        Some(false) => {}
        Some(true) => {
            return Err(problem(format!(
                "{name} administers this Mac, and the helper must be a standard account. Make it \
                 a standard account in System Settings › Users & Groups, then run atlas init again."
            )));
        }
        None => {
            return Err(problem(format!(
                "macOS could not say whether {name} is an administrator; run atlas init again"
            )));
        }
    }

    // Which account this is, so nothing later acts on another account that
    // is given its user ID. Unrecorded, nothing later ends a login under
    // its user ID or deletes it.
    setup::identity::remember_as(&helper, &identity, chosen)?;

    Ok((config, helper, identity))
}

/// Whether the account with the helper's user ID is not the helper Atlas
/// recorded: another account took the user ID. A record that cannot be
/// compared stops init before anything changes.
fn replaced(account: &setup::Account) -> Result<bool> {
    use setup::identity::{OnFile, Standing};

    match setup::identity::standing_of(account) {
        Standing::Replaced | Standing::Unrecorded(_) => Ok(true),
        Standing::Unknown if setup::identity::on_file(account.uid) != OnFile::Nothing => {
            Err(problem(format!(
                "macOS could not say whether {} is still the helper Atlas set up; run atlas init \
                 again",
                account.name
            )))
        }
        Standing::Same(_) | Standing::Unknown => Ok(false),
    }
}

#[cfg(test)]
mod tests {
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
                text::tilde(&path)
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
                text::tilde(&link)
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
}
