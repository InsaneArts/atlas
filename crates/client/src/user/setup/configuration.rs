//! Shared configuration construction and persistence for setup workflows.
use crate::setup::problem;
use atlas_core::{
    account,
    config::{self, Config},
    error::Result,
};
use atlas_daemon::identity::{Identity, Standing};
use std::{fs, path::Path};

/// The configuration for `helper` in the managed private layout.
///
/// # Errors
///
/// Returns an error when this account's home is unavailable.
pub fn proposed(helper: &account::Account) -> Result<Config> {
    let layout = atlas_daemon::user::setup::Layout::new(helper.uid, helper.home.clone());

    Ok(Config {
        helper_uid: helper.uid,
        socket_path: layout.paths().socket(),
        identity_probe: "/Users/Shared/Atlas/atlas".into(),
        input_socket: layout.paths().input_socket(),
        output_dir: config::runs_folder()?,
        activity: config::Activity::default(),
    })
}

fn managed(config: &Config, layout: &atlas_daemon::user::setup::Layout) -> Config {
    Config {
        socket_path: layout.paths().socket(),
        input_socket: layout.paths().input_socket(),
        ..config.clone()
    }
}

/// `config`, its sockets moved into `helper`'s home when they are elsewhere.
pub fn in_home(config: &Config, helper: &account::Account) -> Config {
    if config.socket_path.starts_with(&helper.home) {
        config.clone()
    } else {
        managed(
            config,
            &atlas_daemon::user::setup::Layout::new(helper.uid, helper.home.clone()),
        )
    }
}

pub fn absent(path: &Path) -> bool {
    matches!(
        fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// Write a new configuration, readable only by this account, through the one
/// settings writer: made only where there is none.
///
/// # Errors
///
/// Returns the failure to make the folder or write the file, including when one
/// is already there.
pub fn write_config(path: &Path, config: &Config) -> Result<()> {
    let toml = config.to_toml()?;
    config::write::update(path, |current| match current {
        // Never replaces a configuration that appeared meanwhile.
        Some(_) => Err(problem(format!("{} is already there", config::tilde(path)))),
        None => Ok(Some(config::write::Edit::text(toml.clone()))),
    })
    .map(|_| ())
}

/// Replace the configuration that named `previous` (its helper account is
/// gone or changed), through the one settings writer, and only while the
/// file still says so.
///
/// # Errors
///
/// Returns the failure to write the file, or a refusal when it no longer names
/// `previous`.
pub fn replace_config(path: &Path, previous: &Config, config: &Config) -> Result<()> {
    // Replacing a link would leave the file it points to unchanged.
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(problem(format!(
            "{} is a link, and its helper account changed. Update the file it points to (or \
             remove the link), then run atlas init again.",
            config::tilde(path)
        )));
    }

    // Settings that list desktops keep every other one: only the Second
    // Login's helper changes, judged against the file as it is under the
    // settings lock, so a change made meanwhile is never lost.
    let changed = || {
        problem(format!(
            "{} changed while Atlas was updating it; run atlas init again",
            config::tilde(path)
        ))
    };
    config::write::update(path, |current| {
        let text = current.ok_or_else(changed)?;
        let current = config::desktops::Settings::parse(text.as_bytes(), path)
            .and_then(|settings| settings.user())
            .map_err(|error| changed().with_source(error))?;
        if !current.same_helper(previous) {
            return Err(changed());
        }
        // Only the helper changes: where records go and what the activity
        // keeps stay as the file says now, whoever changed them meanwhile.
        let helper = Config {
            output_dir: current.output_dir,
            activity: current.activity,
            ..config.clone()
        };
        let toml = config::desktops::with_user(text, path, &helper)?;
        Ok(Some(config::write::Edit::text(toml).private()))
    })
    .map(|_| ())
}

/// Keep the selected account through configuration, refusing UID reassignment
/// before recording it. The recorder also checks its permanent identity.
fn record_selected(
    helper: &account::Account,
    expected: &Identity,
    lookup: impl FnOnce(u32) -> Result<Option<account::Account>>,
    record: impl FnOnce(&account::Account, &Identity) -> Result<()>,
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
///
/// # Errors
///
/// Propagates account-choice, configuration, identity-record and filesystem failures.
/// Configuration written earlier in the workflow can remain for supported recovery.
pub fn configure_helper(
    config_path: &Path,
    mut choose_helper: impl FnMut() -> Result<(account::Account, Identity)>,
    say: impl Fn(&str),
) -> Result<(Config, account::Account, Identity)> {
    // The path is the one the command line resolved (the installed one by
    // default); a configuration is created only where none exists, and an
    // existing one that does not load is reported, never replaced.
    // Whether the person chose the helper in this run: only then may its
    // record replace one of another account.
    let mut choice = None;
    let mut config = if absent(config_path) {
        let (helper, identity) = choose_helper()?;
        let config = proposed(&helper)?;
        choice = Some((helper, identity));
        write_config(config_path, &config)?;
        config
    } else {
        Config::load_user(config_path)?
    };

    // The account the settings name was deleted, or its user ID now belongs
    // to another account than the helper Atlas recorded: choose (or create)
    // another, before anything is changed or recorded.
    let account = account::account_with_uid(config.user().uid)?;
    let gone = account.is_none();
    let other = match &account {
        Some(account) if choice.is_none() => replaced(account)?,
        _ => false,
    };
    if choice.is_none() && (gone || other) {
        let (who, what) = if gone {
            (
                format!("The helper account in {}", config::tilde(config_path)),
                "no longer exists",
            )
        } else {
            ("The account".to_owned(), "is not the helper Atlas set up")
        };
        say(&format!("  {who} (user ID {}) {what}.", config.user().uid));

        let (helper, identity) = choose_helper()?;
        // A repair: only the helper changes. Where records go and what the
        // activity keeps stay as the person set them.
        let replacement = Config {
            output_dir: config.output_dir.clone(),
            activity: config.activity.clone(),
            ..proposed(&helper)?
        };
        replace_config(config_path, &config, &replacement)?;
        config = replacement;
        choice = Some((helper, identity));
    }

    let chosen = choice.is_some();
    let (helper, identity) = match choice {
        Some(selected) => selected,
        None => {
            let helper = account.ok_or_else(|| problem("The helper account was not created"))?;
            let identity = match atlas_daemon::user::setup::identity::standing_of(&helper) {
                Standing::Same(identity) => identity,
                Standing::Unrecorded(_) | Standing::Replaced | Standing::Unknown => {
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
        account::account_with_uid,
        |helper, identity| {
            atlas_daemon::user::setup::identity::remember_as(helper, identity, chosen)
        },
    )?;

    // Keep the socket paths in the selected account's home.
    let homed = in_home(&config, &helper);
    if homed != config {
        replace_config(config_path, &config, &homed)?;
        config = homed;
    }

    let name = &helper.name;
    match atlas_core::user::account::administrator(name) {
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
    atlas_daemon::user::setup::identity::remember_as(&helper, &identity, chosen)?;

    Ok((config, helper, identity))
}

/// Whether the account with the helper's user ID is not the helper Atlas
/// recorded: another account took the user ID. A record that cannot be
/// compared stops init before anything changes.
fn replaced(account: &account::Account) -> Result<bool> {
    use atlas_daemon::identity::OnFile;

    match atlas_daemon::user::setup::identity::standing_of(account) {
        Standing::Replaced | Standing::Unrecorded(_) => Ok(true),
        Standing::Unknown
            if atlas_daemon::user::setup::identity::on_file(account.uid) != OnFile::Nothing =>
        {
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
#[path = "configuration_tests.rs"]
mod tests;
