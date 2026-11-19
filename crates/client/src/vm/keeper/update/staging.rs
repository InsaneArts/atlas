//! The keeper's stage of its own bundle into its guest, for an update: the
//! request said on a stage lane of the live generation, naming the update
//! as its purpose; once the guest is ready, the snapshot's bytes (the
//! manifest's length, the manifest, then every file's contents) sent from
//! the files the snapshot holds open, one chunk at a time as the guest's
//! credit allows, each read as the snapshot read it. The keeper never
//! holds more than one chunk of it.
use super::super::{Session, change, lanes::Ticket, stage};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error},
    files::Token,
    stage::{CHUNK, Held},
};
use atlas_session::lane::{LaneId, LaneKind};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// The stage request of `held`, the change `change`, for an update.
pub(super) fn request(change: &Token, held: &Held) -> Value {
    let snapshot = held.snapshot();
    json!({
        "op": "stage",
        "change": change,
        "name": snapshot.name,
        "digest": snapshot.digest,
        "entries": snapshot.entries.len(),
        "size": snapshot.size(),
        "attributes": snapshot.attribute_bytes(),
        "manifest": manifest(held).len(),
        "purpose": "update",
    })
}

fn manifest(held: &Held) -> Vec<u8> {
    serde_json::to_vec(held.snapshot()).unwrap_or_default()
}

/// What saying a stage came to.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Staging {
    /// The guest's final reply.
    Answered(Value),
    /// Nothing of its request went: its lane never opened, the keeper was
    /// no longer to say it once it did, or its line was never written whole.
    Unsent,
    /// It may have gone, and its final reply did not come.
    Lost,
}

/// Stage `held` into the guest of `session` as the change `change`. Its
/// lane opened, its request is said only while `still` holds, judged right
/// before it is written, under the keeper's `turn` and the desktop's
/// `exclusion`: the turn goes once the request is said, the exclusion once
/// the guest's first answer came, before any byte of the stage. A hold that
/// stops its `ticket` cancels its lane, and its credit comes back before
/// the ticket goes.
pub(super) async fn staged(
    session: &Session,
    (change, held): (&Token, Arc<Held>),
    (turn, exclusion, ticket): (tokio::sync::MutexGuard<'_, ()>, Ownership, &Ticket<'_>),
    still: &(dyn Fn() -> bool + Sync),
) -> Staging {
    let manifest = manifest(&held);
    let bytes = 8 + manifest.len() as u64 + held.snapshot().size();
    let request = request(change, &held);
    let kind = LaneKind::Stage {
        change: change.clone(),
    };
    let Ok(Ok(opened)) = tokio::time::timeout(change::OPENING, session.desktop.open(kind)).await
    else {
        return Staging::Unsent;
    };
    let lane = opened.lane;
    let credit = session.control.credit().clone();
    let (reading, mut writing) = tokio::io::split(opened.stream);
    let said = if still() {
        say(&mut writing, &request).await
    } else {
        Said::Unsent
    };
    drop(turn);
    let carried = match said {
        Said::Whole => tokio::select! {
            carried = tokio::time::timeout(
                stage::deadline(bytes),
                carry((reading, writing), (&manifest, held), (&credit, lane), exclusion),
            ) => carried.map_or(Staging::Lost, |carried| carried.map_or(Staging::Lost, Staging::Answered)),
            () = ticket.stopped() => Staging::Lost,
        },
        Said::Unsent => Staging::Unsent,
        Said::Perhaps => Staging::Lost,
    };
    if !matches!(carried, Staging::Answered(_)) {
        session.desktop.cancel(lane);
    }
    credit.closed(lane);
    carried
}

/// How saying a request line went.
enum Said {
    /// Written whole and flushed.
    Whole,
    /// Never written whole: never acted on.
    Unsent,
    /// Written whole, its flush failed: it may have gone.
    Perhaps,
}

