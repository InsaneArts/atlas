//! A file or folder staged into a VM's guest, as a change of its own. Its
//! snapshot was taken here, after every check of what an agent named; the
//! stage is claimed as the guest's before anything goes, its request said
//! through the keeper, and once the guest is ready the snapshot's bytes
//! follow: the manifest's length, the manifest, then every file's contents,
//! read from the very descriptor the snapshot read it through: no path is
//! opened again. Each file is checked to be as it was read before anything
//! is said, each chunk against its hash from the snapshot before it goes,
//! and each file again once it is sent; a change ends the stage before
//! any changed byte goes.
//! A reply lost on its way is asked of the guest, which keeps what the
//! stage came to; the bytes are never sent again.
use super::{Actor, Client};
use crate::link::{Route, Stream};
use atlas_control::{Pending, executors::Executor};
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Result},
    files::Token,
    stage::{CHUNK, Held},
    transport::{self, lost_reply, refusal, reply_lost},
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// The longest line the keeper says before the stage's bytes.
const LINE: u64 = 64 * 1024;

impl Client {
    /// Stage `held`, a snapshot taken at `source`, into this VM's guest as the
    /// change `change`, in the generation `generation` of the keeper's
    /// session its command's proof was taken in. The guest places it in a
    /// folder of its copies named by `change`. What the guest answered, its
    /// claim settled once that answer is certain.
    ///
    /// # Errors
    ///
    /// Refuses a desktop that is not a reachable VM, an agent's stage while
    /// agents are stopped, and a snapshot whose files or paths changed since
    /// it was taken, before anything is claimed. A failure
    /// before the request reached the keeper clears its claim; one after it
    /// may have, and is said uncertain, its claim left pending until the
    /// guest answers what it came to.
    pub(crate) async fn stage_vm(
        &self,
        change: &Token,
        held: &Arc<Held>,
        source: &Path,
        generation: u64,
    ) -> Result<Value> {
        self.reachable()?;
        if !matches!(self.desktop().kind(), Kind::Vm(_)) {
            return Err(self.unreachable());
        }
        let started = Instant::now();
        self.controller.interrupt().check()?;
        let agent = self.actor == Actor::Agent && self.doing.is_some();
        if agent && self.stop_marked() {
            return Err(self.stopped());
        }
        // Bound to what is sent: the paths checked still name the files held.
        still(held, source).await?;
        let snapshot = held.snapshot();
        let manifest = serde_json::to_vec(snapshot)?;
        let request = json!({
            "op": "stage",
            "change": change,
            "name": snapshot.name,
            "digest": snapshot.digest,
            "entries": snapshot.entries.len(),
            "size": snapshot.size(),
            "attributes": snapshot.attribute_bytes(),
            "manifest": manifest.len(),
        });
        let bytes = 8 + manifest.len() as u64 + snapshot.size();
        let claim = self
            .controller
            .quarantine_by(change.as_str(), Executor::Guest)?;
        let link = self
            .link
            .clone()
            .with_timeout(crate::vm::keeper::stage_deadline(bytes));
        let staging = Staging {
            held,
            manifest: &manifest,
        };
        let sent = self
            .send_stage(&link, (change, &request, generation), &claim, &staging)
            .await;
        let reply = match sent {
            Ok(reply) => reply,
            Err(lost) if reply_lost(&lost) => match self.kept_on_vm(change).await {
                Some(mut reply) => {
                    reply["elapsedMs"] = json!(started.elapsed().as_secs_f64() * 1000.0);
                    reply
                }
                None => return Err(self.pending_on_vm(lost)),
            },
            Err(error) => return Err(self.pending_on_vm(error)),
        };
        // Placed is said only with where, and as the very snapshot sent.
        let placed = reply["path"].as_str().is_some_and(|path| !path.is_empty())
            && reply["digest"] == json!(held.snapshot().digest);
        if reply["ok"] == true && !placed {
            return Err(self.pending_on_vm(self.unconfirmed()));
        }
        let reply = self
            .settle(&request, reply, false, Some(claim), started)
            .await?;
        if reply["ok"] == false && reply["uncertain"] == true {
            return Err(self.pending_on_vm(refusal(&reply)));
        }
        Ok(reply)
    }

