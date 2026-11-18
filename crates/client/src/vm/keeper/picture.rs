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
//!
//! A live picture is admitted only while Atlas VM says the VM's window is
//! closed, its picture fence read then; the keeper reads its frames' bounds
//! from their headers, and reads the fence again before each header and each
//! chunk of pixels goes. Nothing sent once the window may have taken the
//! person's input is passed on: at a frame's start the keeper says why in
//! its place, part way through one it only ends the connection, and either
//! way it cancels the lane.
use super::{
    Keeper, Session,
    change::{OPENING, say_within, unsent},
    lanes::Ticket,
};
use crate::vm::{proof::Fence, runner::Window};
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

/// How long Atlas VM is given to say its picture fence, at a live
/// picture's admission and before each of its frames goes.
const FENCE_READ: Duration = Duration::from_secs(1);

/// The longest header a live picture's guest frames, and the longest side
/// of a frame it sends.
const HEADER: usize = 4096;
const SIDE: u64 = 4096;

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
    /// A live picture's frame was held back: Atlas VM's picture fence
    /// moved since the picture was admitted, or could not be read.
    Withheld,
    /// The person's maintenance holds every agent's lane.
    Held,
}

/// What a live picture's frames are held to: Atlas VM's picture fence as
/// the keeper read it when the picture was admitted, the VM's window
/// closed then. A frame goes only once a fresh reading shows the same.
struct Fenced<'a> {
    keeper: &'a Keeper,
    admitted: Fence,
}

impl<'a> Fenced<'a> {
    /// The fence `keeper` reads now, its window closed; else why no live
    /// picture is admitted.
    async fn admit(keeper: &'a Keeper) -> Result<Self, Error> {
        let admitted = closed_fence(keeper).await?;
        Ok(Self { keeper, admitted })
    }

    /// Whether a frame may go now: Atlas VM's run and picture fence as at
    /// admission, its window closed; else why it may not.
    async fn holds(&self) -> Result<(), Error> {
        if closed_fence(self.keeper).await? != self.admitted {
            return Err(took_input());
        }
        Ok(())
    }
}

/// Atlas VM's picture fence, bound to its run, as it says it now with the
/// VM's window closed; else why no picture of it goes.
async fn closed_fence(keeper: &Keeper) -> Result<Fence, Error> {
    let unread = || {
        Error::new(
            Code::Transport,
            "Atlas VM did not say whether the VM's window was opened, so its picture stopped.",
        )
    };
    let reply = keeper
        .reach
        .runner(FENCE_READ)
        .await
        .map_err(|_| unread())?;
    if !reply.ok {
        return Err(unread());
    }
    if reply.window != Some(Window::Closed) {
        return Err(took_input());
    }
    let (Some(incarnation), Some(count)) = (reply.incarnation, reply.fence) else {
        return Err(Error::new(
            Code::Update,
            "Atlas VM, which runs the VM, is from an older build that cannot say whether its \
             window was opened; update Atlas.",
        ));
    };
    Ok(Fence {
        incarnation: incarnation.id,
        count,
    })
}

/// Why a live picture stopped once the VM's window may have been opened.
fn took_input() -> Error {
    Error::new(
        Code::OnConsole,
        "The VM's window took the person's input, so its picture stopped.",
    )
}

