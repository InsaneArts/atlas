//! `atlas switch`: switching to another login, such as the helper. It
//! locks the screen; the person picks the account on the lock screen and
//! types its password there. Nothing here sees or stores a password.
//!
//! The login framework's per-account switch takes an undocumented
//! dictionary, and its login-window call does nothing from a command-line
//! process, so the screen lock (confirmed by the window server) is used.
use crate::error::{Error, Result};
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
fn parse_accounts(listing: &str) -> Vec<(String, u32)> {
    listing
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let name = columns.next()?;
            let uid = columns.next()?.parse::<u32>().ok()?;
            // System accounts live below the first user ID or start with an
            // underscore.
            (uid >= crate::setup::FIRST_USER_UID && !name.starts_with('_'))
                .then(|| (name.to_owned(), uid))
        })
        .collect()
}

/// Accounts other than this one, the helper first.
pub fn accounts(helper: Option<u32>) -> Result<Vec<Account>> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-list", "/Users", "UniqueID"])
        .output()?;

    // A failed listing is an error, never "no other accounts".
    if !output.status.success() {
        return Err(Error::new(
            "switch",
            format!(
                "macOS could not list the accounts: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    let me = nix::unistd::getuid().as_raw();
    Ok(listed(
        &String::from_utf8_lossy(&output.stdout),
        me,
        helper,
        crate::setup::real_name,
    ))
}

/// The ordinary logins in `listing` other than `me`, the helper first,
/// then by user ID.
fn listed(
    listing: &str,
    me: u32,
    helper: Option<u32>,
    real_name: impl Fn(&str) -> Option<String>,
) -> Vec<Account> {
    let mut accounts: Vec<Account> = parse_accounts(listing)
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
    accounts
}

/// The other accounts that are standard (not administrators): the ones
/// that can be the helper.
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

    let unavailable = || Error::new("switch", "This macOS cannot lock the screen from here");

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

/// Call a no-argument function of the login framework.
fn login_call(symbol: &std::ffi::CStr) -> Result<i32> {
    Ok(login_symbol(symbol)?())
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
/// lock asked for: the display is never put to sleep instead.
pub fn lock_screen() -> Result<Value> {
    login_call(c"SACLockScreenImmediate")?;

    if !locked_within(5) {
        return Err(Error::new(
            "switch",
            "The screen could not be locked from here; lock it from the Apple menu",
        ));
    }

    Ok(json!({"ok": true, "locked": true}))
}

/// Lock the screen so the person can switch to `account` from it.
pub fn to(account: &Account) -> Result<Value> {
    let mut reply = lock_screen()?;
    let name = account.real_name.as_deref().unwrap_or(&account.name);
    reply["choose"] = json!(name);
    reply["next"] = json!(format!(
        "On the lock screen, click Switch User, choose {name} and log in."
    ));

    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ordinary_logins_are_offered() {
        let listing = "_spotlight 89\nroot 0\ndaemon 1\natlastest 502\ntheiskaa 501\nnobody -2\n";
        assert_eq!(
            parse_accounts(listing),
            vec![("atlastest".into(), 502), ("theiskaa".into(), 501)]
        );
    }

    #[test]
    fn the_helper_is_listed_first_and_this_login_never() {
        let listing = "zed 504\nme 501\natlas 503\nann 502\n_hidden 600\n";
        let named = |name: &str| (name == "atlas").then(|| "Atlas".to_owned());
        let accounts = listed(listing, 501, Some(503), named);
        let order: Vec<(&str, bool)> = accounts
            .iter()
            .map(|account| (account.name.as_str(), account.helper))
            .collect();

        assert_eq!(
            order,
            [("atlas", true), ("ann", false), ("zed", false)],
            "the helper first, then by user ID, never this login"
        );
        assert_eq!(accounts[0].real_name.as_deref(), Some("Atlas"));

        let without = listed(listing, 501, None, |_| None);
        assert!(without.iter().all(|account| !account.helper));
        assert_eq!(without[0].uid, 502);
    }

    #[test]
    fn this_mac_lists_only_other_ordinary_logins() {
        let accounts = accounts(Some(u32::MAX)).unwrap();
        let me = nix::unistd::getuid().as_raw();
        assert!(accounts.iter().all(|account| account.uid != me));
        assert!(
            accounts
                .iter()
                .all(|account| account.uid >= crate::setup::FIRST_USER_UID)
        );
    }

    #[test]
    fn the_lock_calls_exist_on_this_macos() {
        // Resolved without calling: calling would change the person's screen.
        assert!(login_symbol(c"SACLockScreenImmediate").is_ok());
        assert!(login_symbol(c"SACSwitchToLoginWindow").is_ok());
        assert_eq!(login_symbol(c"SACNoSuchCall").unwrap_err().code, "switch");
    }
}
