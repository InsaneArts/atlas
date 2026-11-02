use atlas_core::{
    account::{FIRST_USER_UID, account_with_uid},
    error::{Code, Error, Result},
};
use std::process::Command;

pub fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Init, message)
}

/// Whether `name` can be a new account's short name.
///
/// # Errors
///
/// Rejects names outside the supported account-name syntax or length without performing
/// account changes.
pub fn validate_account_name(name: &str) -> std::result::Result<(), String> {
    if name.chars().any(char::is_whitespace) {
        return Err("Account names can't have spaces.".into());
    }

    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(
            "Account names can only use letters, numbers, dots, dashes and underscores.".into(),
        );
    }

    if name.starts_with(['.', '-']) {
        return Err("Account names start with a letter or a number.".into());
    }

    if name.is_empty() || name.len() > 31 {
        return Err("Account names are 1 to 31 characters.".into());
    }

    // macOS reads a name of only digits as a user ID.
    if name.bytes().all(|c| c.is_ascii_digit()) {
        return Err("Account names need at least one letter.".into());
    }

    Ok(())
}

/// The full name a new helper account gets.
pub const FULL_NAME: &str = "Atlas";

pub const SYSADMINCTL: &str = "/usr/sbin/sysadminctl";

/// The arguments that make `sysadminctl` create a standard account `name`
/// called `full_name` with user ID `uid`, asking for its password itself
/// (`-password -`), with Atlas's picture when it is at hand.
pub fn add_user_args(name: &str, full_name: &str, uid: u32) -> Vec<String> {
    let mut args: Vec<String> = [
        "-addUser",
        name,
        "-fullName",
        full_name,
        "-UID",
        &uid.to_string(),
        "-password",
        "-",
    ]
    .map(str::to_owned)
    .to_vec();

    if let Some(picture) = helper_picture() {
        args.extend(["-picture".to_owned(), picture]);
    }

    args
}

/// The picture a new helper account gets, Atlas's mark: shipped in
/// Atlas.app, or in a development checkout drawn into app/Resources.
/// macOS keeps a copy on the account, so the file need not stay.
pub fn helper_picture() -> Option<String> {
    let bundled = atlas_core::app_bundle::installed_app()
        .ok()
        .map(|app| app.join("Contents/Resources/HelperPicture.png"));
    let checkout = atlas_core::app_bundle::checkout().join("app/Resources/HelperPicture.png");
    [bundled, Some(checkout)]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .and_then(|path| path.to_str().map(str::to_owned))
}

/// The lowest user ID from the first user ID (501) that no account has and
/// no process runs as: a deleted account's session can outlive it, and
/// sharing its ID would mix that session with the helper's.
///
/// # Errors
///
/// Returns an error when the user IDs in use can't be listed.
pub fn free_uid() -> Result<u32> {
    let taken = |program: &str, args: &[&str]| -> Result<Vec<u32>> {
        let output = atlas_core::bounded::output(
            Command::new(program).args(args),
            atlas_core::bounded::LISTING,
        )?;
        if !output.status.success() {
            return Err(problem("Could not list the user IDs in use"));
        }
        used_uids(
            &String::from_utf8(output.stdout)
                .map_err(|_| problem("Could not read the user IDs in use"))?,
        )
    };

    let accounts = taken("/usr/bin/dscl", &["/Search", "-list", "/Users", "UniqueID"])?;
    let running = taken("/bin/ps", &["-axo", "uid="])?;
    available_uid(&accounts, &running, |uid| {
        Ok(account_with_uid(uid)?.is_some())
    })
}

/// Directory services can list negative system IDs, which cannot collide
/// with the helper. Any unreadable ID still makes the whole list unusable.
fn used_uids(listing: &str) -> Result<Vec<u32>> {
    let mut uids = Vec::new();
    for line in listing.lines() {
        let unreadable = || problem("Could not read a user ID in use");
        let uid = line
            .split_whitespace()
            .last()
            .and_then(|word| word.parse::<i64>().ok())
            .ok_or_else(unreadable)?;
        if uid >= 0 {
            uids.push(u32::try_from(uid).map_err(|_| unreadable())?);
        }
    }
    Ok(uids)
}

fn available_uid(
    accounts: &[u32],
    running: &[u32],
    mut resolves: impl FnMut(u32) -> Result<bool>,
) -> Result<u32> {
    for uid in FIRST_USER_UID..600 {
        if !accounts.contains(&uid) && !running.contains(&uid) && !resolves(uid)? {
            return Ok(uid);
        }
    }
    Err(problem("No free user ID for the helper account"))
}

#[cfg(test)]
mod tests;
