//! A VM guest's copies of what `open` hands it: each file or folder an agent
//! named is snapshotted here, after every check of what it names, and
//! staged into the guest as a change of its own, which the guest places in
//! a folder of its copies named by that change. Nothing on this disk is
//! read for it but the snapshot's own files. What the guest keeps is known
//! only from its inventory, which says each copy's folder, its identity and
//! what runs from it; a copy is removed as a `remove-copy` change, only
//! while it is still the folder that inventory named.
use crate::{
    client::{AppRow, Client, Verified},
    handoff::{Copied, CopyRemoval, Failed, Handed, Removed, Target, runs_from},
    host_session::{HostSession, Key},
    task::{Claims, Owned as Claimed, State},
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    name::same_text,
    stage::{Held, Snapshot},
    transport::accepted,
};
use atlas_session::compatibility::{COPIES, STAGE};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

mod fixture;
mod owners;

pub(crate) use fixture::fixture_copy;
pub(crate) use owners::{Owned, Owner, Owners};

/// The most pages of the guest's inventory read in one listing.
const PAGES: usize = 1024;

/// One copy as the guest lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuestCopy {
    pub(crate) copy: Token,
    /// `placed`, or `staging`, `removing` or `pending` while a change to it
    /// is under way or not yet put right.
    pub(crate) state: String,
    /// Its folder's identity, as `dev:ino`, when it is there.
    pub(crate) node: Option<String>,
    /// The processes running from it; none when the guest cannot tell.
    pub(crate) processes: Option<Vec<i64>>,
    /// Where the item staged into it is, once it is placed.
    pub(crate) path: Option<String>,
}

impl GuestCopy {
    /// One entry of the guest's inventory.
    fn of(entry: &Value) -> Option<Self> {
        let processes = match &entry["processes"] {
            Value::Array(pids) => Some(pids.iter().map(Value::as_i64).collect::<Option<_>>()?),
            _ => None,
        };
        Some(Self {
            copy: Token::parse(entry["copy"].as_str()?)?,
            state: entry["state"].as_str()?.to_owned(),
            node: entry["node"].as_str().map(str::to_owned),
            processes,
            path: entry["path"].as_str().map(str::to_owned),
        })
    }

    /// The copy's folder in the guest, once placed: what runs from it is
    /// judged by.
    pub(crate) fn folder(&self) -> Option<String> {
        let path = Path::new(self.path.as_deref()?);
        Some(path.parent()?.to_string_lossy().into_owned())
    }

    /// The app it holds, when it holds one: never a copied file's name.
    pub(crate) fn app(&self) -> Option<String> {
        let path = Path::new(self.path.as_deref()?);
        let app = path.extension().is_some_and(|extension| extension == "app");
        app.then(|| path.file_stem()?.to_str().map(atlas_core::name::bounded))?
    }

    /// The name what it holds runs as: its file's name, as it was staged.
    fn name(&self) -> Option<String> {
        let path = Path::new(self.path.as_deref()?);
        path.file_stem()?.to_str().map(str::to_owned)
    }

    /// Whether anything may still use it: the guest cannot tell, something
    /// runs from it, or an app in `running` (from a list the guest gave)
    /// runs from its folder.
    fn in_use(&self, running: &[AppRow]) -> bool {
        let folder = self.folder();
        self.processes
            .as_ref()
            .is_none_or(|processes| !processes.is_empty())
            || running.iter().any(|app| {
                folder.as_deref().is_some_and(|folder| {
                    runs_from(app.executable().unwrap_or(""), Path::new(folder))
                })
            })
    }
}

/// Every copy the guest keeps, as its inventory lists them now.
///
/// # Errors
///
/// Fails when the keeper or the guest cannot be asked, or the guest does
/// not answer an inventory.
pub(crate) async fn inventory(client: &Client) -> Result<Vec<GuestCopy>> {
    let mut copies = Vec::new();
    let mut after: Option<Token> = None;
    for _ in 0..PAGES {
        let mut request = json!({"op": "copies"});
        if let Some(after) = &after {
            request["after"] = json!(after);
        }
        let page = read(client, &request).await?;
        copies.extend(page.0);
        match page.1 {
            Some(next) => after = Some(next),
            None => return Ok(copies),
        }
    }
    Err(Error::new(
        Code::Protocol,
        "The VM listed more copies than Atlas reads.",
    ))
}

/// The copy `copy`, as the guest's inventory lists it now; none when it
/// keeps nothing of it.
///
/// # Errors
///
/// As [`inventory`].
pub(crate) async fn listed(client: &Client, copy: &Token) -> Result<Option<GuestCopy>> {
    let (copies, _) = read(client, &json!({"op": "copies", "copy": copy})).await?;
    Ok(copies.into_iter().find(|listed| listed.copy == *copy))
}