/// Carry the picture or read `request` asks for on a picture lane of the
/// generation it names, passing back to the client on `stream` the
/// keeper's first line and then whatever the guest sends; `early` says the
/// client sent more after its request before it was told to.
pub(super) async fn pictured(
    keeper: &Keeper,
    request: &Value,
    stream: UnixStream,
    early: bool,
    ticket: &Ticket<'_>,
) {
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
    let fenced = if asked["op"] == "stream" {
        match Fenced::admit(keeper).await {
            Ok(fenced) => Some(fenced),
            Err(refused) => {
                say_within(&mut writing, &unsent(refused)).await;
                return;
            }
        }
    } else {
        None
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
        (progress, until),
        fenced.as_ref(),
        ticket.stopped(),
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
/// up or stalls, the lane fails, the generation is left, `until` passes or
/// `stop` completes, as the person's maintenance holding its lane does.
/// A live picture's frames go only while `fenced` holds.
async fn relay<R, W>(
    session: &Session,
    lane: LaneId,
    (from_guest, to_guest): (&mut R, &mut W),
    (reading, writing): (&mut OwnedReadHalf, &mut OwnedWriteHalf),
    (progress, until): (Duration, Option<tokio::time::Instant>),
    fenced: Option<&Fenced<'_>>,
    stop: impl Future<Output = ()>,
) -> Relayed
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut down = Down {
        session,
        lane,
        writing,
        progress,
    };
    let down = async {
        match fenced {
            Some(fenced) => down.frames(from_guest, fenced).await,
            None => down.bytes(from_guest).await,
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
        () = stop => Relayed::Held,
    }
}

/// The guest's side of a relay, toward its client: what it sends passed
/// on, and its credit given back for each part once the client took it.
struct Down<'a> {
    session: &'a Session,
    lane: LaneId,
    writing: &'a mut OwnedWriteHalf,
    progress: Duration,
}

impl Down<'_> {
    /// Pass on whatever the guest sends, unread, until it ends the lane.
    async fn bytes<R: AsyncRead + Unpin>(&mut self, from_guest: &mut R) -> Relayed {
        let mut chunk = vec![0_u8; CHUNK];
        loop {
            let read = match from_guest.read(&mut chunk).await {
                Ok(0) => return Relayed::Ended,
                Ok(read) => read,
                Err(_) => return Relayed::Failed,
            };
            if let Err(relayed) = self.pass(&chunk[..read]).await {
                return relayed;
            }
        }
    }

    /// Pass on a live picture's frames as the guest frames them, each
    /// header's length, its header, then the pixels it says follow, until
    /// the guest ends the lane. Each part goes only once `fenced` holds
    /// after it came: its header, and each chunk of its pixels, so nothing
    /// the guest sent once the window may have taken the person's input is
    /// passed on, whatever its header said. Held at a header, the client is
    /// told why in the frame's place; held part way through a frame, the
    /// client's connection only ends, as no word may stand among pixels.
    async fn frames<R: AsyncRead + Unpin>(
        &mut self,
        from_guest: &mut R,
        fenced: &Fenced<'_>,
    ) -> Relayed {
        let mut chunk = vec![0_u8; CHUNK];
        loop {
            let mut length = [0_u8; 4];
            match begun(from_guest, &mut length).await {
                Ok(true) => {}
                Ok(false) => return Relayed::Ended,
                Err(_) => return Relayed::Failed,
            }
            let size = u32::from_be_bytes(length) as usize;
            if size == 0 || size > HEADER {
                return Relayed::Failed;
            }
            let mut header = length.to_vec();
            header.resize(length.len() + size, 0);
            if from_guest
                .read_exact(&mut header[length.len()..])
                .await
                .is_err()
            {
                return Relayed::Failed;
            }
            let Some(mut left) = pixels(&header[length.len()..]) else {
                return Relayed::Failed;
            };
            if left > 0
                && let Err(withheld) = fenced.holds().await
            {
                let _ = tokio::time::timeout(self.progress, frame_header(self.writing, &withheld))
                    .await;
                return Relayed::Withheld;
            }
            if let Err(relayed) = self.pass(&header).await {
                return relayed;
            }
            while left > 0 {
                let want = usize::try_from(left).map_or(CHUNK, |left| left.min(CHUNK));
                if from_guest.read_exact(&mut chunk[..want]).await.is_err() {
                    return Relayed::Failed;
                }
                if fenced.holds().await.is_err() {
                    return Relayed::Withheld;
                }
                if let Err(relayed) = self.pass(&chunk[..want]).await {
                    return relayed;
                }
                left -= want as u64;
            }
        }
    }

    /// Write `part`, read from the guest, to the client within
    /// `progress`, and only then give its credit back.
    async fn pass(&mut self, part: &[u8]) -> Result<(), Relayed> {
        match tokio::time::timeout(self.progress, self.writing.write_all(part)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(Relayed::Gone),
            Err(_) => return Err(Relayed::Stalled),
        }
        // Only now may the guest send it again; a generation already
        // closed takes no credit, and its lanes end with it.
        let _ = self.session.control.say(Said::Credit {
            lane: self.lane,
            bytes: part.len() as u64,
        });
        Ok(())
    }
}

/// Fill `bytes` from `reader`: false when it ended before any of them.
async fn begun<R: AsyncRead + Unpin>(reader: &mut R, bytes: &mut [u8]) -> std::io::Result<bool> {
    let first = reader.read(bytes).await?;
    if first == 0 {
        return Ok(false);
    }
    reader.read_exact(&mut bytes[first..]).await?;
    Ok(true)
}

/// How many pixel bytes follow a live picture's frame header `header`, as
/// its guest frames it: a frame's width by its height by three, none after
/// its first word, its stop's answer or a refusal; none at all for a header
/// that is not one.
fn pixels(header: &[u8]) -> Option<u64> {
    let header: Value = serde_json::from_slice(header).ok()?;
    if header["ok"] != true || (header.get("width").is_none() && header.get("height").is_none()) {
        return Some(0);
    }
    let side = |key: &str| {
        header[key]
            .as_u64()
            .filter(|side| (1..=SIDE).contains(side))
    };
    Some(side("width")? * side("height")? * 3)
}

/// Say `refused` to a live picture's client in a frame header's place.
async fn frame_header(writing: &mut OwnedWriteHalf, refused: &Error) -> std::io::Result<()> {
    let header = serde_json::to_vec(refused)?;
    let length = u32::try_from(header.len()).map_err(std::io::Error::other)?;
    writing.write_all(&length.to_be_bytes()).await?;
    writing.write_all(&header).await?;
    writing.flush().await
}

async fn say<W: AsyncWrite + Unpin>(writing: &mut W, said: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(said)?;
    line.push(b'\n');
    writing.write_all(&line).await?;
    writing.flush().await
}

#[cfg(test)]
#[path = "picture_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "looks_tests.rs"]
mod looks_tests;

#[cfg(test)]
#[path = "live_picture_tests.rs"]
mod live_picture_tests;
