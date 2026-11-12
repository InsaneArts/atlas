//! Switching this Mac to the user desktop: its login window, with the
//! helper's account to choose.
use crate::{cli, prompt};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};

/// `atlas switch` among this Mac's accounts, the second login `login`
/// among them when there is one, and the one chosen when none is named.
pub(crate) fn switch_on_this_mac(
    login: Option<&atlas_core::user::User>,
    args: cli::Switch,
) -> Result<Value> {
    let accounts = atlas_client::user::switch::accounts(login.map(|login| login.uid))?;

    if args.list {
        return Ok(json!({"ok": true, "accounts": accounts}));
    }

    let target = match args.account {
        Some(name) => Some(
            accounts
                .iter()
                .find(|candidate| candidate.name == name)
                .cloned()
                .ok_or_else(|| {
                    Error::new(
                        Code::Switch,
                        format!("No account named {name} to switch to"),
                    )
                })?,
        ),
        None if args.lock => None,
        None => accounts.iter().find(|candidate| candidate.helper).cloned(),
    };

    if !args.yes {
        let question = match &target {
            Some(account) => format!(
                "Lock the screen so you can switch to {}?",
                account.real_name.as_deref().unwrap_or(&account.name)
            ),
            None => "Lock the screen now?".to_owned(),
        };

        if !prompt::confirm(&question, true, "pass --yes")? {
            return Err(prompt::cancelled());
        }
    }

    match target {
        Some(account) => atlas_client::user::switch::to(&account),
        None => atlas_client::user::switch::lock_screen(),
    }
}
