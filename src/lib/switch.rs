//! `atlas switch`: switching to another login, such as the helper. It
//! locks the screen; the person picks the account on the lock screen and
//! types its password there. Nothing here sees or stores a password.
//!
//! The login framework's per-account switch takes an undocumented
//! dictionary, and its login-window call does nothing from a command-line
//! process, so the screen lock (confirmed by the window server) is used.
use crate::{
    error::{Code, Error, Result},
    session::Blocker,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;

/// A login the person can switch to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub name: String,
    pub uid: u32,
    #[serde(rename = "realName")]
    pub real_name: Option<String>,
    /// The configured helper.
    pub helper: bool,
}

/// Parse `dscl . -list /Users UniqueID` into (name, uid) for ordinary logins.
fn parse_accounts(listing: &str) -> Result<Vec<(String, u32)>> {
    let mut accounts = Vec::new();
    for line in listing.lines() {
        let unreadable = || Error::new(Code::Switch, "Could not read an account's user ID");
        let mut columns = line.split_whitespace();
        let name = columns.next().ok_or_else(unreadable)?;
        let uid = columns
            .next()
            .and_then(|word| word.parse::<i64>().ok())
            .ok_or_else(unreadable)?;
        if uid < 0 {
            continue;
        }
        let uid = u32::try_from(uid).map_err(|_| unreadable())?;
        // System accounts live below the first user ID or start with an
        // underscore.
        if uid >= crate::setup::FIRST_USER_UID && !name.starts_with('_') {
            accounts.push((name.to_owned(), uid));
        }
    }
    Ok(accounts)
}

