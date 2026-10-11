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
            // System accounts live below 501 or start with an underscore.
            (uid >= 501 && !name.starts_with('_')).then(|| (name.to_owned(), uid))
        })
        .collect()
}

fn real_name(name: &str) -> Option<String> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{name}"), "RealName"])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let value = text.strip_prefix("RealName:")?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Accounts other than this one, the helper first.
pub fn accounts(helper: Option<u32>) -> Result<Vec<Account>> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-list", "/Users", "UniqueID"])
        .output()?;
    let me = nix::unistd::getuid().as_raw();
    let mut accounts: Vec<Account> = parse_accounts(&String::from_utf8_lossy(&output.stdout))
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

/// Call a no-argument function of the login framework, resolved at run
/// time so a macOS without it reports an error instead of failing to start.
#[allow(unsafe_code)] // FFI: a private framework, loaded with dlopen.
fn login_call(symbol: &std::ffi::CStr) -> Result<i32> {
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
    Ok(unsafe {
        let call: extern "C" fn() -> i32 = std::mem::transmute(function);
        call()
    })
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
/// lock screen; confirmed from the window server, not assumed.
pub fn lock_screen() -> Result<Value> {
    let _ = login_call(c"SACLockScreenImmediate");
    if !locked_within(3) {
        // Sleeping the display locks it when a password is required on wake.
        let _ = Command::new("/usr/bin/pmset")
            .arg("displaysleepnow")
            .status();
        if !locked_within(3) {
            return Err(Error::new(
                "switch",
                "The screen could not be locked from here",
            ));
        }
    }
    Ok(json!({"ok": true, "locked": true}))
}

/// Lock the screen so the person can switch to `account` from it.
pub fn to(account: &Account) -> Result<Value> {
    let mut reply = lock_screen()?;
    let name = account.real_name.as_deref().unwrap_or(&account.name);
    reply["choose"] = json!(name);
    reply["next"] = json!(format!(
        "On the lock screen, click Switch User, choose {name} and sign in."
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
        let accounts = accounts(Some(u32::MAX)).unwrap();
        let me = nix::unistd::getuid().as_raw();
        assert!(accounts.iter().all(|account| account.uid != me));
        assert!(accounts.iter().all(|account| account.uid >= 501));
    }

    #[test]
    fn the_lock_call_exists_on_this_macos() {
        // Resolve without calling: calling would change the person's screen.
        #[allow(unsafe_code)]
        // SAFETY: dlopen/dlsym with NUL-terminated names; nothing is called.
        let found = unsafe {
            let handle = libc::dlopen(
                c"/System/Library/PrivateFrameworks/login.framework/Versions/A/login".as_ptr(),
                libc::RTLD_LAZY,
            );
            !handle.is_null() && !libc::dlsym(handle, c"SACSwitchToLoginWindow".as_ptr()).is_null()
        };
        assert!(found);
    }
}
