//! The account macOS names as the one at the screen: its console user.
//!
//! The login window program publishes the name for the last login to log in
//! or to come onto the screen, and macOS's own agents read it to tell
//! whether their login is the one in use. It is not the window server's
//! record of which login is on the screen: a login made off the screen is
//! named as well.
use objc2_system_configuration::SCDynamicStore;

/// A user ID no account has: left in place when macOS names an account
/// without saying its ID.
const NO_UID: libc::uid_t = libc::uid_t::MAX;

/// The account macOS names as its console user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleUser {
    /// Its short name; `loginwindow` while the login window has the screen.
    pub name: String,
    /// Its user ID.
    pub uid: u32,
}

/// The account macOS names as its console user now, or `None` when it names
/// none, leaves out its user ID, or may not be asked, as from a sandbox.
/// macOS sets no limit on its answer: a caller that must not wait bounds
/// the call itself.
#[allow(
    unsafe_code,
    reason = "FFI: the console user's ID is returned through an out-pointer"
)]
pub fn named() -> Option<ConsoleUser> {
    let mut uid = NO_UID;
    // SAFETY: `uid` is valid for the one write; no store asks for a
    // temporary session, and a null group ID is not written.
    let name = unsafe { SCDynamicStore::console_user(None, &mut uid, std::ptr::null_mut()) }?;
    account(name.to_string(), uid)
}

/// The account of `name` and `uid` as macOS gave them, or `None` when it
/// left the user ID out.
fn account(name: String, uid: libc::uid_t) -> Option<ConsoleUser> {
    (uid != NO_UID).then_some(ConsoleUser { name, uid })
}

#[cfg(test)]
#[path = "console_user_tests.rs"]
mod tests;