/// Accounts other than this one, the helper first.
///
/// # Errors
///
/// Returns account-enumeration or identity failures instead of reporting an incomplete list
/// as complete.
pub fn accounts(helper: Option<u32>) -> Result<Vec<Account>> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-list", "/Users", "UniqueID"]),
        crate::bounded::LOOKUP,
    )?;

    // A failed listing is an error, never "no other accounts".
    if !output.status.success() {
        return Err(Error::new(
            Code::Switch,
            format!(
                "macOS could not list the accounts: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    let me = nix::unistd::getuid().as_raw();
    listed(
        std::str::from_utf8(&output.stdout)
            .map_err(|_| Error::new(Code::Switch, "Could not read the accounts"))?,
        me,
        helper,
        crate::setup::real_name,
    )
}

/// The ordinary logins in `listing` other than `me`, the helper first,
/// then by user ID.
fn listed(
    listing: &str,
    me: u32,
    helper: Option<u32>,
    real_name: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Account>> {
    let mut accounts: Vec<Account> = parse_accounts(listing)?
        .into_iter()
        .filter(|(_, uid)| *uid != me)
        .map(|(name, uid)| Account {
            real_name: real_name(&name),
            helper: Some(uid) == helper,
            name,
            uid,
        })
        .collect();
    accounts.sort_by_key(|account| (!account.helper, account.uid));
    Ok(accounts)
}

/// The other accounts that are standard (not administrators): the ones
/// that can be the helper.
///
/// # Errors
///
/// Returns directory-service enumeration failures; only accounts whose standard-account
/// status is known are offered.
pub fn standard_accounts() -> Result<Vec<Account>> {
    Ok(accounts(None)?
        .into_iter()
        .filter(|account| crate::setup::administrator(&account.name) == Some(false))
        .collect())
}

/// A no-argument function of the login framework, resolved at run time so
/// a macOS without it reports an error instead of failing to start.
#[allow(unsafe_code, reason = "FFI: a private framework, loaded with dlopen")]
fn login_symbol(symbol: &std::ffi::CStr) -> Result<extern "C" fn() -> i32> {
    const FRAMEWORK: &std::ffi::CStr =
        c"/System/Library/PrivateFrameworks/login.framework/Versions/A/login";

    let unavailable = || Error::new(Code::Switch, "This macOS cannot lock the screen from here");

    // SAFETY: a NUL-terminated path; the handle stays open for the process.
    let handle = unsafe { libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_LAZY) };
    if handle.is_null() {
        return Err(unavailable());
    }

    // SAFETY: a live handle and a NUL-terminated symbol name.
    let function = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
    if function.is_null() {
        return Err(unavailable());
    }

    // SAFETY: the login calls used here take no arguments and return a
    // status (as Chromium's login_util declares them).
    Ok(unsafe { std::mem::transmute::<*mut std::ffi::c_void, extern "C" fn() -> i32>(function) })
}

/// Whether this login's screen is locked now, from the window server.
fn screen_locked() -> bool {
    crate::native::cf::session()
        .and_then(|record| crate::native::cf::boolean(&record, "CGSSessionScreenIsLocked"))
        == Some(true)
}

/// Wait up to `seconds` for the screen to report locked.
fn locked_within(seconds: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < deadline {
        if screen_locked() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    false
}

/// Lock the screen now, so the person can choose another account from the
/// lock screen; confirmed from the window server, not assumed. Only the
/// lock asked for: the display is never put to sleep instead. Every caller,
/// including switch-to and lock-only commands, must be on the console.
///
/// # Errors
///
/// Returns failures locating or invoking macOS's screen-lock operation. The caller must
/// coordinate this physical transition with the person.
pub fn lock_screen() -> Result<Value> {
    let lock = login_symbol(c"SACLockScreenImmediate")?;
    lock_screen_using(
        nix::unistd::getuid().as_raw(),
        crate::native::session_info,
        || {
            lock();
        },
        || locked_within(5),
    )
}

fn lock_screen_using(
    uid: u32,
    session: impl FnOnce() -> Result<Value>,
    lock: impl FnOnce(),
    confirmed: impl FnOnce() -> bool,
) -> Result<Value> {
    // Read this process's graphical session immediately before the call.
    // A console owner's UID alone cannot prove that this session is there.
    let proof = session()?;
    if proof["uid"].as_u64() != Some(u64::from(uid)) || proof["hasGraphicAccess"] != true {
        return Err(Error::new(
            Code::Identity,
            "Atlas could not verify this login's graphical session.",
        ));
    }
    if proof["onConsole"] != true {
        return Err(not_ready(
            "this login must be on screen at the Mac to switch accounts or lock the screen.",
        ));
    }
    lock();

    if !confirmed() {
        return Err(Error::new(
            Code::Switch,
            "The screen could not be locked from here; lock it from the Apple menu",
        ));
    }

    Ok(json!({"ok": true, "locked": true}))
}

/// What the switch needs to know of the account it switches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    /// Its user ID.
    pub uid: u32,
    /// It is the configured helper, whose account must be fit for Atlas.
    pub helper: bool,
    /// It is the person this account is the helper of: switching to it is
    /// switching back.
    pub back: bool,
    /// Its desktop already runs: its loginwindow and its Dock.
    pub logged_in: bool,
    /// Its account exists.
    pub exists: bool,
    /// Its home folder exists and belongs to it.
    pub home_owned: bool,
    /// Whether it administers this Mac; unknown when `None`.
    pub administrator: Option<bool>,
    /// It is on screen already.
    pub on_console: bool,
}

/// The refusal for a switch that must wait, after "Atlas can't switch yet:".
fn not_ready(why: &str) -> Error {
    Error::new(Code::NotReady, format!("Atlas can't switch yet: {why}"))
}

/// Whether the switch to `target` may go ahead, given what would make a
/// new login hang now (`blockers`, `None` when they could not be read).
/// Nothing is locked when it may not. Only switching back from the helper
/// to the person's own desktop, already running and an account of its own,
/// starts no login and may pass what would hold up a new one, so the person
/// is never kept in the helper; even then a deleted account's login under
/// the helper's user ID, or anything about the person's own login, stops
/// it.
pub(crate) fn ready_to_switch(target: &Target, blockers: Option<&[Blocker]>) -> Result<()> {
    let Some(blockers) = blockers else {
        return Err(not_ready("macOS didn't list its logins. Try again."));
    };
    let back = target.back && !target.helper && target.logged_in && target.exists;
    let holds = |blocker: &Blocker| {
        !back
            || matches!(blocker, Blocker::HelperLeftover { .. })
            || blocker.uid() == Some(target.uid)
    };
    if let Some(blocker) = blockers.iter().find(|blocker| holds(blocker)) {
        return Err(not_ready(blocker.words()));
    }

    if target.helper {
        if !target.exists {
            return Err(not_ready("the helper account no longer exists."));
        }
        if !target.home_owned {
            return Err(not_ready(
                "the helper's home folder is missing or isn't its own.",
            ));
        }
        if target.administrator != Some(false) {
            return Err(not_ready(
                "the helper must be a standard account, not an administrator.",
            ));
        }
        if target.on_console {
            return Err(not_ready("the helper is already on screen."));
        }
    }

    Ok(())
}

/// The person this account is the helper of: the one whose request named
/// it, else the one its own background job works for.
fn person_of_this_helper() -> Option<u32> {
    let me = nix::unistd::getuid().as_raw();
    if let Some(pairing) =
        crate::setup::pairing::request().filter(|pairing| pairing.helper_uid == me)
    {
        return Some(pairing.controller_uid);
    }

    let layout = crate::setup::helper::Layout {
        uid: me,
        home: crate::config::home().ok()?,
        guard: None,
    };
    let plist = std::fs::read_to_string(layout.agent_plist(crate::setup::helper::LABEL)).ok()?;
    controller_in(&plist)
}

/// The account a helper job's definition `plist` works for.
fn controller_in(plist: &str) -> Option<u32> {
    let strings: Vec<&str> = plist
        .split("<string>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</string>").map(|(value, _)| value))
        .collect();
    let at = strings
        .iter()
        .position(|value| *value == "--controller-uid")?;

    strings.get(at + 1)?.parse().ok()
}

/// [`Target`] for `account` as this Mac shows it now.
fn target(account: &Account) -> Target {
    use std::os::unix::fs::MetadataExt;

    let found = crate::setup::account_with_uid(account.uid).ok().flatten();
    let home_owned = found.as_ref().is_some_and(|found| {
        std::fs::symlink_metadata(&found.home)
            .is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == account.uid)
    });

    Target {
        uid: account.uid,
        helper: account.helper,
        back: !account.helper && person_of_this_helper() == Some(account.uid),
        logged_in: crate::session::desktop_runs(account.uid) == Some(true),
        exists: found.is_some(),
        home_owned,
        administrator: crate::setup::administrator(&account.name),
        on_console: crate::session::console_owner() == Some(account.uid),
    }
}

/// Check that the switch to `account` may go ahead now, as
/// [`ready_to_switch`] decides, and note the check in the login timing.
///
/// # Errors
///
/// Rejects a changed, missing or unsuitable account before switching can be requested.
pub fn check(account: &Account) -> Result<()> {
    let helper = account.helper.then_some(account.uid);
    let blockers = crate::session::blockers_now(helper);
    crate::login_log::mark(&format!(
        "switch.check blockers={}",
        blockers.as_ref().map_or(-1, |found| found.len() as i64)
    ));

    ready_to_switch(&target(account), blockers.as_deref())
}

/// Lock the screen so the person can switch to `account` from it, once
/// [`check`] lets the switch go ahead.
///
/// # Errors
///
/// Returns account-validation or macOS switch-request failures. The caller must coordinate
/// the physical switch with the person.
pub fn to(account: &Account) -> Result<Value> {
    check(account)?;
    let mut reply = lock_screen()?;
    if account.helper {
        crate::login_log::mark(&format!("switch.locked helper={}", account.uid));
    }
    let name = account.real_name.as_deref().unwrap_or(&account.name);
    reply["choose"] = json!(name);
    reply["next"] = json!(format!(
        "On the lock screen, click Switch User, choose {name} and log in."
    ));

    Ok(reply)
}

#[cfg(test)]
mod tests;