    /// Send the stage `request`, the change `change`, through `link`'s keeper
    /// in `generation`: connected first, which a signal's stop cuts; its
    /// request said, the last moment a stop refuses it; then `staging`'s
    /// bytes once the guest is ready, and its final reply. Its `claim` is
    /// cleared whenever the request did not reach the keeper whole.
    async fn send_stage(
        &self,
        link: &crate::link::Link,
        (change, request, generation): (&Token, &Value, u64),
        claim: &Pending,
        staging: &Staging<'_>,
    ) -> Result<Value> {
        let interrupt = self.controller.interrupt();
        let route = Route::Stage(change.clone());
        let connected = tokio::select! {
            biased;
            connected = link.connect_change(&route, request, generation) => connected,
            signal = interrupt.cut() => Err(interrupt.cut_short(signal).certain()),
        };
        let (pending, asked) = match connected {
            Ok(connected) => connected,
            Err(error) => {
                self.controller.clear_if(claim)?;
                return Err(error);
            }
        };
        let (Stream::Keeper(mut stream), left) = pending.into_stream() else {
            self.controller.clear_if(claim)?;
            return Err(self.unreachable());
        };
        let until = tokio::time::Instant::now() + left;
        let said = {
            let _sending = interrupt.sending();
            match interrupt.check() {
                Ok(()) => said(&mut stream, &asked, until).await,
                Err(stop) => Err(stop),
            }
        };
        if let Err(error) = said {
            self.controller.clear_if(claim)?;
            return Err(error);
        }
        let exchanged = tokio::select! {
            biased;
            exchanged = tokio::time::timeout_at(until, exchange(stream, staging, until)) => {
                exchanged.unwrap_or_else(|_| {
                    Err(lost_reply(Error::new(
                        Code::Transport,
                        "The guest did not answer the stage in time.",
                    )))
                })
            }
            signal = interrupt.cut() => Err(interrupt.cut_short(signal).uncertain()),
        };
        let reply = exchanged?;
        // The keeper's own word that the guest's reply did not come whole.
        if reply["ok"] == false && reply["sent"] == true && reply["uncertain"] == true {
            return Err(lost_reply(transport::refusal(&reply)));
        }
        Ok(reply)
    }
}

/// What a stage sends: its snapshot, its files held, and the manifest as
/// it is said.
struct Staging<'a> {
    held: &'a Arc<Held>,
    manifest: &'a [u8],
}

/// Refuse `held` once anything it holds changed, or `source` no longer
/// names it, looked at off the runtime.
async fn still(held: &Arc<Held>, source: &Path) -> Result<()> {
    let (held, source) = (held.clone(), source.to_owned());
    tokio::task::spawn_blocking(move || held.still(&source))
        .await
        .map_err(|_| Error::new(Code::Io, "The snapshot could not be looked at again."))?
}

/// Say `asked` to the keeper on `stream`, whole with its newline last, by
/// `until`. A request not written whole was never acted on.
async fn said(
    stream: &mut tokio::net::UnixStream,
    asked: &Value,
    until: tokio::time::Instant,
) -> Result<()> {
    let mut line = serde_json::to_vec(asked)?;
    line.push(b'\n');
    let unsent = |why: String| Error::new(Code::Transport, why).certain();
    match tokio::time::timeout_at(until, stream.write_all(&line)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(unsent(error.to_string())),
        Err(_) => Err(unsent(
            "The VM's session keeper did not take the stage in time".to_owned(),
        )),
    }
}

