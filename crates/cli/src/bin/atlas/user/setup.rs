//! The user desktop's setup commands: `atlas setup` and the helper-side
//! `atlas setup-helper` worker.
use crate::cli;
use atlas_client::client::Client;
use atlas_core::error::Result;
use serde_json::Value;
use std::path::Path;

pub(crate) fn setup_helper(args: &cli::SetupHelper) -> Result<Value> {
    let guard = atlas_daemon::user::setup::Guard {
        account: Some(serde_json::from_str(&args.account)?),
        logged_out: args.logged_out,
    };
    atlas_daemon::user::setup::run_guarded(serde_json::from_str(&args.request)?, &guard)
}

/// `atlas setup`: a preview, or the recorded setup applied.
pub(crate) async fn setup(
    config: &Path,
    client: &impl Fn() -> Result<Client>,
    args: cli::Setup,
) -> Result<Value> {
    if !args.apply {
        return atlas_client::user::setup::preview(config, args.helper.as_deref()).await;
    }

    let client = client()?;
    // Setup settles only mutations its own log records, and refuses others.
    let ownership = client.service_change(true).await?;
    atlas_client::user::setup::migrate::apply(config, &client, &ownership).await
}
