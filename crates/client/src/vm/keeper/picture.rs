//! The keeper's pictures and file reads: a command's pinned `screenshot`,
//! `frame`, `stream` or `read`, carried to the guest on a picture lane of
//! the generation that command proved, and what the guest sends passed back
//! as it comes, unread. The keeper's own first line goes before any byte of
//! the guest's: its word that the request was said, or why it was not.
//!
//! The guest paces what it sends by its bulk credit, at most two chunks on
//! their way on each lane. The keeper gives each chunk's credit back only
//! once it is written to its client, so a client that reads slowly slows
//! the guest, and the keeper holds one chunk at a time. A client that hangs
//! up, takes nothing more of what comes for [`PROGRESS`], or is still being
//! answered at the request's deadline or once the generation is left has
//! its lane cancelled and its connection closed: the guest's pump ends then
//! and takes back what it was owed.
use super::{
    Keeper, Session,
    change::{OPENING, say_within, unsent},
};
use atlas_core::error::{Code, Error};
use atlas_session::{
    compatibility::{PICTURES, judge},
    control::Said,
    credit::CHUNK,
    lane::{LaneId, LaneKind},
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};

/// The keeper's capability to carry pictures and file reads.
pub(super) const CAPABILITY: &str = "picture-v1";

/// How long a client is given to take each chunk the guest sends, a live
/// picture's included.
pub(super) const PROGRESS: Duration = Duration::from_secs(10);

/// A request's deadline when it names none, and the longest it may name.
/// A live picture has none: it ends by the guest's own idle time, the
/// client's hanging up or the generation's end.
const DEADLINE: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(120);

/// The most of what a client says on a picture lane held at once: a live
/// picture's readiness, its stop, or a frame's cancel.
const UPSTREAM: usize = 64;

/// Whether `request` is one the keeper carries on a picture lane: a
/// screenshot, a frame, a live picture or a file's read, each naming the
/// session its command proved.
pub(crate) fn carries_bulk(request: &Value) -> bool {
    matches!(
        request["op"].as_str(),
        Some("stream" | "frame" | "screenshot" | "read")
    ) && request["session"].is_object()
}

/// How a picture's relay ended.
#[derive(Debug, PartialEq, Eq)]
enum Relayed {
    /// The guest said all it had and ended the lane.
    Ended,
    /// The client hung up.
    Gone,
    /// The client took nothing more for [`PROGRESS`].
    Stalled,
    /// The lane to the guest failed.
    Failed,
    /// The generation was left.
    Retired,
    /// The request's deadline passed.
    Late,
}

/// Carry the picture or read `request` asks for on a picture lane of the
/// generation it names, passing back to the client on `stream` the
/// keeper's first line and then whatever the guest sends; `early` says the
/// client sent more after its request before it was told to.
pub(super) async fn pictured(keeper: &Keeper, request: &Value, stream: UnixStream, early: bool) {
    let (mut reading, mut writing) = stream.into_split();
    let asked = &request["request"];
    let Some(generation) = request["generation"].as_u64() else {
        let refused = Error::new(
            Code::BadRequest,
            "A picture names the generation its command proved.",
        );
        say_within(&mut writing, &unsent(refused)).await;
        return;
    };
    if !asked.is_object() || !carries_bulk(asked) {
        let refused = Error::new(
            Code::Denied,
            "The keeper carries only a screenshot, a frame, a live picture or a file's read on a \
             picture lane, each naming its session.",
        );
        say_within(&mut writing, &unsent(refused)).await;
        return;
    }
    if early {
        let refused = Error::new(
            Code::BadRequest,
            "A picture's client says nothing more until the keeper's first line; nothing was asked.",
        );
        say_within(&mut writing, &unsent(refused)).await;
        return;
    }
    let session = match live(keeper, generation) {
        Ok(session) => session,
        Err(refused) => {
            say_within(&mut writing, &unsent(refused)).await;
            return;
        }
    };
    let until = (asked["op"] != "stream").then(|| {
        let deadline = request["deadline_ms"]
            .as_u64()
            .map_or(DEADLINE, Duration::from_millis)
            .min(LONGEST);
        tokio::time::Instant::now() + deadline
    });
    let opened = match tokio::time::timeout(OPENING, session.desktop.open(LaneKind::Picture)).await
    {
        Ok(Ok(opened)) => opened,
        Ok(Err(error)) => {
            say_within(&mut writing, &unsent(error)).await;
            return;
        }
        Err(_) => {
            let late = Error::new(
                Code::Transport,
                "The picture's lane to the guest did not open in time; nothing was asked.",
            );
            say_within(&mut writing, &unsent(late)).await;
            return;
        }
    };
    let lane = opened.lane;
    let (mut from_guest, mut to_guest) = tokio::io::split(opened.stream);
    let said = tokio::time::timeout(OPENING, say(&mut to_guest, asked)).await;
    if !matches!(said, Ok(Ok(()))) {
        session.desktop.cancel(lane);
        let failed = Error::new(
            Code::Transport,
            "The picture's lane to the guest failed before its request went.",
        );
        say_within(&mut writing, &unsent(failed)).await;
        return;
    }
    let first = tokio::time::timeout(
        super::change::SAYING,
        say(&mut writing, &json!({"ok": true, "op": "picture"})),
    )
    .await;
    if !matches!(first, Ok(Ok(()))) {
        session.desktop.cancel(lane);
        return;
    }
    let progress = keeper.progress();
    let relayed = relay(
        &session,
        lane,
        (&mut from_guest, &mut to_guest),
        (&mut reading, &mut writing),
        progress,
        until,
    )
    .await;
    // Whatever the guest has not said yet is no longer wanted: its pump
    // ends with the lane, and takes back its credit.
    if relayed != Relayed::Ended {
        session.desktop.cancel(lane);
    }
}

