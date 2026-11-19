//! The stand-in's copies, as a machine guest keeps them: each stage's
//! manifest read and checked, its bytes read with credit given back for
//! every read, then placed under [`ROOT`] in a folder named by its change;
//! listed with what runs from each, and removed as a change of its own.
use super::{Asked, StandIn};
use crate::{
    control::Said,
    lane::{Header, Lanes},
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    stage::Snapshot,
};
use serde_json::{Value, json};
use std::sync::{PoisonError, atomic::Ordering::SeqCst};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Where the stand-in places what is staged into it, as a guest names it.
pub const ROOT: &str = "/Users/guest/Library/Application Support/Atlas/open";

/// The most read from a stage's lane at once.
const CHUNK: usize = 64 * 1024;

/// One copy the stand-in keeps: what was staged, what it said of itself,
/// and what runs from it.
#[derive(Clone, Debug)]
pub struct Copy {
    pub name: String,
    pub digest: String,
    /// Its folder's identity, as `dev:ino`.
    pub node: String,
    /// The processes running from it; none when that cannot be told.
    pub processes: Option<Vec<i32>>,
    /// Every file's contents, in the manifest's order.
    pub contents: Vec<u8>,
    /// The manifest it was staged with, which says where each file's
    /// contents begin.
    pub snapshot: Snapshot,
    /// What it was staged for, when not an agent's own use.
    pub purpose: Option<String>,
}

impl StandIn {
    /// The copies it keeps, by their change.
    pub fn copies(&self) -> std::sync::MutexGuard<'_, std::collections::BTreeMap<Token, Copy>> {
        self.placed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Give `bytes` of `header`'s lane's credit back, unless the stand-in
    /// withholds credit: then it is owed until [`StandIn::give_credit_back`].
    fn credit(&self, header: &Header, bytes: usize) {
        let mut withheld = self.withheld.lock().unwrap_or_else(PoisonError::into_inner);
        if self.withholding.load(SeqCst) {
            *withheld.entry(header.lane).or_default() += bytes as u64;
            return;
        }
        drop(withheld);
        self.say_credit(header.lane, bytes as u64);
    }

    /// Stop withholding credit, and give back all it withheld.
    pub fn give_credit_back(&self) {
        let owed = {
            let mut withheld = self.withheld.lock().unwrap_or_else(PoisonError::into_inner);
            self.withholding.store(false, SeqCst);
            std::mem::take(&mut *withheld)
        };
        for (lane, bytes) in owed {
            self.say_credit(lane, bytes);
        }
    }

