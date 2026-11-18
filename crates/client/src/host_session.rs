//! What Atlas records of host sessions: who left what in the helper, read
//! back for the person and for cleanup. Who a session is belongs to
//! [`atlas_core::host`].
pub mod activity;
pub mod brief;
pub mod desktops;
pub mod explain;
pub(crate) mod liveness;
pub mod names;
pub mod reading;
pub mod record;
mod recovery;
pub mod store;

pub(crate) use atlas_core::host::{Host, HostId, HostSession, Key};
pub(crate) use atlas_core::process::Instance;
pub(crate) use recovery::settled_on_its_own;
pub use recovery::{Asker, Recovery};

use crate::client::Client;
use atlas_core::{desktop::Kind, host::verification::HostClaim};
use serde_json::Value;

/// The host session of this process's command, resolved locally, its
/// ancestry proved when it needs proof, or the previous host's exit. Failure
/// preserves the local metadata, and its record is not replaced.
pub async fn from_env_verified(client: &Client) -> Option<HostSession> {
    let local = HostSession::from_env();
    match client.desktop().kind() {
        // The second login's daemon shares this Mac's kernel, so it can look
        // at this command's ancestry.
        Kind::User(login) => {
            crate::user::host_session::verified_by_user(client, login, local).await
        }
        // A VM's guest runs on another kernel, so it is never asked: this
        // command's ancestry is in this one, and this process looks at it.
        Kind::Vm(_) => {
            let claims = claims(client);
            verified_here(local, claims, |claims| {
                atlas_core::host::verification::verify(
                    &claims,
                    Instance::current(),
                    nix::unistd::getuid().as_raw(),
                )
            })
            .await
        }
    }
}

/// What this process's environment claims of its host, each with the
/// instance the desktop's records last named for it.
pub(crate) fn claims(client: &Client) -> Vec<HostClaim> {
    let mut claims = atlas_core::host::verification::from_env();
    if let Ok(output) = client.output_dir() {
        let store = store::Store::at(output);
        for claim in &mut claims {
            claim.previous = store
                .read(&claim.key())
                .and_then(|record| record.host_process);
        }
    }
    claims
}

/// [`HostSession::verify_with`] `local`, its proof of `claims` given by
/// `verify` in this process, off the runtime: the processes it reads may
/// take a while to describe.
async fn verified_here(
    local: Option<HostSession>,
    claims: Vec<HostClaim>,
    verify: impl FnOnce(Vec<HostClaim>) -> Value + Send + 'static,
) -> Option<HostSession> {
    let asked = claims.clone();
    let reply = async move {
        tokio::task::spawn_blocking(move || verify(asked))
            .await
            .map_err(|_| {
                atlas_core::error::Error::new(
                    atlas_core::error::Code::State,
                    "This command's host could not be looked at.",
                )
            })
    };
    HostSession::verify_with(local, &claims, reply).await
}

#[cfg(test)]
#[path = "host_session_tests.rs"]
mod tests;
