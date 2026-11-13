//! What Atlas records of host sessions: who left what in the helper, read
//! back for the person and for cleanup. Who a session is belongs to
//! [`atlas_core::host`].
pub mod activity;
pub mod brief;
pub mod explain;
pub(crate) mod liveness;
pub mod names;
pub mod reading;
pub mod record;
mod recovery;
pub mod store;

pub(crate) use atlas_core::host::{Host, HostId, HostSession, Key};
pub(crate) use atlas_core::process::Instance;
pub use recovery::{Asker, Recovery};

use crate::client::Client;
use atlas_core::desktop::Kind;

/// The host session of this process's command, resolved locally, asking the
/// daemon when ancestry or the previous host's exit needs proof. Failure
/// preserves the local metadata.
pub async fn from_env_verified(client: &Client) -> Option<HostSession> {
    let local = HostSession::from_env();
    match client.desktop().kind() {
        // The second login's daemon shares this Mac's kernel, so it can look
        // at this command's ancestry.
        Kind::User(login) => {
            crate::user::host_session::verified_by_user(client, login, local).await
        }
    }
}

#[cfg(test)]
#[path = "host_session_tests.rs"]
mod tests;
