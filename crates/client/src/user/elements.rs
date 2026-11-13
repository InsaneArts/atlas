//! The inspection tool of the user desktop: the shared Atlas command it
//! runs from this disk.
use crate::client::{Client, Run};
use atlas_core::error::Result;
use serde_json::Value;

/// The element probe in the second login `login`, which runs the shared Atlas
/// command from this disk.
pub(crate) async fn probe_in(
    client: &Client,
    _login: &atlas_core::user::User,
    run: Run<'_>,
    pid: i64,
) -> Result<Value> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let argv = vec![probe, "elements-info".into(), pid.to_string()];
    client.run(argv, run).await
}
