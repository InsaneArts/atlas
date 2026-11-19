//! The native check's fixture in a VM's guest: one copy, staged once for
//! each fixture by its digest and kept across checks at the one path the
//! guest placed it at, so what the person allows it in the VM's window
//! once holds for every later check of that same fixture. Before each reuse
//! the guest reads the copy afresh, with no link followed, and only a copy
//! whose every file is as staged is launched again; one changed or missing
//! a file is removed and the fixture staged anew. The copy is the
//! check's own: its owner record names no session or task, so neither a
//! cleanup nor `close` takes it; only a check of a fixture with another
//! digest replaces it, removing the old copy as a change of its own, and
//! it goes with the VM. Which copy holds the fixture, and its digest, is
//! kept in the VM's records before anything of it is said.
use super::{Owner, Owners, listed, remove, snapshot};
use crate::client::{Client, Verified};
use atlas_core::{
    error::{Code, Error, Result},
    files::{self, Token},
    stage::{Entry, Kind, Snapshot},
    transport::accepted,
};
use atlas_session::compatibility::STAGE;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The record of the fixture's copy, in a VM's records folder.
const RECORD: &str = "native-fixture.json";

/// The longest record read.
const LONGEST: u64 = 4096;

/// Which copy holds the fixture, and the digest of what it holds.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    copy: Token,
    digest: String,
}

fn read(records: &Path) -> Option<Kept> {
    let bytes = files::read_own(&records.join(RECORD), LONGEST)?;
    serde_json::from_slice(&bytes).ok()
}

fn keep(records: &Path, kept: &Kept) -> Result<()> {
    files::write_durable(&records.join(RECORD), &serde_json::to_vec(kept)?, 0o600)
}

fn forget(records: &Path) -> Result<()> {
    let path = records.join(RECORD);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::writing(&path, error)),
    }
}

fn refused(why: String) -> Error {
    Error::new(Code::Native, why)
}

/// Where the guest of `client`'s VM keeps its copy of the fixture at
/// `source`, on this Mac, in the generation of `verified`: the copy already
/// placed for a fixture of this digest while it still holds it as staged,
/// else one staged now, any other copy kept for the fixture removed first.
///
/// # Errors
///
/// Refuses a guest that takes no files or removes no copies, a fixture that
/// cannot be snapshotted, and a copy still being placed or one of another
/// fixture that cannot be removed yet; returns the stage's refusal or
/// failure.
pub(crate) async fn fixture_copy(
    client: &Client,
    verified: &Verified,
    source: &Path,
) -> Result<PathBuf> {
    let name = &client.selected.name;
    let generation = client.generation_offering(verified, STAGE, |vm| {
        format!("{vm} is running an older Atlas that takes no files")
    })?;
    let records = client.output_dir()?.to_owned();
    let owners = Owners::in_records(&records);
    let (source, held) = snapshot(source).await?;
    let digest = held.snapshot().digest.clone();
    if let Some(kept) = read(&records) {
        match listed(client, &kept.copy).await? {
            // Reused only once the guest, reading it afresh, finds every file
            // of it as this fixture's snapshot holds it.
            Some(copy) if copy.state == "placed" && kept.digest == digest => {
                if let Some(path) = copy.path
                    && holds(client, &kept.copy, held.snapshot()).await?
                {
                    return Ok(PathBuf::from(path));
                }
                replaced(client, verified, &kept.copy).await?;
            }
            Some(copy) if copy.state == "placed" => {
                replaced(client, verified, &kept.copy).await?;
            }
            // Gone from the guest: staged again below.
            None => {}
            Some(_) => {
                return Err(refused(format!(
                    "The fixture's copy in {name} is still being placed; check again in a \
                     moment."
                )));
            }
        }
        owners.forget(&kept.copy)?;
        forget(&records)?;
    }

    let copy = Token::new();
    keep(
        &records,
        &Kept {
            copy: copy.clone(),
            digest: digest.clone(),
        },
    )?;
    owners.keep(&Owner {
        copy: copy.clone(),
        session: None,
        task: None,
        plan: None,
        fixture: Some(digest),
    })?;
    let staged = client
        .stage_vm(&copy, &held, &source, generation)
        .await
        .and_then(accepted);
    match staged {
        Ok(reply) => Ok(PathBuf::from(reply["path"].as_str().unwrap_or_default())),
        Err(error) => {
            // A stage that may have placed it stays the fixture's copy.
            if !error.uncertain {
                let _ = owners.forget(&copy);
                let _ = forget(&records);
            }
            Err(error)
        }
    }
}

/// Whether the guest's copy `copy`, read afresh by the guest with no link
/// followed, holds exactly what `snapshot` does: every entry by its place,
/// kind and permission bits, each file by its size and SHA-256 and each
/// link by its target. Extended attributes macOS adds as it keeps a file
/// do not count.
async fn holds(client: &Client, copy: &Token, snapshot: &Snapshot) -> Result<bool> {
    let reply = accepted(
        client
            .link
            .call(&json!({"op": "copy-contents", "copy": copy}))
            .await
            .map_err(Error::certain)?,
    )?;
    let Ok(read) = serde_json::from_value::<Vec<Entry>>(reply["entries"].clone()) else {
        return Ok(false);
    };
    let plain = |entries: &[Entry]| -> Vec<(Vec<String>, Kind, u32)> {
        entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.kind.clone(), entry.mode))
            .collect()
    };
    Ok(plain(&read) == plain(&snapshot.entries))
}

/// Remove the guest's copy `copy` of an earlier fixture, or of this one
/// once it no longer holds it as staged, as a change of its own while it is
/// still the folder the guest lists and nothing runs from it, in the
/// generation of `verified`; one already gone is fine.
async fn replaced(client: &Client, verified: &Verified, copy: &Token) -> Result<()> {
    let name = &client.selected.name;
    let Some(listed) = listed(client, copy).await? else {
        return Ok(());
    };
    let kept = || {
        refused(format!(
            "The copy of an earlier fixture in {name} cannot be removed yet; check again once \
             nothing runs from it."
        ))
    };
    let node = listed
        .node
        .as_deref()
        .filter(|_| listed.state == "placed")
        .ok_or_else(kept)?;
    if listed.in_use(&[]) {
        return Err(kept());
    }
    let reply = remove(client, copy, node, verified).await?;
    match reply["outcome"].as_str() {
        Some("removed" | "alreadyGone") if reply["ok"] == true => Ok(()),
        _ => Err(kept()),
    }
}
