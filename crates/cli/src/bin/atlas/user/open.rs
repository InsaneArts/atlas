//! Opening Atlas as an app for the user desktop: the helper's one-time
//! opener, and whether its setup is done.
use std::process::ExitCode;

/// When launchd started this as the helper's one-time opener, open the
/// window once the helper's desktop is up: how that went. Its output goes
/// nowhere; the next login tries again.
pub(crate) fn first_login_opener() -> Option<ExitCode> {
    if !atlas_daemon::user::setup::first_login::started_as_opener() {
        return None;
    }
    Some(
        match atlas_daemon::user::setup::first_login::open_at_login() {
            Ok(_) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
    )
}

/// Whether this account is the helper another account chose, by the
/// request that names it.
pub(crate) fn chosen_as_helper() -> bool {
    let me = nix::unistd::getuid().as_raw();
    atlas_daemon::user::setup::pairing::request().is_some_and(|pairing| pairing.helper_uid == me)
}

/// Whether the second login `login` is set up: its setup settled, and its
/// account still there.
pub(crate) fn set_up_in(login: &atlas_core::user::User) -> bool {
    atlas_client::user::setup::migrate::settled(login.uid)
        && atlas_core::account::account_with_uid(login.uid).is_ok_and(|account| account.is_some())
}
