//! The helper account as Atlas knew it when it was chosen: its user ID, its
//! name and its GeneratedUID, which macOS never gives to another account. A
//! user ID alone can pass to a new account once the helper is deleted, so
//! nothing that ends a login of the helper's user ID, or deletes its account,
//! acts unless the account there is still the one recorded; root checks the
//! same again itself.
use super::problem;
use crate::{
    account::{Account, account_with_uid},
    config,
    error::Result,
    files::save_private,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// One account, as directory services name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub(crate) uid: u32,
    pub(crate) name: String,
    pub(crate) generated_uid: String,
}

fn path_in(home: &Path, uid: u32) -> PathBuf {
    home.join("Library/Application Support/Atlas")
        .join(format!("helper-{uid}.identity.json"))
}

/// The GeneratedUID in `dscl . -read /Users/NAME GeneratedUID` output, when
/// it has the form macOS gives it.
fn parse_generated_uid(text: &str) -> Option<String> {
    let value = text.trim().strip_prefix("GeneratedUID:")?.trim();
    let mut groups = value.split('-');
    let valid = [8, 4, 4, 4, 12].into_iter().all(|length| {
        groups.next().is_some_and(|group| {
            group.len() == length && group.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    });
    (valid && groups.next().is_none()).then(|| value.to_owned())
}

/// The account `account` as directory services name it now; `None` when its
/// GeneratedUID cannot be read.
pub fn current(account: &Account) -> Option<Identity> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([
            ".",
            "-read",
            &format!("/Users/{}", account.name),
            "GeneratedUID",
        ]),
        crate::bounded::LOOKUP,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }

    Some(Identity {
        uid: account.uid,
        name: account.name.clone(),
        generated_uid: parse_generated_uid(&String::from_utf8_lossy(&output.stdout))?,
    })
}

/// What is on file for one helper user ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OnFile {
    /// Nothing: no helper has been chosen for this user ID.
    Nothing,
    /// The helper's identity, as recorded.
    Recorded(Identity),
    /// A record that cannot be read: never taken for nothing.
    Unreadable,
}

/// How the account with a helper's user ID stands against its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Nothing is recorded, and the account can be read: it may be recorded.
    Unrecorded(Identity),
    /// It is the account recorded.
    Same(Identity),
    /// It is another account than the one recorded: only an explicit new
    /// choice of helper replaces the record.
    Replaced,
    /// The account or its record cannot be read.
    Unknown,
}

pub(crate) fn standing(on_file: &OnFile, now: Option<&Identity>) -> Standing {
    match (on_file, now) {
        (OnFile::Unreadable, _) | (_, None) => Standing::Unknown,
        (OnFile::Nothing, Some(now)) => Standing::Unrecorded(now.clone()),
        (OnFile::Recorded(recorded), Some(now)) if recorded == now => {
            Standing::Same(recorded.clone())
        }
        (OnFile::Recorded(_), Some(_)) => Standing::Replaced,
    }
}

/// Record the identity the person chose, refusing a replacement discovered
/// after authorization. The record always contains `expected`, never a newer
/// directory entry observed while this call was running.
pub(crate) fn remember_as(account: &Account, expected: &Identity, chosen: bool) -> Result<()> {
    if current(account).as_ref() != Some(expected)
        || account.uid != expected.uid
        || account.name != expected.name
    {
        return Err(problem(
            "The chosen helper account changed; choose it again",
        ));
    }
    remember_in(&config::home()?, expected, chosen)
}

fn remember_in(home: &Path, identity: &Identity, chosen: bool) -> Result<()> {
    if !chosen {
        match standing(&on_file_in(home, identity.uid), Some(identity)) {
            Standing::Same(_) => return Ok(()),
            Standing::Unrecorded(_) => {}
            Standing::Replaced | Standing::Unknown => {
                return Err(problem(format!(
                    "The account with user ID {} is not the helper Atlas set up. Choose the \
                     helper again.",
                    identity.uid
                )));
            }
        }
    }

    save_private(
        &path_in(home, identity.uid),
        &serde_json::to_vec_pretty(identity)?,
    )
}

pub(crate) fn on_file(uid: u32) -> OnFile {
    config::home().map_or(OnFile::Unreadable, |home| on_file_in(&home, uid))
}

fn on_file_in(home: &Path, uid: u32) -> OnFile {
    let bytes = match fs::read(path_in(home, uid)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return OnFile::Nothing,
        Err(_) => return OnFile::Unreadable,
    };

    match serde_json::from_slice::<Identity>(&bytes) {
        Ok(identity) if identity.uid == uid => OnFile::Recorded(identity),
        _ => OnFile::Unreadable,
    }
}

pub(crate) fn standing_of(account: &Account) -> Standing {
    standing(&on_file(account.uid), current(account).as_ref())
}

/// The helper `uid` as recorded, while the account with that user ID is
/// still that one; `None` when nothing is recorded, the account is gone or
/// another, or it cannot be read.
pub(crate) fn verified(uid: u32) -> Option<Identity> {
    let account = account_with_uid(uid).ok().flatten()?;

    match standing_of(&account) {
        Standing::Same(identity) => Some(identity),
        Standing::Unrecorded(_) | Standing::Replaced | Standing::Unknown => None,
    }
}

#[cfg(test)]
mod tests;