/// One page of the guest's inventory, and the copy the next one is after.
async fn read(client: &Client, request: &Value) -> Result<(Vec<GuestCopy>, Option<Token>)> {
    let reply = accepted(client.link.call(request).await.map_err(Error::certain)?)?;
    let copies = reply["copies"]
        .as_array()
        .map(|copies| copies.iter().map(GuestCopy::of).collect::<Option<Vec<_>>>())
        .and_then(|copies| copies)
        .ok_or_else(|| {
            Error::new(
                Code::Protocol,
                "The VM's guest listed its copies in a way this Atlas does not read.",
            )
        })?;
    let next = reply["next"].as_str().and_then(Token::parse);
    Ok((copies, next))
}

/// [`crate::handoff::hand_over`] to a VM, whose guest reads nothing of this
/// disk: names and URLs go as they are, and every path is staged into the
/// guest, each a change of its own, all in the generation of `verified`,
/// the command's own proof: a guest started anew since refuses them.
/// Who each copy is made for is kept here before anything of it is said,
/// and its change is claimed for the command's task, settled once it is
/// known how it went. When a stage fails, the copies already made for the
/// others go. The caller holds `ownership` throughout, so no other open,
/// close or cleanup comes between.
///
/// # Errors
///
/// Refuses a snapshot of what cannot be staged, or of what holds the
/// person's sign-ins or secrets, and a copy whose owner cannot be kept;
/// returns the stage's refusal or failure.
pub(crate) async fn hand_over_to(
    client: &Client,
    _ownership: &Ownership,
    targets: Vec<Target>,
    verified: &Verified,
) -> Result<Handed> {
    let mut placed = Vec::new();
    for target in targets {
        placed.push(match target {
            Target::Name(name) => Placed::Given(name),
            Target::Path(path) => {
                let (source, held) = snapshot(&path).await?;
                Placed::Staged {
                    path,
                    source,
                    held,
                    token: Token::new(),
                }
            }
        });
    }
    let tokens: Vec<Token> = placed
        .iter()
        .filter_map(|placed| match placed {
            Placed::Staged { token, .. } => Some(token.clone()),
            Placed::Given(_) => None,
        })
        .collect();
    if tokens.is_empty() {
        let paths = placed.into_iter().map(Placed::given).collect();
        return Ok(Handed {
            paths,
            copied: Vec::new(),
        });
    }
    let generation = client.generation_offering(verified, STAGE, |vm| {
        format!("{vm} is running an older Atlas that takes no files to open")
    })?;
    let session = client.host_session.as_ref().map(HostSession::key).cloned();
    let owners = Owners::in_records(client.output_dir()?);
    let kept = kept_for(&owners, &tokens, &session, client);
    if let Err(error) = kept {
        for token in &tokens {
            let _ = owners.forget(token);
        }
        return Err(error);
    }
    let claims = Claims::of(client);
    let claimed: Vec<Claimed> = tokens
        .iter()
        .map(|id| Claimed::Copy { id: id.clone() })
        .collect();
    let _ = claims.claim_all(&claimed, State::Intent);

    let mut paths = Vec::new();
    let mut made: Vec<Copied> = Vec::new();
    let mut failed = None;
    for placed in placed {
        let (path, source, held, token) = match placed {
            Placed::Given(given) => {
                paths.push(given);
                continue;
            }
            Placed::Staged {
                path,
                source,
                held,
                token,
            } => (path, source, held, token),
        };
        let staged = client
            .stage_vm(&token, &held, &source, generation)
            .await
            .and_then(accepted);
        let to = match staged {
            Ok(reply) => PathBuf::from(reply["path"].as_str().unwrap_or_default()),
            Err(error) => {
                failed = Some((token, error));
                break;
            }
        };
        paths.push(to.to_string_lossy().into_owned());
        made.push(Copied {
            from: path,
            to,
            token,
        });
    }

    let Some((token, error)) = failed else {
        let settled: Vec<(Claimed, State)> = tokens
            .into_iter()
            .map(|id| (Claimed::Copy { id }, State::Made))
            .collect();
        let _ = claims.settle_all(&settled);
        return Ok(Handed {
            paths,
            copied: made,
        });
    };
    // Nothing has opened these copies yet: they go with the failure. One
    // whose stage may have placed it, or that would not go, stays claimed
    // and owned.
    let mut settled = Vec::new();
    for id in tokens {
        let gone = if made.iter().any(|copy| copy.token == id) {
            removal(client, &id, (session.as_ref(), verified), &[]).await == CopyRemoval::Removed
        } else {
            let gone = id != token || !error.uncertain;
            gone && owners.forget(&id).is_ok()
        };
        if gone {
            settled.push((Claimed::Copy { id }, State::Gone));
        }
    }
    let _ = claims.settle_all(&settled);
    Err(error)
}