/// The keeper's first line on `stream`; once it says the guest is ready,
/// the stage's bytes, then the guest's final reply by `until`. A failure
/// once the request was said is a reply lost on its way: the guest keeps
/// what the stage came to.
async fn exchange(
    stream: tokio::net::UnixStream,
    staging: &Staging<'_>,
    until: tokio::time::Instant,
) -> Result<Value> {
    let lost = |why: &str| lost_reply(Error::new(Code::Transport, why));
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    let read = (&mut reader)
        .take(LINE)
        .read_line(&mut first)
        .await
        .map_err(|_| lost("The VM's session keeper ended the stage before it said anything."))?;
    if read == 0 || !first.ends_with('\n') {
        return Err(lost(
            "The VM's session keeper ended the stage before it said anything.",
        ));
    }
    let first: Value = serde_json::from_str(&first)
        .map_err(|_| lost("The VM's session keeper said something not understood."))?;
    if first != json!({"ready": true}) {
        return Ok(first);
    }
    // The keeper says nothing more until every byte is in.
    if !reader.buffer().is_empty() {
        return Err(lost(
            "The VM's session keeper said more than that the guest is ready.",
        ));
    }
    let mut stream = reader.into_inner();
    sent(&mut stream, staging).await.map_err(lost_reply)?;
    let left = until.saturating_duration_since(tokio::time::Instant::now());
    transport::Pending::over_authenticated(Stream::Keeper(stream), left)
        .finish()
        .await
}

/// Send `staging`'s bytes on `stream`: the manifest's length, the manifest,
/// then each file's contents, exactly as long as the snapshot says. Each
/// file is checked to be as it was read before its last bytes go, and the
/// stage's very last byte goes only once every file is: until it goes, the
/// guest cannot have the whole stage, so a change ends it with nothing
/// placed.
async fn sent(stream: &mut tokio::net::UnixStream, staging: &Staging<'_>) -> Result<()> {
    let failed = |error: std::io::Error| {
        Error::new(Code::Transport, "The stage's bytes could not be sent.").with_source(error)
    };
    let sizes: Vec<u64> = staging.held.files().map(|(_, size, _)| size).collect();
    let mut head = (staging.manifest.len() as u64).to_be_bytes().to_vec();
    head.extend_from_slice(staging.manifest);
    let mut last = None;
    if sizes.iter().all(|size| *size == 0) {
        last = head.pop();
    }
    stream.write_all(&head).await.map_err(failed)?;
    let final_file = sizes.iter().rposition(|size| *size > 0);
    for (at, size) in sizes.into_iter().enumerate() {
        let mut offset = 0;
        for index in 0..size.div_ceil(CHUNK as u64) {
            let index = usize::try_from(index).map_err(|_| changed())?;
            let mut chunk = read_chunk(staging.held, at, index, size).await?;
            offset += chunk.len() as u64;
            if offset == size {
                // Read whole as it was: its last bytes go only then.
                unchanged(staging.held, at).await?;
                if Some(at) == final_file {
                    last = chunk.pop();
                }
            }
            stream.write_all(&chunk).await.map_err(failed)?;
        }
    }
    for at in 0..staging.held.files().count() {
        unchanged(staging.held, at).await?;
    }
    if let Some(last) = last {
        stream.write_all(&[last]).await.map_err(failed)?;
    }
    stream.flush().await.map_err(failed)
}

fn changed() -> Error {
    Error::new(
        Code::Io,
        "A staged file changed while it was sent; nothing was staged.",
    )
}

/// Chunk `index` of `held`'s file `at`, of `size` bytes, read off the
/// runtime through the descriptor the snapshot read it with, and only as
/// the snapshot read it: a chunk changed since stops the stage before any
/// of it goes.
async fn read_chunk(held: &Arc<Held>, at: usize, index: usize, size: u64) -> Result<Vec<u8>> {
    let held = held.clone();
    tokio::task::spawn_blocking(move || {
        let file = held.file(at).ok_or_else(changed)?;
        file.chunk(index, size).map_err(|_| changed())
    })
    .await
    .map_err(|_| Error::new(Code::Io, "A staged file could not be read."))?
}

/// Refuse `held`'s file `at` once it is not as it was read, looked at off
/// the runtime.
async fn unchanged(held: &Arc<Held>, at: usize) -> Result<()> {
    let held = held.clone();
    tokio::task::spawn_blocking(move || {
        let file = held.file(at).ok_or_else(changed)?;
        file.unchanged().map_err(|_| changed())
    })
    .await
    .map_err(|_| Error::new(Code::Io, "A staged file could not be looked at."))?
}
