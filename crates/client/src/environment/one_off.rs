//! One-off requests of a desktop's environment worker: the helper's list of
//! its copies, and installing an app, removing one or asking how installs
//! stand. A second login's go through a request folder of their own, beside
//! the shared command, that goes once the outcome is known; a VM's, as
//! changes its guest's own worker carries out. A change runs under the
//! control its caller holds and stays pending while its outcome is not
//! known, as every change does.
use super::{
    reply::{InstallState, Installed, InstalledApp, RemovalReply, WorkerReply},
    *,
};
use crate::client::Verified;
use atlas_core::files::Token;

/// The most of an install's output read for the app it placed: an install's
/// answer is a few hundred bytes.
const INSTALLED_OUTPUT: usize = 64 * 1024;

/// `answer`, a read's, when the helper accepted it and it holds the array
/// `field`; a refusal keeps its code, and anything else is said as
/// `malformed`, never read as an empty answer.
pub(crate) fn read(answer: Value, field: &str, malformed: &str) -> Result<Value> {
    if answer["ok"] == false {
        return Err(atlas_core::transport::refusal(&answer));
    }
    if answer["ok"] != true || !answer[field].is_array() {
        return Err(Error::new(Code::Protocol, malformed.to_owned()));
    }
    Ok(answer)
}

/// Install the bundle at `source`, which the desktop can read, as
/// `~/Applications/NAME` for install `id`, under `ownership`: on a VM, in
/// the generation of `verified`, the proof its bundle was staged on.
pub(crate) async fn install_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
    source: &Path,
    name: &str,
    verified: Option<&Verified>,
) -> Result<Value> {
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::install_app_in(client, login, ownership, id, source, name)
                .await
        }
        Kind::Vm(_) => {
            let verified = verified.ok_or_else(|| client.unreachable())?;
            crate::vm::environment::install_app_in(client, verified, id, source, name).await
        }
    }
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
/// under `ownership`: what the desktop answered.
pub(crate) async fn remove_app(
    client: &Client,
    ownership: &Ownership,
    id: &Token,
) -> Result<RemovalReply> {
    match client.desktop().kind() {
        Kind::User(login) => crate::user::environment::remove_app_in(client, login, ownership, id)
            .await
            .map(RemovalReply::read),
        Kind::Vm(_) => crate::vm::environment::remove_app_in(client, id).await,
    }
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
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::environment::install_states_in(client, login, &unique).await
        }
        Kind::Vm(_) => crate::vm::environment::install_states_in(client, &unique).await,
    }
}

#[cfg(test)]
#[path = "one_off_tests.rs"]
mod tests;