    fn say_credit(&self, lane: crate::lane::LaneId, bytes: u64) {
        if let Some(outbox) = self
            .outbox
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            let _ = outbox.say(Said::Credit { lane, bytes });
        }
    }

    /// The copies it keeps as a `copies` read answers them: the one
    /// `request` names, or every one.
    pub(super) fn inventory(&self, request: &Value) -> Value {
        let only = request["copy"].as_str().and_then(Token::parse);
        let listed: Vec<Value> = self
            .copies()
            .iter()
            .filter(|(token, _)| only.as_ref().is_none_or(|only| only == *token))
            .map(|(token, copy)| {
                json!({
                    "copy": token,
                    "state": "placed",
                    "node": copy.node,
                    "processes": copy.processes.as_ref().map_or(json!("unknown"), |pids| json!(pids)),
                    "path": format!("{ROOT}/{token}/{}", copy.name),
                    "evidence": {"change": token, "digest": copy.digest},
                })
            })
            .collect();
        json!({"ok": true, "op": "copies", "copies": listed, "next": null, "unknown": []})
    }

    /// What the copy `request` names holds now, as a guest reads it afresh:
    /// each file's size and SHA-256 from the bytes it keeps, in the
    /// manifest's order; `null` entries when it keeps no such copy, or its
    /// bytes no longer cover what its manifest says.
    pub(super) fn copy_contents(&self, request: &Value) -> Value {
        let Some(token) = request["copy"].as_str().and_then(Token::parse) else {
            return json!({"ok": false, "code": "bad-request",
                "error": "copy-contents requires the \"copy\" it reads"});
        };
        let entries = self.copies().get(&token).and_then(|copy| {
            let mut offset = 0usize;
            copy.snapshot
                .entries
                .iter()
                .map(|entry| {
                    let mut entry = entry.clone();
                    if let atlas_core::stage::Kind::File { size, sha256 } = &mut entry.kind {
                        let end = offset.checked_add(usize::try_from(*size).ok()?)?;
                        *sha256 = atlas_core::digest::bytes_sha256(copy.contents.get(offset..end)?);
                        offset = end;
                    }
                    Some(entry)
                })
                .collect::<Option<Vec<_>>>()
        });
        json!({"ok": true, "op": "copy-contents", "copy": token, "entries": entries})
    }

    /// What removing the copy `request` names comes to, as a guest removes
    /// it: only while it is still the folder its node says and nothing runs
    /// from it.
    pub(super) fn remove_copy(&self, request: &Value) -> Value {
        let Some(token) = request["copy"].as_str().and_then(Token::parse) else {
            return json!({"ok": false, "code": "bad-request",
                "error": "remove-copy requires the \"copy\" it removes"});
        };
        let outcome = |outcome: &str, pids: &[i32]| {
            json!({"ok": true, "op": "remove-copy", "copy": token, "outcome": outcome,
                "pids": pids})
        };
        let mut copies = self.copies();
        let Some(copy) = copies.get(&token) else {
            return outcome("alreadyGone", &[]);
        };
        if request["node"] != json!(copy.node) {
            return outcome("changed", &[]);
        }
        match &copy.processes {
            None => return outcome("unknown", &[]),
            Some(pids) if !pids.is_empty() => return outcome("inUse", pids),
            Some(_) => {}
        }
        copies.remove(&token);
        outcome("removed", &[])
    }
}

