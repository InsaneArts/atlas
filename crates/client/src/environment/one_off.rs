//! Requests sent to the environment worker through a request folder of
//! their own, beside the shared command, that goes once the outcome is
//! known: the helper's list of its copies, and installing an app, removing
//! one or asking how installs stand. A change runs under the control its
//! caller holds; its folder stays while its outcome is not known, as every
//! change's does.
use super::{
    installs::ASKED,
    model::*,
    reply::{InstallState, Installed, InstalledApp, WorkerReply},
    *,
};
use atlas_core::files::Token;

/// The most of an install's output read for the app it placed: an install's
/// answer is a few hundred bytes.
const INSTALLED_OUTPUT: usize = 64 * 1024;

/// Send `request` to the helper account `helper` through a request folder
/// of its own, under `held` when it changes the helper. A change may copy
/// a large app, so it is given as long as any command; a read, a command's
/// usual time.
pub(super) async fn send(
    client: &Client,
    helper: &str,
    held: Option<&Ownership>,
    request: Request,
) -> Result<Value> {
    let stage = request_stage(client)?;
    create_stage(&stage, helper)?;

    let timeout = if request.changes_helper() {
        crate::client::RUN_TIMEOUT_MAX
    } else {
        crate::client::RUN_TIMEOUT
    };
    let result = dispatch_as(client, held, helper, &stage, request, timeout).await;
    if !delivery_uncertain(&result) && !requests(&stage) {
        discard(&stage);
    }
    result
}

/// `answer`, a read's, when the helper accepted it and it holds the array
/// `field`; a refusal keeps its code, and anything else is said as
/// `malformed`, never read as an empty answer.
pub(super) fn read(answer: Value, field: &str, malformed: &str) -> Result<Value> {
    if answer["ok"] == false {
        return Err(atlas_core::transport::refusal(&answer));
    }
    if answer["ok"] != true || !answer[field].is_array() {
        return Err(Error::new(Code::Protocol, malformed.to_owned()));
    }
    Ok(answer)
}

/// Install the bundle at `source`, which the helper can read, as
/// `~/Applications/NAME` for install `id`, under `ownership`.
pub(crate) async fn install_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
    source: &Path,
    name: &str,
) -> Result<Value> {
    // The account the second login's staged files are given to, by the
    // name its ACLs use.
    let helper = match client.desktop().kind() {
        Kind::SecondLogin(login) => helper_name(login.uid)?,
    };
    let request = Request::InstallApp {
        id: id.as_str().to_owned(),
        source: source.to_owned(),
        name: name.to_owned(),
    };
    send(client, &helper, Some(ownership), request).await
}

/// Whether the install that answered `reply` may have put its app in place:
/// it finished, or stopped part way and must be resumed.
pub(crate) fn placed_app(reply: &Value) -> bool {
    let said = WorkerReply::read(reply);
    said.ok == Some(true) || said.resume_required == Some(true)
}

/// The app the install whose run answered `reply` placed, as its worker's
/// answer names it; none for any other run, and for an install that did
/// not place one.
pub(crate) fn installed_app(reply: &Value) -> Option<InstalledApp> {
    let output = run_output(reply, INSTALLED_OUTPUT).ok()?;
    let installed: Installed = serde_json::from_slice(&output).ok()?;
    (installed.ok && installed.installed).then_some(InstalledApp {
        app: installed.app,
        bundle_id: installed.bundle_id,
    })
}

/// Remove install `id`, when it is unchanged and nothing runs from it,
/// under `ownership`.
pub(crate) async fn remove_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
) -> Result<Value> {
    // The account the second login's staged files are given to, by the
    // name its ACLs use.
    let helper = match client.desktop().kind() {
        Kind::SecondLogin(login) => helper_name(login.uid)?,
    };
    let request = Request::RemoveApp {
        id: id.as_str().to_owned(),
    };
    send(client, &helper, Some(ownership), request).await
}

/// How each of `ids` stands, read without control: each asked once, in as
/// many reads as the worker's limit needs, their answers in one list. An
/// answer this build cannot read is left out, as one never given.
pub(crate) async fn install_states(client: &Client, ids: &[Token]) -> Result<Vec<InstallState>> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if !unique.iter().any(|known| known == id.as_str()) {
            unique.push(id.as_str().to_owned());
        }
    }
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    // The account the second login's staged files are given to, by the
    // name its ACLs use.
    let helper = match client.desktop().kind() {
        Kind::SecondLogin(login) => helper_name(login.uid)?,
    };
    let mut listed = Vec::new();
    for batch in unique.chunks(ASKED) {
        let request = Request::Installs {
            ids: batch.to_vec(),
        };
        let answer = read(
            send(client, &helper, None, request).await?,
            "installs",
            "The helper's word on its installs was malformed",
        )?;
        listed.extend(
            answer["installs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|install| serde_json::from_value(install.clone()).ok()),
        );
    }
    Ok(listed)
}

#[cfg(test)]
#[path = "one_off_tests.rs"]
mod tests;
