//! Shared configuration construction and persistence for setup workflows.
use crate::{
    config::{self, Config},
    error::{Code, Error, Result},
    setup::{self, helper::Layout},
    ui::text,
};
use std::{fs, io::Write, os::unix::fs::DirBuilderExt, path::Path};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Init, message)
}

/// The configuration for `helper` in the managed private layout.
pub(crate) fn proposed(helper: &setup::Account) -> Result<Config> {
    let layout = Layout {
        uid: helper.uid,
        home: helper.home.clone(),
        guard: None,
    };

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

#[cfg(test)]
mod tests {
    use super::*;
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