/// Say `request` on a stage lane, whole with its newline last.
async fn say<W: AsyncWrite + Unpin>(writing: &mut W, request: &Value) -> Said {
    let Ok(mut said) = serde_json::to_vec(request) else {
        return Said::Unsent;
    };
    said.push(b'\n');
    if writing.write_all(&said).await.is_err() {
        return Said::Unsent;
    }
    if writing.flush().await.is_err() {
        return Said::Perhaps;
    }
    Said::Whole
}

/// Once the guest says it is ready for the stage said on a lane, send the
/// stage's bytes on `writing`, the `exclusion` let go at the guest's first
/// answer; its final reply, read on `reading`, or none when it failed
/// first.
async fn carry<R, W>(
    (reading, mut writing): (R, W),
    (manifest, held): (&[u8], Arc<Held>),
    (credit, id): (&atlas_session::credit::Credit, LaneId),
    exclusion: Ownership,
) -> Option<Value>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Unpin,
{
    let mut replies = stage::Replies::reading(reading);
    let first = replies.next().await;
    drop(exclusion);
    let first = first.ok()?;
    if !stage::ready(&first) {
        return serde_json::from_str(&first).ok();
    }
    let sending = send(&mut writing, (manifest, &held), (credit, id));
    tokio::pin!(sending);
    let line = tokio::select! {
        sent = &mut sending => {
            sent.ok()?;
            replies.next().await.ok()?
        }
        // Answered before every byte went: refused, as nothing is placed
        // before them all.
        line = replies.next() => line.ok()?,
    };
    serde_json::from_str(&line).ok()
}

/// Send the stage's bytes on `writing`, each piece spending the lane's
/// credit: the manifest's length, the manifest, then each file's contents,
/// chunk by chunk from the file the snapshot holds, each as it was read.
async fn send<W: AsyncWrite + Unpin>(
    writing: &mut W,
    (manifest, held): (&[u8], &Arc<Held>),
    (credit, id): (&atlas_session::credit::Credit, LaneId),
) -> atlas_core::error::Result<()> {
    let mut head = (manifest.len() as u64).to_be_bytes().to_vec();
    head.extend_from_slice(manifest);
    for piece in head.chunks(CHUNK) {
        credit.send(id, writing, piece).await?;
    }
    let sizes: Vec<u64> = held.files().map(|(_, size, _)| size).collect();
    for (at, size) in sizes.into_iter().enumerate() {
        for index in 0..size.div_ceil(CHUNK as u64) {
            let index = usize::try_from(index).map_err(|_| changed())?;
            let chunk = read_chunk(held, at, index, size).await?;
            credit.send(id, writing, &chunk).await?;
        }
        unchanged(held, at).await?;
    }
    writing.flush().await.map_err(|error| {
        Error::new(Code::Transport, "The stage's bytes could not be sent.").with_source(error)
    })
}

fn changed() -> Error {
    Error::new(
        Code::Io,
        "A file of the keeper's own bundle changed while it was staged; nothing was staged.",
    )
}

/// Chunk `index` of `held`'s file `at`, of `size` bytes, read off the
/// runtime as the snapshot read it.
async fn read_chunk(
    held: &Arc<Held>,
    at: usize,
    index: usize,
    size: u64,
) -> atlas_core::error::Result<Vec<u8>> {
    let held = held.clone();
    tokio::task::spawn_blocking(move || {
        held.file(at)
            .ok_or_else(changed)?
            .chunk(index, size)
            .map_err(|_| changed())
    })
    .await
    .map_err(|_| changed())?
}

/// Refuse `held`'s file `at` once it is not as it was read.
async fn unchanged(held: &Arc<Held>, at: usize) -> atlas_core::error::Result<()> {
    let held = held.clone();
    tokio::task::spawn_blocking(move || {
        held.file(at)
            .ok_or_else(changed)?
            .unchanged()
            .map_err(|_| changed())
    })
    .await
    .map_err(|_| changed())?
}
