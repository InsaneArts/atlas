//! The onboarding steps that are the person's own preferences, whatever
//! desktop Atlas uses: opening the menu bar at login, and whether agents
//! may copy sign-ins in. Setting up the desktop itself is its kind's.
use atlas_client::setup::executor::problem;
use atlas_core::{config, error::Result};
use serde_json::{Value, json};
use std::path::Path;

/// Open the menu bar at login, or not, as the person chose. As with `atlas
/// init`, only for the default settings, the ones the login item uses.
///
/// # Errors
///
/// Returns configuration, helper-identity or login-item update failures.
pub fn login(config_path: &Path, on: bool) -> Result<Value> {
    let default = || {
        config::installed_path()
            .is_ok_and(|default| atlas_agents::same_place(&default, config_path))
    };

    if on && !default() {
        return Err(problem(format!(
            "Open at Login uses the default settings; start the menu bar with atlas menubar \
             --config {}",
            config_path.display()
        )));
    }

    crate::menubar::set_login(if on {
        crate::menubar::Login::On
    } else {
        crate::menubar::Login::Off
    })
}

/// Let agents copy sign-ins and secrets into the helper (`Some(true)`, the
/// person only), stop them (`Some(false)`), or say which it is.
///
/// # Errors
///
/// Returns `stopped` when an agent asks to allow copies, and file errors
/// when the choice can't be written.
pub fn sign_ins(allow: Option<bool>, by_person: bool) -> Result<Value> {
    let allowed = match allow {
        Some(allow) => atlas_client::sign_in_copies::set(allow, by_person)?,
        None => atlas_client::sign_in_copies::allowed(),
    };
    Ok(json!({"ok": true, "signInCopies": allowed}))
}
