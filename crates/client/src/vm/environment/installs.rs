//! A VM's installs: an app staged into the guest is installed from where
//! the guest placed it by the guest's own environment worker, as a change
//! of its own, and taken away or looked at the same way. Beside what the
//! guest keeps, this Mac keeps its own record of each install that may have
//! reached the guest, in the VM's records, `environment/installs/`: kept
//! before the install is said, marked once the guest acknowledges it placed
//! the app, and gone only once the guest says for certain that it installed
//! nothing, or answers a removal from its own record of the install. A
//! guest that has no record of one never makes this Mac forget it.
use super::{frozen, send, verified};
use crate::{
    client::{Client, RUN_TIMEOUT, RUN_TIMEOUT_MAX, Verified},
    environment::{
        installs::ASKED,
        model::Request,
        one_off, placed_app,
        reply::{InstallState, Outcome, RemovalReply},
    },
};
use atlas_core::{
    error::{Code, Result},
    files::{self, Token},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Where the records of installs are, in a VM's records folder.
const FOLDER: &str = "environment/installs";

/// This Mac's own record of one install that may have reached a VM's guest.
struct Record {
    path: PathBuf,
}

impl Record {
    /// Install `id`'s record in the VM's records folder `records`.
    fn of(records: &Path, id: &Token) -> Self {
        Self {
            path: records
                .join(FOLDER)
                .join(format!("install-{}.json", id.as_str())),
        }
    }

    /// Keep it, before the install is claimed or said; `acknowledged` once
    /// the guest answered that it placed the app.
    fn keep(&self, acknowledged: bool) -> Result<()> {
        let folder = self
            .path
            .parent()
            .ok_or_else(|| crate::environment::problem("Missing installs folder"))?;
        if let Some(environment) = folder.parent() {
            files::private_folder(environment, Code::State)?;
        }
        files::private_folder(folder, Code::State)?;
        let record = serde_json::to_vec(&json!({"version": 1, "acknowledged": acknowledged}))?;
        files::write_atomic(&self.path, &record, 0o600)
    }

    /// Whether this Mac keeps a record of the install, or may: one that
    /// cannot be looked for counts.
    fn kept(&self) -> bool {
        self.path.try_exists().unwrap_or(true)
    }

    fn forget(&self) -> Result<()> {
        frozen::remove(&self.path)
    }
}

/// [`crate::environment::install_app`] on a VM: the bundle the guest
/// placed at `source`, its own path, installed by its worker as
/// `~/Applications/NAME` for install `id`, in the generation of `verified`,
/// the proof the bundle was staged on. This Mac's record of it is kept
/// first: one that cannot be kept sends nothing.
pub(crate) async fn install_app_in(
    client: &Client,
    verified: &Verified,
    id: &Token,
    source: &Path,
    name: &str,
) -> Result<Value> {
    let record = Record::of(client.output_dir()?, id);
    record.keep(false)?;
    let request = Request::InstallApp {
        id: id.as_str().to_owned(),
        source: source.to_owned(),
        name: name.to_owned(),
    };
    // Copying an app into a VM may take as long as any command.
    let mut result = send(client, verified, request, (&Token::new(), RUN_TIMEOUT_MAX)).await;
    match &mut result {
        Ok(answer) if placed_app(answer) => {
            if let Err(error) = record.keep(true) {
                answer["warnings"] = json!([format!(
                    "The install was acknowledged, but Atlas could not mark its own record of \
                     it so; it is kept as one that may have been made: {error}"
                )]);
            }
        }
        // Nothing was installed, for certain: its record goes. One that
        // would not go keeps it as an install that may have been made.
        Ok(_) => {
            let _ = record.forget();
        }
        Err(error) if !error.uncertain => {
            let _ = record.forget();
        }
        Err(_) => {}
    }
    result
}

/// [`crate::environment::remove_app`] on a VM: install `id` taken away by
/// the guest's worker, when it is unchanged and nothing runs from it. A
/// guest with no record of an install this Mac keeps one of moves nothing
/// aside, and the install is kept, as said; this Mac's record goes once the
/// guest answers from its own.
///
/// # Errors
///
/// Returns the failure to prove the VM or to ask the guest; one whose
/// outcome is not known comes back uncertain, pending until the guest
/// answers.
pub(crate) async fn remove_app_in(client: &Client, id: &Token) -> Result<RemovalReply> {
    let verified = verified(client).await?;
    let record = Record::of(client.output_dir()?, id);
    let request = Request::RemoveApp {
        id: id.as_str().to_owned(),
    };
    let answer = send(client, &verified, request, (&Token::new(), RUN_TIMEOUT_MAX)).await?;
    let reply = RemovalReply::read(answer);
    let RemovalReply::Finished(removal) = &reply else {
        return Ok(reply);
    };
    match &removal.outcome {
        // The guest answered from no record of it at all.
        Outcome::AlreadyGone if removal.app.is_none() && record.kept() => {
            let name = &client.selected.name;
            Ok(RemovalReply::Forgotten(format!(
                "{name} has no record of this install, though it may have been made there, so \
                 Atlas moved nothing aside and keeps its own record of it until {name} is removed"
            )))
        }
        Outcome::InUse => Ok(reply),
        // A record left behind changes nothing: the step is finished, and
        // no later removal of this install is planned.
        Outcome::Held { .. } | Outcome::AlreadyGone | Outcome::Kept { .. } => {
            let _ = record.forget();
            Ok(reply)
        }
    }
}

/// [`crate::environment::install_states`] of the installs `unique` on a
/// VM, asked in as many reads as the guest's worker's limit needs: where
/// each is in the guest, as the guest names its paths.
pub(crate) async fn install_states_in(
    client: &Client,
    unique: &[String],
) -> Result<Vec<InstallState>> {
    let verified = verified(client).await?;
    let mut listed = Vec::new();
    for batch in unique.chunks(ASKED) {
        let request = Request::Installs {
            ids: batch.to_vec(),
        };
        let answer = one_off::read(
            send(client, &verified, request, (&Token::new(), RUN_TIMEOUT)).await?,
            "installs",
            "The VM's word on its installs was malformed",
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
