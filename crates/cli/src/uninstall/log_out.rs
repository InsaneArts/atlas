//! Logging the helper out for uninstall, gentlest way first, until its login
//! is known to be gone.
use super::{Progress, Question};
use crate::onboarding::as_root::{self, Target, Via};
use atlas_client::client::Client;
use atlas_control::Ownership;
use atlas_core::error::{Code, Error, Result};
use atlas_daemon::helper_setup::identity::{Identity, Standing};

/// One way of logging the helper out, gentlest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Way {
    /// Through the helper's Atlas, as Log Out does: an app may hold it up.
    Ask,
    /// Through the helper's Atlas, ending the login at once: the helper
    /// ends its own login, with no administrator rights.
    Force,
    /// As root, under the sudo uninstall asks for: launchd ends the login.
    Root,
}

/// What came of one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Came {
    /// The helper's login is gone.
    Gone,
    /// An app in the helper holds the log-out up.
    Held,
    /// The helper is still logged in: its Atlas did not answer, refused,
    /// or could not end it.
    Stayed,
}

/// What follows a way that came to something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Next {
    /// The helper is logged out.
    Done,
    /// Ask the person whether to end the login past the app holding it up.
    AskToForce,
    /// Try another way.
    Try(Way),
    /// Every way failed: stop before removing anything.
    Stop,
}

/// What to do after `way` came to `came`. Root acts only once the helper's
/// own Atlas could not log it out; forcing past an app needs `yes` or the
/// person's word.
pub(super) fn after(way: Way, came: Came, yes: bool) -> Next {
    match (way, came) {
        (Way::Ask | Way::Force | Way::Root, Came::Gone) => Next::Done,
        (Way::Ask, Came::Held) if yes => Next::Try(Way::Force),
        (Way::Ask, Came::Held) => Next::AskToForce,
        (Way::Ask, Came::Stayed) | (Way::Force, Came::Held | Came::Stayed) => Next::Try(Way::Root),
        (Way::Root, Came::Held | Came::Stayed) => Next::Stop,
    }
}

/// Log the helper out under `ownership`, gentlest way first, and see its
/// login gone; or stop before anything is removed. Root closes the login
/// it is named by (`target`), and is never asked without one. Every way is
/// followed by the strict look, whatever it said. A log-out whose outcome
/// was left unknown is settled once the login is seen gone.
pub(super) async fn log_out_helper(
    client: &Client,
    ownership: &Ownership,
    yes: bool,
    target: Option<Target>,
    ask: &dyn Fn(Question) -> Result<bool>,
    progress: &dyn Fn(Progress<'_>),
) -> Result<()> {
    let uid = client.config.helper_uid;
    let before = client.controller.pending()?;
    let mut way = Way::Ask;

    progress(Progress::Say("Logging out the helper…"));
    loop {
        let came = match way {
            Way::Ask | Way::Force => match client.logout_held(ownership, way == Way::Force).await {
                Ok(reply) if reply["loggedOut"] == true => Came::Gone,
                Err(error) if error.code == Code::LogoutHeld => Came::Held,
                _ => Came::Stayed,
            },
            Way::Root => {
                progress(Progress::Say(
                    "The helper's Atlas could not log it out; macOS may ask for your password.",
                ));
                root_log_out(client, ownership, target.clone(), progress)
            }
        };
        let came = strictly(came, logged_out_twice(uid));

        match after(way, came, yes) {
            Next::Done => break,
            Next::AskToForce => {
                if !ask(Question::ForceLogout)? {
                    return Err(still_logged_in());
                }
                way = Way::Force;
            }
            Next::Try(next) => way = next,
            Next::Stop => return Err(still_logged_in()),
        }
    }

    // Its outcome is known now: the login is gone.
    for generation in client.controller.pending()? {
        if !before.contains(&generation) {
            client.controller.clear_if(&generation)?;
        }
    }

    progress(Progress::Done("Helper", "logged out"));
    Ok(())
}

/// The helper whose job is retired, from what is known of the account with
/// its user ID (`known`, `None` when it was gone): only the helper as
/// recorded. Another account's is never touched, only Atlas's own records
/// of the helper go; one never recorded is not taken for the helper, and
/// its account and job stay while Atlas itself can be removed.
pub(super) fn retiring(known: Option<&Standing>) -> Option<&Identity> {
    match known {
        Some(Standing::Same(identity)) => Some(identity),
        Some(Standing::Replaced | Standing::Unknown | Standing::Unrecorded(_)) | None => None,
    }
}

pub(super) fn unrecorded_message(name: &str) -> String {
    format!(
        "The account {name} and its background job are left alone because Atlas has no recorded \
         identity for it. To remove the account and its files, log it out yourself, then delete \
         {name} in System Settings > Users & Groups."
    )
}

pub(super) fn still_logged_in() -> Error {
    Error::new(
        Code::Logout,
        "The helper is still logged in, so Atlas stopped before removing anything. Log it out \
         (atlas logout), then run atlas uninstall again.",
    )
}

/// Whether the helper's user ID `uid` has no loginwindow and no Dock, seen
/// twice half a second apart.
pub(super) fn logged_out_twice(uid: u32) -> bool {
    (0..2).all(|_| {
        std::thread::sleep(std::time::Duration::from_millis(500));
        atlas_core::session::logged_out(uid) == Some(true)
    })
}

/// What a way came to, once the helper's user ID was looked at strictly
/// (`logged_out`): only that look says it is gone.
pub(super) fn strictly(came: Came, logged_out: bool) -> Came {
    match came {
        Came::Gone | Came::Held | Came::Stayed if logged_out => Came::Gone,
        Came::Gone | Came::Stayed => Came::Stayed,
        Came::Held => Came::Held,
    }
}

/// End the helper's login as root, through sudo, under the control held
/// (`ownership`) and a pending claim kept until it is seen gone: checked
/// here first as not on screen and not this account, then again by root
/// itself. A helper account that still exists, as recorded (`target`), is
/// only booted out by launchd; one already deleted is closed as any deleted
/// account's login. Without a target root is not asked.
fn root_log_out(
    client: &Client,
    ownership: &Ownership,
    target: Option<Target>,
    progress: &dyn Fn(Progress<'_>),
) -> Came {
    let uid = client.config.helper_uid;
    let me = nix::unistd::getuid().as_raw();
    let Some(target) = target else {
        progress(Progress::Say(
            "Atlas can't tell that the account with the helper's user ID is still its helper.",
        ));
        return Came::Stayed;
    };
    if uid == me || atlas_core::session::console_owner() == Some(uid) {
        return Came::Stayed;
    }

    let ended = crate::onboarding::end_as_root(
        &client.controller,
        ownership,
        || as_root::close(&[target], Via::Sudo { ask: true }),
        || logged_out_twice(uid),
    );

    match ended {
        Ok(()) => Came::Gone,
        Err(_) => Came::Stayed,
    }
}