/// Keep, in `owners`, each of `tokens` as made for `session` and `client`'s
/// task.
fn kept_for(
    owners: &Owners,
    tokens: &[Token],
    session: &Option<Key>,
    client: &Client,
) -> Result<()> {
    let task = client.task.as_ref().map(|task| task.key().clone());
    for copy in tokens {
        owners.keep(&Owner {
            copy: copy.clone(),
            session: session.clone(),
            task: task.clone(),
            plan: None,
            fixture: None,
        })?;
    }
    Ok(())
}

/// Stage `payloads`, the folder of a frozen plan's payloads on this Mac,
/// into the guest as the change `copy`, in the generation of `verified`,
/// the command's own proof: where the guest placed it. Its owner is kept
/// first, naming the plan, so only that plan's discard removes it; one
/// whose stage may have placed it stays owned. The folder is the one the
/// plan's own record made, in Atlas's records, never a path an agent
/// names, so it is read as Atlas reads its own records: never through the
/// guard for files handed over, which refuses Atlas's own folder.
///
/// # Errors
///
/// Refuses a guest that takes no stages, an owner that cannot be kept and
/// a folder that cannot be snapshotted; returns the stage's refusal or
/// failure.
pub(in crate::vm) async fn stage_frozen(
    client: &Client,
    payloads: &crate::vm::environment::Payloads,
    copy: &Token,
    verified: &Verified,
) -> Result<PathBuf> {
    let generation = client.generation_offering(verified, STAGE, |vm| {
        format!("{vm} is running an older Atlas that takes no files")
    })?;
    let owners = Owners::in_records(client.output_dir()?);
    owners.keep(&Owner {
        copy: copy.clone(),
        session: client.host_session.as_ref().map(HostSession::key).cloned(),
        task: client.task.as_ref().map(|task| task.key().clone()),
        plan: Some(payloads.plan().clone()),
        fixture: None,
    })?;
    let folder = payloads.folder().to_owned();
    let held = tokio::task::spawn_blocking({
        let folder = folder.clone();
        move || {
            let held = Snapshot::held(&folder)?;
            held.still(&folder)?;
            Ok::<_, Error>(Arc::new(held))
        }
    })
    .await
    .map_err(|_| Error::new(Code::Io, "The plan's payloads could not be read."))
    .and_then(|held| held);
    let staged = match held {
        Ok(held) => client
            .stage_vm(copy, &held, &folder, generation)
            .await
            .and_then(accepted),
        Err(error) => Err(error),
    };
    match staged {
        Ok(reply) => Ok(PathBuf::from(reply["path"].as_str().unwrap_or_default())),
        Err(error) => {
            if !error.uncertain {
                let _ = owners.forget(copy);
            }
            Err(error)
        }
    }
}

/// What one target becomes for the guest.
enum Placed {
    /// A name or URL, opened as it is.
    Given(String),
    /// A path, staged as the snapshot `held` taken at `source`, its real
    /// path, under the change `token`.
    Staged {
        path: PathBuf,
        source: PathBuf,
        held: Arc<Held>,
        token: Token,
    },
}

impl Placed {
    fn given(self) -> String {
        match self {
            Self::Given(given) => given,
            Self::Staged { path, .. } => path.to_string_lossy().into_owned(),
        }
    }
}

/// The snapshot of `path`, taken off the runtime at its real path, which
/// no other account can change, its files held open to be sent from; and
/// that real path. Every entry is checked against the person's sign-ins and
/// secrets and Atlas's own folder (a folder goes whole), and the check is
/// bound to what was held: each path checked must still name the very file
/// read.
async fn snapshot(path: &Path) -> Result<(PathBuf, Arc<Held>)> {
    let named = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let source = atlas_core::checked::handover_path(&named)?;
        let held = Snapshot::held(&source)?;
        let paths = held.paths(&source);
        crate::credential_files::refuse(paths.iter().map(PathBuf::as_path))?;
        held.still(&source)?;
        Ok((source, Arc::new(held)))
    })
    .await
    .map_err(|_| Error::new(Code::Io, "The snapshot to stage could not be taken."))?
}

