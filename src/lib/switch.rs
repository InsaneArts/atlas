//! `atlas switch`: switching to another login, such as the helper. It
//! locks the screen; the person picks the account on the lock screen and
//! types its password there. Nothing here sees or stores a password.
//!
//! The login framework's per-account switch takes an undocumented
//! dictionary, and its login-window call does nothing from a command-line
//! process, so the screen lock (confirmed by the window server) is used.
use crate::{
    error::{Error, Result},
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
        let unreadable = || Error::new("switch", "Could not read an account's user ID");
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
pub fn accounts(helper: Option<u32>) -> Result<Vec<Account>> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-list", "/Users", "UniqueID"]),
        crate::bounded::LOOKUP,
    )?;

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
    listed(
        std::str::from_utf8(&output.stdout)
            .map_err(|_| Error::new("switch", "Could not read the accounts"))?,
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

/// What the switch needs to know of the account it switches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
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
    Error::new("not-ready", format!("Atlas can't switch yet: {why}"))
}

/// Whether the switch to `target` may go ahead, given what would make a
/// new login hang now (`blockers`, `None` when they could not be read).
/// Nothing is locked when it may not. Only switching back from the helper
/// to the person's own desktop, already running and an account of its own,
/// starts no login and may pass what would hold up a new one, so the person
/// is never kept in the helper; even then a deleted account's login under
/// the helper's user ID, or anything about the person's own login, stops
/// it.
pub fn ready_to_switch(target: &Target, blockers: Option<&[Blocker]>) -> Result<()> {
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
    if let Some(pairing) = crate::onboarding::request().filter(|pairing| pairing.helper_uid == me) {
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
mod tests {
    use super::*;

    #[test]
    fn only_ordinary_logins_are_offered() {
        let listing = "_spotlight 89\nroot 0\ndaemon 1\n_unknown 99\natlastest 502\ntheiskaa 501\nnobody -2\n";
        assert_eq!(
            parse_accounts(listing).unwrap(),
            vec![("atlastest".into(), 502), ("theiskaa".into(), 501)]
        );
    }

    #[test]
    fn account_lists_refuse_garbage_instead_of_omitting_an_account() {
        for line in ["garbage", "broken NaN", "large 4294967296"] {
            let listing = format!("nobody -2\ndaemon 1\n_unknown 99\n{line}\natlas 502\n");
            assert!(
                listed(&listing, 501, Some(502), |_| None).is_err(),
                "{line}"
            );
        }
    }

    #[test]
    fn the_helper_is_listed_first_and_this_login_never() {
        let listing = "zed 504\nme 501\natlas 503\nann 502\n_hidden 600\n";
        let named = |name: &str| (name == "atlas").then(|| "Atlas".to_owned());
        let accounts = listed(listing, 501, Some(503), named).unwrap();
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

        let without = listed(listing, 501, None, |_| None).unwrap();
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

    fn helper() -> Target {
        Target {
            uid: 503,
            helper: true,
            back: false,
            logged_in: false,
            exists: true,
            home_owned: true,
            administrator: Some(false),
            on_console: false,
        }
    }

    /// The person's own desktop, switched back to from the helper.
    fn back() -> Target {
        Target {
            uid: 501,
            helper: false,
            back: true,
            logged_in: true,
            administrator: Some(true),
            ..helper()
        }
    }

    fn refusal(target: &Target, blockers: Option<&[Blocker]>) -> String {
        let error = ready_to_switch(target, blockers).unwrap_err();
        assert_eq!(error.code, "not-ready");
        error.error
    }

    #[test]
    fn a_new_login_waits_while_another_could_hang_it() {
        assert!(ready_to_switch(&helper(), Some(&[])).is_ok());

        let cases = [
            (
                Blocker::Orphan { uid: 502 },
                "Atlas can't switch yet: a deleted account is still logged in.",
            ),
            (
                Blocker::HelperLeftover { uid: 503 },
                "Atlas can't switch yet: a deleted account is still logged in.",
            ),
            (
                Blocker::StuckLogin { uid: 504 },
                "Atlas can't switch yet: another account is still logging in.",
            ),
            (
                Blocker::AccountUpdating,
                "Atlas can't switch yet: macOS is still setting up an account.",
            ),
        ];
        for (blocker, words) in cases {
            assert_eq!(refusal(&helper(), Some(&[blocker])), words);
        }

        assert_eq!(
            refusal(&helper(), None),
            "Atlas can't switch yet: macOS didn't list its logins. Try again."
        );
    }

    #[test]
    fn switching_back_to_the_persons_own_running_desktop_passes_what_holds_a_new_login() {
        for blocker in [
            Blocker::Orphan { uid: 502 },
            Blocker::StuckLogin { uid: 504 },
            Blocker::AccountUpdating,
        ] {
            assert!(
                ready_to_switch(&back(), Some(&[blocker])).is_ok(),
                "{blocker:?}"
            );
        }
    }

    #[test]
    fn a_helpers_running_dock_never_lets_a_switch_pass_a_leftover() {
        let leftover = [Blocker::HelperLeftover { uid: 503 }];
        let running = Target {
            logged_in: true,
            ..helper()
        };
        assert_eq!(
            refusal(&running, Some(&leftover)),
            "Atlas can't switch yet: a deleted account is still logged in."
        );
        assert_eq!(
            refusal(&running, Some(&[Blocker::Orphan { uid: 502 }])),
            "Atlas can't switch yet: a deleted account is still logged in.",
            "the helper is never switched back to"
        );
        assert_eq!(
            refusal(&back(), Some(&leftover)),
            "Atlas can't switch yet: a deleted account is still logged in.",
            "not even when switching back"
        );
    }

    #[test]
    fn only_the_persons_own_verified_desktop_counts_as_switching_back() {
        let orphan = [Blocker::Orphan { uid: 502 }];
        let cases = [
            Target {
                back: false,
                ..back()
            },
            Target {
                logged_in: false,
                ..back()
            },
            Target {
                exists: false,
                ..back()
            },
            Target {
                helper: true,
                ..back()
            },
        ];
        for target in cases {
            assert!(
                ready_to_switch(&target, Some(&orphan)).is_err(),
                "{target:?}"
            );
        }

        // Its own login stuck, or unknown logins: it waits too.
        assert!(ready_to_switch(&back(), Some(&[Blocker::StuckLogin { uid: 501 }])).is_err());
        assert!(ready_to_switch(&back(), None).is_err());
    }

    #[test]
    fn a_helper_job_names_the_person_it_works_for() {
        use crate::setup::helper::{Agent, plist};

        let agent = Agent {
            label: "com.insanearts.atlas.helper".into(),
            program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
            uid: 503,
            controller_uid: 501,
            socket: "/Users/opie/s.sock".into(),
        };
        assert_eq!(
            controller_in(&plist(&agent, std::path::Path::new("/tmp"))),
            Some(501)
        );
        assert_eq!(controller_in("<plist></plist>"), None);
    }

    #[test]
    fn the_helper_must_be_a_standard_account_with_its_own_home_off_screen() {
        let cases = [
            (
                Target {
                    exists: false,
                    ..helper()
                },
                "Atlas can't switch yet: the helper account no longer exists.",
            ),
            (
                Target {
                    home_owned: false,
                    ..helper()
                },
                "Atlas can't switch yet: the helper's home folder is missing or isn't its own.",
            ),
            (
                Target {
                    administrator: None,
                    ..helper()
                },
                "Atlas can't switch yet: the helper must be a standard account, not an administrator.",
            ),
            (
                Target {
                    on_console: true,
                    ..helper()
                },
                "Atlas can't switch yet: the helper is already on screen.",
            ),
        ];
        for (target, words) in cases {
            assert_eq!(refusal(&target, Some(&[])), words);
        }
    }

    #[test]
    fn the_lock_calls_exist_on_this_macos() {
        // Resolved without calling: calling would change the person's screen.
        assert!(login_symbol(c"SACLockScreenImmediate").is_ok());
        assert!(login_symbol(c"SACSwitchToLoginWindow").is_ok());
        assert_eq!(login_symbol(c"SACNoSuchCall").unwrap_err().code, "switch");
    }
}
