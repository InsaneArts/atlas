//! The onboarding steps that are the person's own preferences, whatever
//! desktop Atlas uses: opening the menu bar at login, and whether agents
//! may copy sign-ins in. Setting up the desktop itself is its kind's.
use atlas_client::setup::executor::problem;
use atlas_core::{
    config::{
        self,
        desktops::{Chosen, Selected},
    },
    error::{Code, Error, Result},
};
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

/// What the person hears of copying sign-ins in, or of the home folder's
/// privacy from the helper, on a VM: both are Second Login's.
const SECOND_LOGIN_ONLY: &str = "Copying your sign-ins works only with Second Login.";

/// Refuse the desktop the settings at `config_path` choose when it is a VM,
/// before anything is read or changed: copying sign-ins in and the home
/// folder's privacy are Second Login's alone. Settings that cannot be read
/// are left to the step's own refusal.
///
/// # Errors
///
/// Refuses a VM: copying sign-ins in works only with Second Login.
pub fn second_login_only(config_path: &Path) -> Result<()> {
    match Selected::load(config_path) {
        Ok(selected) if matches!(selected.desktop, Chosen::Vm(_)) => {
            Err(Error::new(Code::Platform, SECOND_LOGIN_ONLY))
        }
        Ok(_) | Err(_) => Ok(()),
    }
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

#[cfg(test)]
#[path = "onboarding_tests.rs"]
mod tests;