/// The live generation, when it is `generation` and its guest takes
/// pictures; else why nothing is asked.
fn live(keeper: &Keeper, generation: u64) -> Result<std::sync::Arc<Session>, Error> {
    let Some(session) = keeper.shared.live() else {
        let why = keeper.shared.state().why.clone();
        return Err(Error::new(
            Code::Transport,
            why.unwrap_or_else(|| "The VM's session is not live.".to_owned()),
        ));
    };
    if session.control.generation().0 != generation {
        return Err(Error::new(
            Code::SessionChanged,
            "The VM's session this picture was proved in has ended; nothing was asked.",
        ));
    }
    if judge(session.control.declared(), &[PICTURES])
        .refusal()
        .is_some()
    {
        return Err(Error::new(
            Code::Update,
            "Atlas in the VM gives no pictures or files to this Mac; update it.",
        ));
    }
    Ok(session)
}

/// Pass what the guest sends on lane `lane` of `session` to the client, one
/// chunk at a time, each chunk's credit given back once the client took
/// it whole, each within `progress`; and what the client says to the guest,
/// a few bytes at a time. Until the guest ends the lane, the client hangs
/// up or stalls, the lane fails, the generation is left or `until` passes.
async fn relay<R, W>(
    session: &Session,
    lane: LaneId,
    (from_guest, to_guest): (&mut R, &mut W),
    (reading, writing): (&mut OwnedReadHalf, &mut OwnedWriteHalf),
    progress: Duration,
    until: Option<tokio::time::Instant>,
) -> Relayed
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let down = async {
        let mut chunk = vec![0_u8; CHUNK];
        loop {
            let read = match from_guest.read(&mut chunk).await {
                Ok(0) => return Relayed::Ended,
                Ok(read) => read,
                Err(_) => return Relayed::Failed,
            };
            match tokio::time::timeout(progress, writing.write_all(&chunk[..read])).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => return Relayed::Gone,
                Err(_) => return Relayed::Stalled,
            }
            // Only now may the guest send it again; a generation already
            // closed takes no credit, and its lanes end with it.
            let _ = session.control.say(Said::Credit {
                lane,
                bytes: read as u64,
            });
        }
    };
    let up = async {
        let mut said = [0_u8; UPSTREAM];
        loop {
            let read = match reading.read(&mut said).await {
                Ok(0) | Err(_) => return Relayed::Gone,
                Ok(read) => read,
            };
            if to_guest.write_all(&said[..read]).await.is_err() || to_guest.flush().await.is_err() {
                return Relayed::Failed;
            }
        }
    };
    let late = async {
        match until {
            Some(until) => tokio::time::sleep_until(until).await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        relayed = down => relayed,
        relayed = up => relayed,
        () = session.ended() => Relayed::Retired,
        () = late => Relayed::Late,
    }
}

async fn say<W: AsyncWrite + Unpin>(writing: &mut W, said: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(said)?;
    line.push(b'\n');
    writing.write_all(&line).await?;
    writing.flush().await
}

#[cfg(test)]
#[path = "picture_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "looks_tests.rs"]
mod looks_tests;
