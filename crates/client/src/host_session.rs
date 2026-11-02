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
use serde_json::json;

/// The host session of this process's command, resolved locally, asking the
/// daemon when ancestry or the previous host's exit needs proof. Failure
/// preserves the local metadata.
pub async fn from_env_verified(client: &Client) -> Option<HostSession> {
    let local = HostSession::from_env();
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
mod tests;
