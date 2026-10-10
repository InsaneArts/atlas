//! `understudy switch`: fast user switching to another login, such as the
//! helper. macOS shows its own login window for that account and the person
//! types the password there; nothing here sees or stores a password.
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

/// The login framework's switch calls, resolved at run time so a macOS
/// without them reports an error instead of failing to start.
#[allow(unsafe_code)] // FFI: a private framework, loaded with dlopen.
fn switch_call(symbol: &std::ffi::CStr, argument: Option<u32>) -> Result<()> {
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
    let function = unsafe { libc::dlsym(handle, symbol.as_ptr()) };
    if function.is_null() {
        return Err(unavailable());
    }
    // SAFETY: SACSwitchToUser takes a uid and SACSwitchToLoginWindow takes
    // nothing; both return a status, zero on success.
    let status = unsafe {
        match argument {
            Some(uid) => {
                let call: extern "C" fn(libc::uid_t) -> i32 = std::mem::transmute(function);
                call(uid)
            }
            None => {
                let call: extern "C" fn() -> i32 = std::mem::transmute(function);
                call()
            }
        }
    };
    (status == 0)
        .then_some(())
        .ok_or_else(|| Error::new("switch", format!("macOS refused to switch ({status})")))
}

/// Show the login window for `account`; the person signs in there.
pub fn to(account: &Account) -> Result<Value> {
    switch_call(c"SACSwitchToUser", Some(account.uid))?;
    Ok(json!({"ok": true, "switchedTo": account.name}))
}

/// Show the login window without choosing an account.
pub fn to_login_window() -> Result<Value> {
    switch_call(c"SACSwitchToLoginWindow", None)?;
    Ok(json!({"ok": true, "switchedTo": "login window"}))
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
    fn the_switch_calls_exist_on_this_macos() {
        // Resolve without calling: calling would change the person's screen.
        #[allow(unsafe_code)]
        // SAFETY: dlopen/dlsym with NUL-terminated names; nothing is called.
        let found = unsafe {
            let handle = libc::dlopen(
                c"/System/Library/PrivateFrameworks/login.framework/Versions/A/login".as_ptr(),
                libc::RTLD_LAZY,
            );
            !handle.is_null()
                && !libc::dlsym(handle, c"SACSwitchToUser".as_ptr()).is_null()
                && !libc::dlsym(handle, c"SACSwitchToLoginWindow".as_ptr()).is_null()
        };
        assert!(found);
    }
}
