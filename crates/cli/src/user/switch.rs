//! Showing the person the user desktop from the MCP server: this Mac's
//! login window, with the helper's account to choose there.
use crate::mcp::presentation::tool_result;
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};

/// Lock this Mac to its login window, for the person to choose the
/// second login `login` there.
pub(crate) async fn switch_to_user(login: atlas_core::user::User) -> Result<Value> {
    tokio::task::spawn_blocking(move || {
        let account = atlas_client::user::switch::accounts(Some(login.uid))?
            .into_iter()
            .find(|account| account.helper)
            .ok_or_else(|| Error::new(Code::Switch, "The helper's account wasn't found"))?;
        atlas_client::user::switch::to(&account)
    })
    .await
    .map_err(|error| Error::new(Code::Switch, "The screen didn't lock").with_source(error))??;

    Ok(tool_result(
        "The screen is locked. Choose the helper there.",
        json!({"switched": true}),
    ))
}
