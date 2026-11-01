use crate::error::{Code, Error, Result};
use crate::setup;
use serde::Serialize;
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    pub home: PathBuf,
}

enum Lookup {
    Name(CString),
    Uid(u32),
}

/// macOS numbers people's accounts from this user ID; lower ones belong to
/// the system.
pub const FIRST_USER_UID: u32 = 501;

#[allow(
    unsafe_code,
    reason = "FFI: getpw*_r, whose ERANGE nix misreads on macOS"
)]
fn lookup(key: Lookup) -> Result<Option<Account>> {
    // SAFETY: passwd is plain C data for which all-zero is a valid value.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found = std::ptr::null_mut();
    let mut buffer = Vec::new();
    let mut status = libc::ERANGE;

    // Directory-service records can exceed any fixed buffer; grow on ERANGE.
    // The error is the return value: macOS need not set errno here.
    for size in [16 << 10, 64 << 10, 256 << 10, 1 << 20] {
        buffer.resize(size, 0);

        // SAFETY: valid entry, buffer and result pointers that outlive the call;
        // the strings in `entry` point into `buffer` and are copied before return.
        status = unsafe {
            match &key {
                Lookup::Name(name) => libc::getpwnam_r(
                    name.as_ptr(),
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
                Lookup::Uid(uid) => libc::getpwuid_r(
                    *uid,
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                ),
            }
        };

        if status != libc::ERANGE {
            break;
        }
    }

    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status).into());
    }

    if found.is_null() {
        return Ok(None);
    }

    // SAFETY: getpw*_r succeeded, so both fields are NUL-terminated strings.
    let text = |field| {
        // SAFETY: see above.
        unsafe { CStr::from_ptr(field) }
            .to_string_lossy()
            .into_owned()
    };

    Ok(Some(Account {
        name: text(entry.pw_name),
        uid: entry.pw_uid,
        home: PathBuf::from(text(entry.pw_dir)),
    }))
}

/// Resolve an account through directory services by short name.
///
/// # Errors
///
/// Rejects a name containing NUL and returns directory-service lookup errors. A missing
/// account is represented by None.
pub fn account_named(name: &str) -> Result<Option<Account>> {
    let name = CString::new(name)
        .map_err(|_| Error::new(Code::Setup, "Account names cannot contain NUL"))?;
    lookup(Lookup::Name(name))
}

/// Resolve an account through directory services by UID.
///
/// # Errors
///
/// Returns directory-service lookup errors. A missing UID is represented by None rather
/// than an invented account.
pub fn account_with_uid(uid: u32) -> Result<Option<Account>> {
    lookup(Lookup::Uid(uid))
}

/// The full name of account `name`, as people know it, if it has one.
pub fn real_name(name: &str) -> Option<String> {
    asked_real_name(name).titled()
}

/// What directory services say of an account's full name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Title {
    /// They did not answer in time.
    Unknown,
    /// The account has none.
    Untitled,
    /// The account's full name.
    Titled(String),
}

/// What directory services say of account `name`'s full name.
pub(crate) fn asked_real_name(name: &str) -> Title {
    let Ok(output) = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-read", &format!("/Users/{name}"), "RealName"]),
        crate::bounded::LOOKUP,
    ) else {
        return Title::Unknown;
    };

    parse_real_name(&output.stdout).map_or(Title::Untitled, Title::Titled)
}

/// The full name in `dscl -read`'s answer for RealName, if it has one.
fn parse_real_name(answer: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(answer).ok()?;
    let value = text.strip_prefix("RealName:")?.trim().to_owned();

    (!value.is_empty()).then_some(value)
}

/// Whether `name` administers this Mac (a helper must not); None when macOS
/// cannot say, which never counts as "no".
pub fn administrator(name: &str) -> Option<bool> {
    // An agent's sandbox may keep dseditgroup from the directory service;
    // the account's group list still answers there.
    directory_member(name).or_else(|| listed_member(name))
}

/// What the directory service says of `name` in the admin group.
fn directory_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/sbin/dseditgroup").args(["-o", "checkmember", "-m", name, "admin"]),
        crate::bounded::LOOKUP,
    )
    .ok()?;

    let answer = String::from_utf8_lossy(&output.stdout);
    if answer.starts_with("yes ") {
        Some(true)
    } else if answer.starts_with("no ") {
        Some(false)
    } else {
        None
    }
}

