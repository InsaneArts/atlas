//! Proving an agent host to the user desktop, whose daemon shares this
//! Mac's kernel and so can look at a command's ancestry.
use crate::{
    client::Client,
    host_session::{HostSession, store},
};
use serde_json::json;

/// [`from_env_verified`](crate::host_session::from_env_verified), asking the second login's daemon for the proof
/// `local` lacks.
pub(crate) async fn verified_by_user(
    client: &Client,
    _login: &atlas_core::user::User,
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