/// Remove the guest's copy `copy`, while it is still the folder `node`
/// says, as a `remove-copy` change the guest keeps, in the generation of
/// `verified`, the command's own proof: what it came to.
///
/// # Errors
///
/// Returns the failure to prove the VM, claim the change or reach the
/// guest; an uncertain one leaves its claim pending until the guest
/// answers.
pub(crate) async fn remove(
    client: &Client,
    copy: &Token,
    node: &str,
    verified: &Verified,
) -> Result<Value> {
    let generation = client.generation_offering(verified, COPIES, |vm| {
        format!("{vm} is running an older Atlas that removes no copies")
    })?;
    let mutation = crate::client::Mutation::RemoveCopy { copy, node };
    client.mutate_vm(mutation, generation, None).await
}

/// Remove the guest's copy `copy` for a command of host session `session`,
/// or none, in the generation of `verified`, the command's own proof, by
/// the rule every removal of a copy follows: only a copy whose record here
/// names that session (one with no record, or an unreadable one, is never
/// taken as anyone's), and never one holding a frozen plan's payloads or
/// the native check's fixture;
/// kept while the guest lists it as anything but placed, and while
/// anything runs from it or an app in `running` (from a list the guest
/// gave) does. Its record goes once it is known gone.
pub(crate) async fn removal(
    client: &Client,
    copy: &Token,
    (session, verified): (Option<&Key>, &Verified),
    running: &[AppRow],
) -> CopyRemoval {
    let owners = match client.output_dir() {
        Ok(records) => Owners::in_records(records),
        Err(error) => return CopyRemoval::Failed(error.error),
    };
    match owners.of(copy) {
        // A frozen plan's payloads go only with its plan, and the native
        // check's fixture only when a check of another fixture replaces it.
        Owned::By(owner) if owner.plan.is_some() || owner.fixture.is_some() => {
            return CopyRemoval::AnotherSession;
        }
        Owned::By(owner) if owner.names(session) => {}
        Owned::By(_) => return CopyRemoval::AnotherSession,
        Owned::Unknown | Owned::Unreadable => return CopyRemoval::Unreadable,
    }
    let gone = |came: CopyRemoval| match owners.forget(copy) {
        Ok(()) => came,
        Err(error) => CopyRemoval::Failed(error.error),
    };
    let listed = match listed(client, copy).await {
        Ok(Some(listed)) => listed,
        Ok(None) => return gone(CopyRemoval::AlreadyGone),
        Err(error) => return CopyRemoval::Failed(error.error),
    };
    let Some(node) = listed.node.as_deref().filter(|_| listed.state == "placed") else {
        return CopyRemoval::Unreadable;
    };
    if listed.in_use(running) {
        return CopyRemoval::InUse;
    }
    match remove(client, copy, node, verified).await {
        Ok(reply) if reply["ok"] == true => match reply["outcome"].as_str() {
            Some("removed") => gone(CopyRemoval::Removed),
            Some("alreadyGone") => gone(CopyRemoval::AlreadyGone),
            Some("inUse" | "unknown") => CopyRemoval::InUse,
            Some("changed") => CopyRemoval::Unreadable,
            _ => CopyRemoval::Failed(format!("The guest kept it ({})", reply["outcome"])),
        },
        Ok(reply) => CopyRemoval::Failed(
            reply["error"]
                .as_str()
                .unwrap_or("The guest refused to remove it")
                .to_owned(),
        ),
        Err(error) => CopyRemoval::Failed(error.error),
    }
}

/// Remove the guest's copies of `app`, which has quit, by the rule
/// [`removal`] follows: those the app is, or a file named like it.
pub(crate) async fn remove_of_app(
    client: &Client,
    app: &str,
    running: &[AppRow],
    (session, verified): (Option<&Key>, &Verified),
) -> Removed {
    let mut removed = Removed::default();
    let copies = match inventory(client).await {
        Ok(copies) => copies,
        Err(error) => {
            removed.failed.push(Failed {
                path: app.to_owned(),
                error: error.error,
            });
            return removed;
        }
    };
    for copy in copies
        .iter()
        .filter(|copy| copy.name().is_some_and(|name| same_text(&name, app)))
    {
        let folder = copy
            .folder()
            .unwrap_or_else(|| copy.copy.as_str().to_owned());
        match removal(client, &copy.copy, (session, verified), running).await {
            CopyRemoval::Removed => removed.folders.push(folder),
            CopyRemoval::AnotherSession | CopyRemoval::Unreadable => removed.kept_others += 1,
            CopyRemoval::Failed(error) => removed.failed.push(Failed {
                path: folder,
                error,
            }),
            CopyRemoval::AlreadyGone | CopyRemoval::InUse => {}
        }
    }
    removed
}

#[cfg(test)]
#[path = "copies_tests.rs"]
mod tests;
