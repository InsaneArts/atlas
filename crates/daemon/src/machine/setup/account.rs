//! The guest's accounts: a second administrator for what asks for one, the
//! guest account taken out of the administrators, and Remote Login closed.
//! Each property is made true on its own, so a phase cut short anywhere is
//! finished when asked again.
use super::{
    Places, System,
    record::{Admin, Bootstrap},
    refused,
};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::io::Read;

/// The second administrator's full name, which is how this setup knows an
/// account it began making.
pub(super) const FULL_NAME: &str = "Atlas Administrator";

/// The longest password line read.
const LONGEST: u64 = 256;

/// The password on standard input's second line: the first is `sudo`'s.
pub(super) fn password_line(input: &mut impl Read) -> Result<String> {
    let mut text = String::new();
    input.take(LONGEST * 2).read_to_string(&mut text)?;
    let line = text
        .lines()
        .next()
        .unwrap_or_default()
        .trim_end_matches('\r');
    if line.is_empty() || line.len() as u64 > LONGEST {
        return Err(Error::new(Code::BadRequest, "No password was given."));
    }
    Ok(line.to_owned())
}

/// Make `name` the second administrator, its password `password`.
pub(super) fn admin(
    places: &Places,
    system: &impl System,
    name: &str,
    password: &str,
) -> Result<Value> {
    let mut kept = Bootstrap::read(places)?;
    let ours = kept.admin.as_ref().filter(|admin| admin.name == name);
    match system.account(name) {
        Some(account) => {
            // An account there is this phase's only when its record says it
            // was being made, or made, and it has the full name it was given.
            let began = ours.is_some_and(|admin| admin.uid.is_none_or(|uid| uid == account.uid));
            if !began || account.full_name.as_deref() != Some(FULL_NAME) {
                return Err(refused("an account of that name is not this setup's"));
            }
            if kept.admin.as_ref().and_then(|admin| admin.uid).is_none() {
                kept.admin = Some(Admin {
                    name: name.into(),
                    uid: Some(account.uid),
                });
                kept.write(places)?;
            }
        }
        None => {
            // Kept as being made before it is, so a crash after is known.
            kept.admin = Some(Admin {
                name: name.into(),
                uid: None,
            });
            kept.write(places)?;
            if !system.create_admin(name, FULL_NAME, password) {
                return Err(refused("the second administrator could not be made"));
            }
            let Some(account) = system.account(name) else {
                return Err(refused("the second administrator was not made"));
            };
            kept.admin = Some(Admin {
                name: name.into(),
                uid: Some(account.uid),
            });
            kept.write(places)?;
        }
    }
    if !system.set_password(name, password) {
        return Err(refused(
            "the second administrator's password could not be set",
        ));
    }
    if !system.set_admin(name, true) || system.is_admin(name) != Some(true) {
        return Err(refused("the second administrator is not an administrator"));
    }
    if !system.hide(name) {
        return Err(refused("the second administrator could not be hidden"));
    }
    Ok(json!({"ok": true, "admin": name}))
}

/// Take `account` out of the administrators, and see that it is.
pub(super) fn demote(system: &impl System, account: &str) -> Result<Value> {
    if system.is_admin(account) == Some(false) {
        return Ok(json!({"ok": true, "demoted": account}));
    }
    if !system.set_admin(account, false) || system.is_admin(account) != Some(false) {
        return Err(refused("the guest account is still an administrator"));
    }
    Ok(json!({"ok": true, "demoted": account}))
}

/// Turn Remote Login off, a second after this answers.
pub(super) fn close(system: &impl System) -> Result<Value> {
    if !system.close_remote_login() {
        return Err(refused("Remote Login could not be turned off"));
    }
    Ok(json!({"ok": true, "closing": true}))
}
