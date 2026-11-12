//! An update's steps in the user desktop: checking a migration before
//! the app is replaced, and moving the helper onto the new build.
use crate::update::has_helper_job;
use atlas_core::error::Result;

/// The update's check before the app is replaced, for the second login
/// `login`: a setup still being moved onto a new build checks the staged
/// app first.
pub(crate) async fn preflight_in(
    config: &std::path::Path,
    program: &std::path::Path,
    login: &atlas_core::user::User,
) -> Result<()> {
    if atlas_client::user::setup::migrate::in_effect(login.uid) {
        atlas_client::user::setup::migrate::preflight_apply(config, program).await?;
    }
    Ok(())
}

/// The `atlas` steps that move the second login `login` onto this
/// executable, when it has a helper job (`daemon`: its daemon answers);
/// none when there is nothing to move.
pub(crate) fn helper_steps_in(
    login: &atlas_core::user::User,
    daemon: bool,
) -> Option<Vec<&'static [&'static str]>> {
    if !has_helper_job(
        atlas_client::user::setup::migrate::recorded(login.uid),
        daemon,
    ) {
        return None;
    }
    // The daemon restarts first: a daemon whose app was swapped under it
    // can no longer vouch (to TCC) for a new input service it starts,
    // while the input service it already runs is cleaned up by the
    // restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];

    if atlas_client::user::setup::migrate::in_effect(login.uid) {
        steps.push(&["setup", "--apply"]);
    }

    Some(steps)
}
