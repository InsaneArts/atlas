//! Whether apps in the helper are heard on the person's speakers. The
//! helper keeps the person's choice itself, silent unless told otherwise;
//! this asks it, and changes it, through the verified helper.
use crate::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    protocol::SOUND,
};
use serde_json::{Value, json};

/// What an older helper daemon is told it cannot do.
const OLDER: &str = "The helper is running an older Atlas that cannot mute its apps; run atlas \
                     restart, then try again";

/// Whether apps in the helper can be heard now: `helperSound` is `muted`,
/// `on` or `unavailable`, with the `reason` and its fix when unavailable,
/// and `wanted` is the choice the helper keeps. It only reads what the
/// helper last reported.
///
/// # Errors
///
/// Returns the failure to verify or reach the helper, or its refusal.
pub async fn show(client: &Client) -> Result<Value> {
    client.verify_offering(SOUND, OLDER).await?;
    ask(client, &json!({"op": "sound"})).await
}

/// Keep the helper's apps silent (`muted`) or let them play. The helper
/// keeps the choice.
///
/// # Errors
///
/// Returns the failure to verify or reach the helper, or its refusal. A
/// lost reply, or a helper that did not confirm the change in time, leaves
/// it uncertain; `atlas sound` reads it again.
pub async fn set(client: &Client, muted: bool) -> Result<Value> {
    client.verify_offering(SOUND, OLDER).await?;
    ask(client, &json!({"op": "sound", "muted": muted})).await
}

async fn ask(client: &Client, request: &Value) -> Result<Value> {
    let reply = client.transport.call(request).await?;
    if reply["ok"] != true {
        return Err(atlas_core::transport::refusal(&reply));
    }
    answer(&reply)
}

/// The helper's answer, only when it says what its sound is and what the
/// person chose.
fn answer(reply: &Value) -> Result<Value> {
    match (reply["helperSound"].as_str(), reply["wanted"].as_bool()) {
        (Some("muted" | "on" | "unavailable"), Some(wanted)) => Ok(json!({
            "ok": true,
            "helperSound": reply["helperSound"],
            "reason": reply["reason"].as_str(),
            "wanted": wanted,
        })),
        _ => Err(Error::new(
            Code::Protocol,
            "The helper did not say whether its apps are muted",
        )),
    }
}

#[cfg(test)]
#[path = "sound_tests.rs"]
mod tests;
