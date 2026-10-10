//! `understudy switch`: fast user switching to another login, such as the
//! helper. macOS shows its own login window, the person picks the account
//! and types its password there; nothing here sees or stores a password.
//!
//! Only the login window is opened: the framework's per-account call takes
//! an undocumented dictionary, not a UID, and is never guessed at.
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

/// Show the macOS login window (fast user switching), resolved at run time
/// so a macOS without it reports an error instead of failing to start.
#[allow(unsafe_code)] // FFI: a private framework, loaded with dlopen.
pub fn to_login_window() -> Result<Value> {
    const FRAMEWORK: &std::ffi::CStr =
        c"/System/Library/PrivateFrameworks/login.framework/Versions/A/login";
    let unavailable = || {
        Error::new(
            "switch",
            "This macOS does not offer fast user switching here",
        )
    };
    // SAFETY: a NUL-terminated path; the handle stays open for the process.
    let handle = unsafe { libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_LAZY) };
    if handle.is_null() {
        return Err(unavailable());
    }
    // SAFETY: a live handle and a NUL-terminated symbol name.
    let function = unsafe { libc::dlsym(handle, c"SACSwitchToLoginWindow".as_ptr()) };
    if function.is_null() {
        return Err(unavailable());
    }
    // SAFETY: SACSwitchToLoginWindow takes no arguments and returns a
    // status, zero on success (as used by Chromium's login_util).
    let status = unsafe {
        let call: extern "C" fn() -> i32 = std::mem::transmute(function);
        call()
    };
    if status != 0 {
        return Err(Error::new(
            "switch",
            format!("macOS refused to switch ({status})"),
        ));
    }
    Ok(json!({"ok": true, "switchedTo": "login window"}))
}

/// Open the login window for switching to `account`; the person picks it
/// there (it is listed by name).
pub fn to(account: &Account) -> Result<Value> {
    let mut reply = to_login_window()?;
    reply["choose"] = json!(account.real_name.as_deref().unwrap_or(&account.name));
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
    fn the_login_window_call_exists_on_this_macos() {
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