/// The admin group's ID, the same on every Mac.
const ADMIN_GID: &str = "80";

/// Whether the admin group is among the group IDs `id` lists for `name`.
/// IDs, not names: a name `id` cannot resolve is printed as its number.
fn listed_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/id").args(["-G", name]),
        crate::bounded::LOOKUP,
    )
    .ok()
    .filter(|output| output.status.success())?;

    parse_admin_listed(&String::from_utf8_lossy(&output.stdout))
}

/// Whether `id -G`'s list holds the admin group's ID; None for no list.
fn parse_admin_listed(groups: &str) -> Option<bool> {
    let mut groups = groups.split_whitespace().peekable();
    groups.peek()?;

    Some(groups.any(|group| group == ADMIN_GID))
}

impl Title {
    /// The full name, when the account has one.
    pub(crate) fn titled(self) -> Option<String> {
        match self {
            Self::Titled(title) => Some(title),
            Self::Unknown | Self::Untitled => None,
        }
    }
}

pub(crate) fn problem(message: impl Into<String>) -> Error {
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

/// macOS's account tool, which creates the helper account.
pub(crate) const SYSADMINCTL: &str = "/usr/sbin/sysadminctl";

/// The arguments that make `sysadminctl` create a standard account `name`
/// called `full_name` with user ID `uid`, asking for its password itself
/// (`-password -`), with Atlas's picture when it is at hand.
pub(crate) fn add_user_args(name: &str, full_name: &str, uid: u32) -> Vec<String> {
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
pub(crate) fn helper_picture() -> Option<String> {
    let bundled = crate::update::installed_app()
        .ok()
        .map(|app| app.join("Contents/Resources/HelperPicture.png"));
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("app/Resources/HelperPicture.png");
    [bundled, Some(checkout)]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .and_then(|path| path.to_str().map(str::to_owned))
}

/// The lowest user ID from the first user ID (501) that no account has and
/// no process runs as: a deleted account's session can outlive it, and
/// sharing its ID would mix that session with the helper's.
pub(crate) fn free_uid() -> Result<u32> {
    let taken = |program: &str, args: &[&str]| -> Result<Vec<u32>> {
        let output =
            crate::bounded::output(Command::new(program).args(args), crate::bounded::LISTING)?;
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
        Ok(setup::account_with_uid(uid)?.is_some())
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
    for uid in setup::FIRST_USER_UID..600 {
        if !accounts.contains(&uid) && !running.contains(&uid) && !resolves(uid)? {
            return Ok(uid);
        }
    }
    Err(problem("No free user ID for the helper account"))
}

/// Create the selected standard account under claims for every affected UID.
/// Errors preserve incomplete creation and orphan cleanup for recovery.
///
/// # Errors
///
/// Rejects an invalid or conflicting account name and returns privilege, account-creation
/// or verification failures. Never retries an unknown creation outcome blindly.
pub fn create_helper(name: &str, full_name: &str) -> Result<Account> {
    let orphans = crate::session::sessions(None)
        .map(|sessions| sessions.leftover_uids(crate::session::Kind::Orphan))
        .ok_or_else(|| problem("macOS could not list its logins, so no account was made"))?;
    let uid = free_uid()?;
    crate::onboarding::closing_first(uid, &orphans, || {
        crate::onboarding::as_root::create_in_terminal(name, uid, full_name, &orphans)
    })?
    .map_err(|refusal| {
        problem(format!(
            "The helper account could not be created: {refusal:?}"
        ))
    })?;

    setup::account_named(name)?.ok_or_else(|| problem("The new account was not found"))
}

/// Close the logins deleted accounts left open, once the person agrees:
/// under sudo, which asks for their password here, all in one run as root.
/// Root checks each user ID again itself; only a login the helper's user ID
/// (`uid`, its account as just recorded: `helper`) was given after is closed
/// while its account exists, and then without killing anything.
///
/// # Errors
///
/// Returns identity, ownership or cleanup failures for the selected account's Atlas
/// resources. Unrelated or unverifiable resources are retained.
pub fn close_leftovers(
    uid: u32,
    helper: Option<&crate::setup::identity::Identity>,
    approve: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    use crate::onboarding;

    let Some(sessions) = crate::session::sessions(Some(uid)) else {
        return Ok(false);
    };
    let still_open =
        || problem("It's still open. Close it before you switch: run atlas init again.");
    let failed = |refusal| match refusal {
        crate::approval::Refusal::Failed(reason) => problem(format!(
            "macOS didn't close it: {reason}. Restart the Mac to clear it, then run atlas init again."
        )),
        _ => still_open(),
    };
    let targets = onboarding::leftover_targets(&sessions, helper).map_err(failed)?;
    if targets.is_empty() {
        return Ok(false);
    }
    approve()?;
    let uids: Vec<u32> = targets.iter().map(|target| target.uid()).collect();
    onboarding::close_batch(
        &onboarding::controllers_of(&uids)?,
        &targets,
        |targets| {
            crate::onboarding::as_root::close(
                targets,
                crate::onboarding::as_root::Via::Sudo { ask: false },
            )
        },
        crate::session::confirm_gone,
    )
    .map_err(|held| match held {
        onboarding::Held::State(error) => error,
        onboarding::Held::Refused(refusal) => failed(refusal),
    })?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounts_resolve_through_directory_services() {
        let me = account_with_uid(nix::unistd::getuid().as_raw())
            .unwrap()
            .unwrap();
        assert_eq!(account_named(&me.name).unwrap(), Some(me.clone()));
        assert!(account_named("no-such-atlas-account").unwrap().is_none());
        assert!(account_named("bad\0name").is_err());
    }

    #[test]
    fn group_lists_name_the_admin_group_exactly() {
        assert_eq!(parse_admin_listed("20 12 80 61\n"), Some(true));

        // An ID merely starting with 80 is not the admin group.
        assert_eq!(parse_admin_listed("20 12 801 61\n"), Some(false));

        assert_eq!(parse_admin_listed("\n"), None);
    }

    #[test]
    fn a_full_name_is_read_from_directory_services_answer_on_one_line_or_the_next() {
        assert_eq!(
            parse_real_name(b"RealName: Atlas\n").as_deref(),
            Some("Atlas")
        );
        assert_eq!(
            parse_real_name(b"RealName:\n Atlas Helper\n").as_deref(),
            Some("Atlas Helper")
        );
        assert_eq!(parse_real_name(b"RealName:\n"), None);
        assert_eq!(parse_real_name(b"No such key: RealName\n"), None);
    }
    #[test]
    fn uid_lists_ignore_negative_system_ids_and_keep_nonnegative_ids() {
        let accounts = used_uids(
            "nobody                  -2\ndaemon                   1\n_unknown                99\nme                     501\n",
        )
        .unwrap();
        assert_eq!(accounts, [1, 99, 501]);
        let running = used_uids("   -2\n     1\n    99\n   502\n").unwrap();
        assert_eq!(running, [1, 99, 502]);
        assert_eq!(
            available_uid(&accounts, &running, |_| Ok(false)).unwrap(),
            503
        );
        assert_eq!(
            used_uids("root 0\nhigh 4294967295\n").unwrap(),
            [0, u32::MAX]
        );
    }

    #[test]
    fn uid_lists_refuse_garbage_missing_ids_and_out_of_range_numbers() {
        for line in [
            "garbage",
            "broken NaN",
            "",
            "large 4294967296",
            "huge 9223372036854775808",
        ] {
            let listing = format!("nobody -2\ndaemon 1\n_unknown 99\n{line}\nme 501\n");
            let error = used_uids(&listing).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Could not read a user ID in use",
                "{line}"
            );
        }
    }

    #[test]
    fn uid_allocation_skips_directory_accounts_and_processes_and_refuses_lookup_errors() {
        let uid = available_uid(&[501], &[502], |uid| Ok(uid == 503)).unwrap();
        assert_eq!(uid, 504);
        let error = available_uid(&[], &[], |_| Err(problem("lookup failed")));
        assert!(error.is_err());
    }

    #[test]
    fn new_account_names_are_short_and_plain() {
        assert!(validate_account_name("atlas").is_ok());
        assert!(validate_account_name("qa_helper-2").is_ok());
        assert!(validate_account_name("").is_err());
        assert!(validate_account_name("has space").is_err());
        assert!(validate_account_name("-dash").is_err());
        assert!(
            validate_account_name("503").is_err(),
            "macOS reads digits alone as a user ID"
        );
        assert!(validate_account_name("503a").is_ok());
        assert!(validate_account_name(&"x".repeat(32)).is_err());
    }
}