/// Serve the stage `request` asks for on its lane `header` opened, as a
/// machine guest does: its reply to say, none when nothing goes (it was cut
/// short, or the stand-in drops it).
pub(super) async fn staged<S>(
    stand_in: &StandIn,
    lanes: &Lanes,
    header: &Header,
    change: &Token,
    request: &Value,
    lane: &mut S,
) -> Result<Option<Value>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match stand_in.ask(change) {
        Asked::Again(Some(reply)) => return Ok(Some(reply)),
        Asked::Again(None) => {
            return Ok(Some(json!({"ok": false, "code": "uncertain",
                "error": "What this stage came to is unknown", "uncertain": true})));
        }
        Asked::Begun => {}
    }
    let asked = (
        request["name"].as_str(),
        request["digest"].as_str(),
        request["manifest"].as_u64(),
        request["size"].as_u64(),
    );
    let (Some(name), Some(digest), Some(manifest), Some(size)) = asked else {
        let refused = json!({"ok": false, "code": "bad-request",
            "error": "The stage could not be read"});
        stand_in.done(change, &refused);
        return Ok(Some(refused));
    };
    let stops = stand_in.stops.load(SeqCst);
    let mut ready = serde_json::to_vec(&json!({"ready": true}))?;
    ready.push(b'\n');
    lane.write_all(&ready).await.map_err(io)?;
    lane.flush().await.map_err(io)?;
    let read = tokio::select! {
        read = received(stand_in, (header, change), manifest, size, lane) => read,
        () = lanes.until_cancelled(header.lane) => Err("it was cancelled".to_owned()),
    };
    let (snapshot, contents) = match read.and_then(|(snapshot, contents)| {
        let matches = snapshot.check().is_ok()
            && snapshot.name == name
            && snapshot.digest == digest
            && snapshot.size() == size
            && request["entries"] == json!(snapshot.entries.len())
            && request["attributes"] == json!(snapshot.attribute_bytes());
        if matches {
            Ok((snapshot, contents))
        } else {
            Err("its manifest is not the one it asked for".to_owned())
        }
    }) {
        Ok(read) => read,
        Err(why) => {
            let placed_nothing = nothing_placed(&why);
            stand_in.done(change, &placed_nothing);
            return Ok(Some(placed_nothing));
        }
    };
    stand_in.received.fetch_add(1, SeqCst);
    while stand_in.holding.load(SeqCst) {
        if lanes.cancelled(header.lane) {
            stand_in.done(change, &nothing_placed("it was cancelled"));
            return Ok(None);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let reply = if stand_in.stops.load(SeqCst) == stops {
        let node = format!("1:{}", stand_in.nodes.fetch_add(1, SeqCst) + 100);
        stand_in.copies().insert(
            change.clone(),
            Copy {
                name: name.to_owned(),
                digest: digest.to_owned(),
                node,
                processes: Some(Vec::new()),
                contents,
                snapshot,
                purpose: request["purpose"].as_str().map(str::to_owned),
            },
        );
        json!({"ok": true, "path": format!("{ROOT}/{change}/{name}"), "digest": digest})
    } else {
        nothing_placed("agents were stopped")
    };
    stand_in.done(change, &reply);
    let dropped = stand_in
        .dropping
        .fetch_update(SeqCst, SeqCst, |left| left.checked_sub(1))
        .is_ok();
    Ok((!dropped).then_some(reply))
}

/// Read a stage's manifest, its length first, then its `size` bytes of
/// contents, crediting every read: the snapshot and the contents, or why
/// they did not all come.
async fn received<S>(
    stand_in: &StandIn,
    (header, change): (&Header, &Token),
    manifest: u64,
    size: u64,
    lane: &mut S,
) -> std::result::Result<(Snapshot, Vec<u8>), String>
where
    S: AsyncRead + Unpin,
{
    let hung_up = |_| "the station hung up".to_owned();
    let length = lane.read_u64().await.map_err(hung_up)?;
    stand_in.credit(header, 8);
    if length != manifest {
        return Err("its manifest is not the length it said".into());
    }
    let manifest = bytes(stand_in, (header, None), manifest, lane).await?;
    let snapshot: Snapshot =
        serde_json::from_slice(&manifest).map_err(|_| "its manifest could not be read")?;
    let contents = bytes(stand_in, (header, Some(change)), size, lane).await?;
    Ok((snapshot, contents))
}

/// Exactly `count` bytes from `lane`, each read credited, and each kept as
/// it comes among what `change`'s stage received, when one is named.
async fn bytes<S>(
    stand_in: &StandIn,
    (header, change): (&Header, Option<&Token>),
    count: u64,
    lane: &mut S,
) -> std::result::Result<Vec<u8>, String>
where
    S: AsyncRead + Unpin,
{
    let mut read = Vec::new();
    let mut buffer = vec![0_u8; CHUNK];
    let mut left = count;
    while left > 0 {
        let room = usize::try_from(left).unwrap_or(CHUNK).min(CHUNK);
        let got = lane
            .read(&mut buffer[..room])
            .await
            .map_err(|_| "the station hung up".to_owned())?;
        if got == 0 {
            return Err("the station hung up".into());
        }
        stand_in.credit(header, got);
        read.extend_from_slice(&buffer[..got]);
        if let Some(change) = change {
            stand_in
                .partial
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(change.clone())
                .or_default()
                .extend_from_slice(&buffer[..got]);
        }
        left -= got as u64;
    }
    Ok(read)
}

/// The reply of a stage that placed nothing.
fn nothing_placed(why: &str) -> Value {
    json!({"ok": false, "code": "cancelled", "error": format!("Nothing was staged: {why}")})
}

fn io(error: std::io::Error) -> Error {
    Error::new(Code::Io, "The stand-in's lane failed.").with_source(error)
}
