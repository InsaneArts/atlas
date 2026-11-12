//! What the user desktop shares with this Mac's own account: the disk, so
//! the person's home, the Applications folder and Atlas's own app.
use std::path::{Path, PathBuf};

/// Where a second login's apps are installed: this Mac's Applications
/// folder, which it shares.
const APPLICATIONS: &str = "/Applications";

/// The person's own home, for the second login `login`: it shares this
/// disk, so a path there is the shell's `~`, never the helper's file.
pub(crate) fn person_home(_login: &atlas_core::user::User) -> Option<PathBuf> {
    atlas_core::config::home().ok()
}

/// [`installed`](crate::app_selection::installed) for the second login
/// `login`: whether `/Applications/{name}.app` is a folder, as it shares
/// this Mac's.
pub(crate) fn in_applications(_login: &atlas_core::user::User, name: &str) -> bool {
    crate::app_selection::installed_in(Path::new(APPLICATIONS), name)
}

/// The app bundle this process runs from, which the second login `login`
/// runs Atlas from too.
pub(crate) fn atlas_bundle(_login: &atlas_core::user::User) -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| atlas_core::app_bundle::app_of(&exe))
        .map(|bundle| bundle.to_string_lossy().into_owned())
}
