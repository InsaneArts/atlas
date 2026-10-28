//! The helper account as Atlas knew it when it was chosen: its user ID, its
//! name and its GeneratedUID, which macOS never gives to another account. A
//! user ID alone can pass to a new account once the helper is deleted, so
//! nothing that ends a login of the helper's user ID, or deletes its account,
//! acts unless the account there is still the one recorded; root checks the
//! same again itself.
use super::Account;
use crate::{
    config::{self, save_private},
    error::Result,
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
    pub uid: u32,
    pub name: String,
    pub generated_uid: String,
}

/// Where the identity of helper `uid` is kept, in the home `home`.
fn path_in(home: &Path, uid: u32) -> PathBuf {
    home.join("Library/Application Support/Atlas")
        .join(format!("helper-{uid}.identity.json"))
}

/// The GeneratedUID in `dscl . -read /Users/NAME GeneratedUID` output, when
/// it has the form macOS gives it.
fn parse_generated_uid(text: &str) -> Option<String> {
    let value = text.trim().strip_prefix("GeneratedUID:")?.trim();
    let groups: Vec<&str> = value.split('-').collect();
    let lengths: Vec<usize> = groups.iter().map(|group| group.len()).collect();
    let hex = groups
        .iter()
        .all(|group| group.bytes().all(|byte| byte.is_ascii_hexdigit()));

    (lengths == [8, 4, 4, 4, 12] && hex).then(|| value.to_owned())
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
pub enum OnFile {
    /// Nothing: set up before Atlas recorded it, or never.
    Nothing,
    /// The helper's identity, as recorded.
    Recorded(Identity),
    /// A record that cannot be read: never taken for nothing.
    Unreadable,
}

/// How the account with a helper's user ID stands against its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
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

/// How the account read now (`now`) stands against what is on file.
pub fn standing(on_file: &OnFile, now: Option<&Identity>) -> Standing {
    match (on_file, now) {
        (OnFile::Unreadable, _) | (_, None) => Standing::Unknown,
        (OnFile::Nothing, Some(now)) => Standing::Unrecorded(now.clone()),
        (OnFile::Recorded(recorded), Some(now)) if recorded == now => {
            Standing::Same(recorded.clone())
        }
        (OnFile::Recorded(_), Some(_)) => Standing::Replaced,
    }
}

/// Record `account` as the helper, read now. Only a helper the person has
/// just chosen (`chosen`) replaces a record of another account; otherwise
/// a record is only ever written where there is none, and a different one
/// is refused.
pub fn remember(account: &Account, chosen: bool) -> Result<Identity> {
    let identity = current(account).ok_or_else(|| {
        crate::error::Error::new(
            "setup",
            format!("macOS could not say which account {} is", account.name),
        )
    })?;
    remember_in(&config::home()?, &identity, chosen)?;

    Ok(identity)
}

/// Record the identity the person chose, refusing a replacement discovered
/// after authorization. The record always contains `expected`, never a newer
/// directory entry observed while this call was running.
pub fn remember_as(account: &Account, expected: &Identity, chosen: bool) -> Result<()> {
    if current(account).as_ref() != Some(expected)
        || account.uid != expected.uid
        || account.name != expected.name
    {
        return Err(crate::error::Error::new(
            "setup",
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
                return Err(crate::error::Error::new(
                    "setup",
                    format!(
                        "The account with user ID {} is not the helper Atlas set up. Choose the \
                         helper again.",
                        identity.uid
                    ),
                ));
            }
        }
    }

    save_private(
        &path_in(home, identity.uid),
        &serde_json::to_vec_pretty(identity)?,
    )
}

/// What is on file for helper `uid`.
pub fn on_file(uid: u32) -> OnFile {
    match config::home() {
        Ok(home) => on_file_in(&home, uid),
        Err(_) => OnFile::Unreadable,
    }
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

/// How the account with helper `uid`'s user ID, `account`, stands now.
pub fn standing_of(account: &Account) -> Standing {
    standing(&on_file(account.uid), current(account).as_ref())
}

/// The helper `uid` as recorded, while the account with that user ID is
/// still that one; `None` when nothing is recorded, the account is gone or
/// another, or it cannot be read.
pub fn verified(uid: u32) -> Option<Identity> {
    let account = super::account_with_uid(uid).ok().flatten()?;

    match standing_of(&account) {
        Standing::Same(identity) => Some(identity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opie() -> Identity {
        Identity {
            uid: 502,
            name: "opie".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        }
    }

    #[test]
    fn only_a_generated_uid_of_macos_form_is_read() {
        assert_eq!(
            parse_generated_uid("GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B4558946919\n"),
            Some("6DB4B7AF-BE3C-486C-BD46-1B4558946919".into())
        );
        for text in [
            "",
            "GeneratedUID:",
            "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46",
            "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B455894691Z",
            "GeneratedUID: 6DB4B7AF BE3C 486C BD46 1B4558946919",
            "RealName: Opie",
        ] {
            assert_eq!(parse_generated_uid(text), None, "{text}");
        }
    }

    #[test]
    fn a_recorded_identity_is_read_back_only_for_its_own_user_id() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(on_file_in(home.path(), 502), OnFile::Nothing);

        remember_in(home.path(), &opie(), false).unwrap();
        assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(opie()));
        assert_eq!(on_file_in(home.path(), 503), OnFile::Nothing);

        fs::write(path_in(home.path(), 504), br#"{"uid": 502}"#).unwrap();
        assert_eq!(on_file_in(home.path(), 504), OnFile::Unreadable);
    }

    fn reused() -> Identity {
        Identity {
            generated_uid: "11111111-2222-3333-4444-555555555555".into(),
            ..opie()
        }
    }

    #[test]
    fn explicitly_choosing_a_legacy_helper_records_its_identity() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(on_file_in(home.path(), 502), OnFile::Nothing);
        remember_in(home.path(), &opie(), true).unwrap();
        assert_eq!(
            standing(&on_file_in(home.path(), 502), Some(&opie())),
            Standing::Same(opie())
        );
    }

    #[test]
    fn an_account_that_took_the_helpers_user_id_is_not_the_helper() {
        let recorded = OnFile::Recorded(opie());
        assert_eq!(standing(&recorded, Some(&opie())), Standing::Same(opie()));
        assert_eq!(
            standing(&recorded, Some(&reused())),
            Standing::Replaced,
            "same name, new account"
        );
        let renamed = Identity {
            name: "ann".into(),
            ..opie()
        };
        assert_eq!(standing(&recorded, Some(&renamed)), Standing::Replaced);
        assert_eq!(standing(&recorded, None), Standing::Unknown);
        assert_eq!(
            standing(&OnFile::Unreadable, Some(&opie())),
            Standing::Unknown
        );
        assert_eq!(
            standing(&OnFile::Nothing, Some(&opie())),
            Standing::Unrecorded(opie())
        );
    }

    #[test]
    fn a_record_of_another_account_is_replaced_only_by_a_new_choice() {
        let home = tempfile::tempdir().unwrap();
        remember_in(home.path(), &opie(), false).unwrap();

        // The user ID went to another account: repairing never records it.
        assert!(remember_in(home.path(), &reused(), false).is_err());
        assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(opie()));
        remember_in(home.path(), &opie(), false).unwrap();

        // An unreadable record is never overwritten by a repair either.
        fs::write(path_in(home.path(), 502), b"{").unwrap();
        assert!(remember_in(home.path(), &opie(), false).is_err());

        // The person chose it as the new helper.
        remember_in(home.path(), &reused(), true).unwrap();
        assert_eq!(on_file_in(home.path(), 502), OnFile::Recorded(reused()));
    }

    #[test]
    fn this_accounts_own_identity_is_read_from_its_directory() {
        let me = nix::unistd::getuid().as_raw();
        let account = super::super::account_with_uid(me).unwrap().unwrap();
        let identity = current(&account).unwrap();
        assert_eq!(identity.uid, me);
        assert_eq!(identity.name, account.name);
    }
}
