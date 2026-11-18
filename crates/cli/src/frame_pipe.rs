//! The shared frame pipe's broker side: its private endpoint, and the
//! service that owns capture and authority for one controller and helper.
pub(crate) mod auth;
pub(crate) mod broker;
pub(crate) mod client;
pub(crate) mod endpoint;
pub(crate) mod service;
pub(crate) mod subscription;

use atlas_client::client::Client;
use atlas_core::error::{Code, Error};

/// The refusal of a live window or frame service of the VM `client` acts
/// on, in one plain wording for both: a VM's live picture is watched only
/// in the terminal.
pub(crate) fn live_refused(client: &Client) -> Error {
    let name = &client.selected.name;
    Error::new(
        Code::Platform,
        format!(
            "{name}'s live picture shows only in the terminal, read-only: atlas monitor \
             --terminal --desktop {name}."
        ),
    )
    .with(
        "next",
        serde_json::json!(format!("Run atlas monitor --terminal --desktop {name}.")),
    )
}
