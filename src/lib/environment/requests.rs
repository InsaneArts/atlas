//! Installing an app into the helper for a task, removing one, and asking
//! how installs stand, sent to the environment worker through a request
//! folder of their own. A change runs under the control its caller holds;
//! its folder stays while its outcome is not known, as every change's does.
use super::{model::*, *};
use crate::control::Token;

/// Install the bundle at `source`, which the helper can read, as
/// `~/Applications/NAME` for install `id`, under `ownership`.
pub(crate) async fn install_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
    source: &Path,
    name: &str,
) -> Result<Value> {
    let request = Request::InstallApp {
        id: id.as_str().to_owned(),
        source: source.to_owned(),
        name: name.to_owned(),
    };
    send(client, Some(ownership), request).await
}

/// Remove install `id`, when it is unchanged and nothing runs from it,
/// under `ownership`.
pub(crate) async fn remove_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
) -> Result<Value> {
    send(
        client,
        Some(ownership),
        Request::RemoveApp {
            id: id.as_str().to_owned(),
        },
    )
    .await
}

/// How each of `ids` stands, read without control.
pub(crate) async fn installs(client: &Client, ids: &[Token]) -> Result<Value> {
    if ids.is_empty() {
        return Ok(json!({"ok": true, "installs": []}));
    }
    let ids = ids.iter().map(|id| id.as_str().to_owned()).collect();
    let answer = send(client, None, Request::Installs { ids }).await?;
    if answer["ok"] == false {
        return Err(crate::transport::refusal(&answer));
    }
    if answer["ok"] != true || !answer["installs"].is_array() {
        return Err(Error::new(
            "protocol",
            "The helper's word on its installs was malformed",
        ));
    }
    Ok(answer)
}

/// Send `request` through a request folder of its own, under `held` when
/// it changes the helper. The folder goes once the outcome is known. A
/// change may copy a large app, so it is given as long as any command; a
/// read, a command's usual time.
async fn send(client: &Client, held: Option<&Ownership>, request: Request) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    let id = crate::control::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    create_stage(&stage, &helper)?;

    let timeout = if request.changes_helper() {
        crate::client::RUN_TIMEOUT_MAX
    } else {
        crate::client::RUN_TIMEOUT
    };
    let result = dispatch_as(client, held, &helper, &stage, request, timeout).await;
    if !delivery_uncertain(&result) && !requests(&stage) {
        discard(&stage);
    }
    result
}
