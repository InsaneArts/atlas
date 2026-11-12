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
use serde_json::json;

/// The host session of this process's command, resolved locally, asking the
/// daemon when ancestry or the previous host's exit needs proof. Failure
/// preserves the local metadata.
pub async fn from_env_verified(client: &Client) -> Option<HostSession> {
    let local = HostSession::from_env();
    match client.desktop().kind() {
        // The second login's daemon shares this Mac's kernel, so it can look
        // at this command's ancestry.
        Kind::SecondLogin(_) => verified_by_second_login(client, local).await,
    }
}

/// [`from_env_verified`], asking the second login's daemon for the proof
/// `local` lacks.
async fn verified_by_second_login(
    client: &Client,
    local: Option<HostSession>,
) -> Option<HostSession> {
    let mut claims = atlas_core::host::verification::from_env();
    if let Ok(output) = client.output_dir() {
        let store = store::Store::at(output);
        for claim in &mut claims {
            claim.previous = store
                .read(&claim.key())
                .and_then(|record| record.host_process);
        }
    }
    let request = json!({"op": "host-session", "claims": claims});
    HostSession::verify_with(local, &claims, client.transport.call(&request)).await
}

#[cfg(test)]
#[path = "host_session_tests.rs"]
mod tests;
