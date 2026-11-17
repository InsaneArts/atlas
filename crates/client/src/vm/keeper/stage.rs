//! The keeper's stages: a file or folder a command snapshotted, carried to
//! the guest on a stage lane of the generation that command proved. The
//! request is said first; once the guest says it is ready, exactly the
//! stage's bytes (the manifest's length, the manifest, then every file's
//! contents) are relayed from the client in chunks, each sent only as the
//! guest's credit allows, so the keeper never holds more than two chunks of
//! it: the one being sent and the next. The guest's final reply is passed
//! on.
//!
//! Nothing here reads a path: the client took the snapshot and sends its
//! bytes. A client that hangs up before the final reply, or sends more than
//! the stage holds, has the lane cancelled at once, while it waits for the
//! guest's credit too, and the guest places nothing.
use super::{
    Keeper,
    change::{OPENING, lane_failed, live_in, lost, say_within, unsent},
    final_reply, reply_line,
};
use atlas_core::{
    error::{Code, Error},
    files::Token,
    stage::{MOST_CONTENTS, MOST_MANIFEST},
};
use atlas_session::{
    compatibility::{STAGE, judge},
    credit::Credit,
    lane::{LaneId, LaneKind},
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering::SeqCst},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::mpsc,
};

/// The keeper's capability to carry stages.
pub(super) const CAPABILITY: &str = "stage-v1";

/// The most of a stage relayed at once.
const CHUNK: usize = 64 * 1024;

/// The least rate a guest takes a stage's bytes at, after its grace, and
/// how long it may then take to put it in place: a stage's deadline
/// follows them, with room for the lane.
const RATE: u64 = 256 * 1024;
const GRACE: Duration = Duration::from_secs(30);
const SETTLING: Duration = Duration::from_secs(120);
const ROOM: Duration = Duration::from_secs(10);

/// How long a stage of `bytes` (its manifest and contents) may take from
/// its request to its final reply.
pub(crate) fn deadline(bytes: u64) -> Duration {
    GRACE + Duration::from_secs(bytes / RATE) + SETTLING + ROOM
}

/// What a client's request asks be staged.
struct Asked {
    change: Token,
    generation: u64,
    request: Value,
    /// The bytes that follow once the guest is ready: the manifest's length,
    /// the manifest, and every file's contents.
    bytes: u64,
}

impl Asked {
    /// The stage `request` asks for: its change, named by its stage too,
    /// the generation it was proved in, and how many bytes follow, within
    /// the bounds a guest takes.
    fn of(request: &Value) -> Option<Self> {
        let change = Token::parse(request["change"].as_str()?)?;
        let generation = request["generation"].as_u64()?;
        let asked = request["request"].clone();
        if asked["op"] != "stage" || asked["change"] != json!(change) {
            return None;
        }
        let manifest = asked["manifest"]
            .as_u64()
            .filter(|manifest| *manifest <= MOST_MANIFEST as u64)?;
        let size = asked["size"]
            .as_u64()
            .filter(|size| *size <= MOST_CONTENTS)?;
        Some(Self {
            change,
            generation,
            request: asked,
            bytes: 8 + manifest + size,
        })
    }
}

/// How a stage's carrying ended.
enum Carrying {
    /// Its request never went whole: the guest cannot have acted on it.
    Unsent(Error),
    /// Its final reply was passed on.
    Answered,
    /// It may have reached the guest, and its final reply did not come.
    Lost(Error),
    /// The client went, or sent what the stage does not hold, before its
    /// final reply.
    Gone,
}

/// Carry the stage `request` asks for on its lane, relaying its bytes from
/// the client on `stream` once the guest is ready, and answering it with
/// each reply line, or with why it could not be. `early` says the client
/// sent more after its request before it was told to.
pub(super) async fn staged(keeper: &Keeper, request: &Value, stream: UnixStream, early: bool) {
    let (mut reading, mut writing) = stream.into_split();
    let Some(asked) = Asked::of(request) else {
        let refused = unsent(Error::new(
            Code::BadRequest,
            "A stage names its change, the generation it was proved in, and a stage request \
             naming that change within a guest's bounds.",
        ));
        say_within(&mut writing, &refused).await;
        return;
    };
    if early {
        let refused = unsent(Error::new(
            Code::BadRequest,
            "A stage's bytes follow only once the guest is ready for them; nothing was sent.",
        ));
        say_within(&mut writing, &refused).await;
        return;
    }
    let session = match live_in(keeper, asked.generation) {
        Ok(session) => session,
        Err(refused) => {
            say_within(&mut writing, &unsent(refused)).await;
            return;
        }
    };
    if judge(session.control.declared(), &[STAGE])
        .refusal()
        .is_some()
    {
        let refused = Error::new(Code::Update, "Atlas in the VM takes no stages; update it.");
        say_within(&mut writing, &unsent(refused)).await;
        return;
    }
    let deadline = request["deadline_ms"]
        .as_u64()
        .map_or(Duration::MAX, Duration::from_millis)
        .min(self::deadline(asked.bytes));
    // Remembered before anything is said, so it is settled once the
    // station holds no claim on it, whatever comes of its lane.
    session.carried.keep(asked.change.clone());
    let until = tokio::time::Instant::now() + deadline;
    let kind = LaneKind::Stage {
        change: asked.change.clone(),
    };
    let opened = tokio::time::timeout(OPENING.min(deadline), session.desktop.open(kind))
        .await
        .map_err(|_| {
            Error::new(
                Code::Transport,
                "The stage's lane to the guest did not open in time; nothing was sent.",
            )
        })
        .and_then(|opened| opened);
    let opened = match opened {
        Ok(opened) => opened,
        Err(error) => {
            say_within(&mut writing, &unsent(error)).await;
            return;
        }
    };
    let lane = opened.lane;
    let credit = session.control.credit().clone();
    let between = AtomicBool::new(true);
    let carried = tokio::time::timeout_at(
        until,
        relay(
            &asked,
            opened.stream,
            (&mut reading, &mut writing),
            (&credit, lane),
            &between,
        ),
    )
    .await
    .unwrap_or_else(|_| {
        Carrying::Lost(Error::new(
            Code::Transport,
            "The guest did not answer the stage in time.",
        ))
    });
    match carried {
        Carrying::Answered => {}
        Carrying::Unsent(error) => {
            session.desktop.cancel(lane);
            say_within(&mut writing, &unsent(error)).await;
        }
        Carrying::Lost(error) => {
            session.desktop.cancel(lane);
            if between.load(SeqCst) {
                say_within(&mut writing, &lost(error)).await;
            }
        }
        Carrying::Gone => session.desktop.cancel(lane),
    }
    // What the lane still owes is never read once it is done.
    credit.closed(lane);
}

/// Say the stage's request on `lane`, and once the guest is ready relay its
/// bytes from `client`, each chunk spending `credit` of the lane; pass every
/// reply line on to the client until the final one.
async fn relay<S>(
    asked: &Asked,
    lane: S,
    client: (&mut OwnedReadHalf, &mut OwnedWriteHalf),
    (credit, id): (&Credit, LaneId),
    between: &AtomicBool,
) -> Carrying
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reading, writing) = client;
    let (lane_reading, mut lane_writing) = tokio::io::split(lane);
    let Ok(mut said) = serde_json::to_vec(&asked.request) else {
        return Carrying::Unsent(Error::new(Code::BadRequest, "The stage cannot be said."));
    };
    said.push(b'\n');
    // Its newline is last: a request not written whole cannot have been
    // acted on. Once written, a flush that fails may have sent it.
    if let Err(error) = lane_writing.write_all(&said).await {
        return Carrying::Unsent(lane_failed(&error));
    }
    if let Err(error) = lane_writing.flush().await {
        return Carrying::Lost(lane_failed(&error));
    }
    // Read apart, so a reply is never cut short by bytes still being sent.
    let mut replies = Replies::reading(lane_reading);
    let mut byte = [0_u8; 1];
    let first = tokio::select! {
        line = replies.next() => line,
        _ = reading.read(&mut byte) => return Carrying::Gone,
    };
    let first = match first {
        Ok(line) => line,
        Err(lost) => return Carrying::Lost(lost),
    };
    if !ready(&first) {
        // Answered at once: refused, or answered from what it came to.
        return passed_on(first, &mut replies, reading, writing, between).await;
    }
    if let Some(gone) = pass(&first, writing, between).await {
        return gone;
    }
    // Two chunks at most: the one being sent, and the next read meanwhile,
    // so the client is watched while the guest's credit is awaited.
    let (mut sending, mut ahead) = (vec![0_u8; CHUNK], vec![0_u8; CHUNK]);
    let (mut held, mut read_ahead) = (0, 0);
    let mut unread = asked.bytes;
    loop {
        if held == 0 {
            if read_ahead > 0 {
                std::mem::swap(&mut sending, &mut ahead);
                (held, read_ahead) = (read_ahead, 0);
            } else if unread > 0 {
                let room = usize::try_from(unread).unwrap_or(CHUNK).min(CHUNK);
                held = tokio::select! {
                    read = reading.read(&mut sending[..room]) => match read {
                        Ok(0) | Err(_) => return Carrying::Gone,
                        Ok(got) => got,
                    },
                    line = replies.next() => return early(line, &mut replies, reading, writing, between).await,
                };
                unread -= held as u64;
            } else {
                break;
            }
        }
        let sent = credit.send(id, &mut lane_writing, &sending[..held]);
        tokio::pin!(sent);
        loop {
            let room = usize::try_from(unread).unwrap_or(CHUNK).min(CHUNK);
            tokio::select! {
                sent = &mut sent => {
                    if let Err(error) = sent {
                        return Carrying::Lost(error);
                    }
                    held = 0;
                    break;
                }
                got = watched(reading, &mut ahead[..room.max(1)], read_ahead == 0) => match got {
                    Some(got) if unread > 0 => {
                        read_ahead = got;
                        unread -= got as u64;
                    }
                    _ => return Carrying::Gone,
                },
                line = replies.next() => return early(line, &mut replies, reading, writing, between).await,
            }
        }
    }
    let line = tokio::select! {
        line = replies.next() => line,
        // The client says nothing more: anything, or its end, is its
        // hanging up.
        _ = reading.read(&mut byte) => return Carrying::Gone,
    };
    match line {
        Ok(line) => passed_on(line, &mut replies, reading, writing, between).await,
        Err(lost) => Carrying::Lost(lost),
    }
}

/// How often a client whose bytes wait unread is looked at again for its
/// end.
const WATCH: Duration = Duration::from_millis(50);

/// What the client says while a chunk waits for the guest's credit: with
/// `reading_ahead`, the next bytes read into `ahead` (how many); else none,
/// once its end is closed, as with a read that ends or fails. With the next
/// chunk in hand already, nothing more is read, and only its end is watched
/// for. Read ahead only within what the stage holds: once it is all in,
/// anything more is its hanging up too.
async fn watched(
    reading: &mut OwnedReadHalf,
    ahead: &mut [u8],
    reading_ahead: bool,
) -> Option<usize> {
    if !reading_ahead {
        hung_up(reading).await;
        return None;
    }
    match reading.read(ahead).await {
        Ok(got) if got > 0 => Some(got),
        Ok(_) | Err(_) => None,
    }
}

/// Completes once the client's end of `reading` is closed, whatever it sent
/// before that is still unread.
async fn hung_up(reading: &OwnedReadHalf) {
    loop {
        match reading.ready(tokio::io::Interest::READABLE).await {
            Ok(ready) if !ready.is_read_closed() => tokio::time::sleep(WATCH).await,
            Ok(_) | Err(_) => return,
        }
    }
}

/// The guest's `line`, said before all the stage's bytes came: passed on
/// with what follows it, or why it did not come.
async fn early(
    line: Result<String, Error>,
    replies: &mut Replies,
    reading: &mut OwnedReadHalf,
    writing: &mut OwnedWriteHalf,
    between: &AtomicBool,
) -> Carrying {
    match line {
        Ok(line) => passed_on(line, replies, reading, writing, between).await,
        Err(lost) => Carrying::Lost(lost),
    }
}

/// Whether `line` is the guest's word that it is ready for a stage's bytes.
fn ready(line: &str) -> bool {
    serde_json::from_str::<Value>(line).ok() == Some(json!({"ready": true}))
}

/// Pass `line` on, then every line after it until the final one, while the
/// client stays.
async fn passed_on(
    mut line: String,
    replies: &mut Replies,
    reading: &mut OwnedReadHalf,
    writing: &mut OwnedWriteHalf,
    between: &AtomicBool,
) -> Carrying {
    let mut byte = [0_u8; 1];
    loop {
        if let Some(gone) = pass(&line, writing, between).await {
            return gone;
        }
        if final_reply(&line) {
            return Carrying::Answered;
        }
        line = tokio::select! {
            next = replies.next() => match next {
                Ok(next) => next,
                Err(lost) => return Carrying::Lost(lost),
            },
            _ = reading.read(&mut byte) => return Carrying::Gone,
        };
    }
}

/// Say one reply line to the client: why it is gone, if it is.
async fn pass(line: &str, writing: &mut OwnedWriteHalf, between: &AtomicBool) -> Option<Carrying> {
    between.store(false, SeqCst);
    let mut said = line.as_bytes().to_vec();
    said.push(b'\n');
    if writing.write_all(&said).await.is_err() || writing.flush().await.is_err() {
        return Some(Carrying::Gone);
    }
    between.store(true, SeqCst);
    None
}

/// A stage lane's reply lines, read by a task of their own and ended with
/// it: each whole and bounded, as every reply line is.
struct Replies {
    lines: mpsc::Receiver<Result<String, Error>>,
    reading: tokio::task::JoinHandle<()>,
}

impl Replies {
    fn reading<R: AsyncRead + Send + Unpin + 'static>(lane: R) -> Self {
        let (said, lines) = mpsc::channel(2);
        let reading = tokio::spawn(async move {
            let mut lane = BufReader::new(lane);
            loop {
                let line = match reply_line(&mut lane).await {
                    Ok(Some(line)) => Ok(line),
                    Ok(None) => Err(Error::new(
                        Code::Transport,
                        "The stage's lane to the guest ended before its final reply.",
                    )),
                    Err(error) => Err(lane_failed(&error)),
                };
                // The guest's word that it is ready is the one reply line a
                // stage says before its final one.
                let last = line
                    .as_ref()
                    .map_or(true, |line| final_reply(line) && !ready(line));
                if said.send(line).await.is_err() || last {
                    return;
                }
            }
        });
        Self { lines, reading }
    }

    /// The next line, or why none comes.
    async fn next(&mut self) -> Result<String, Error> {
        self.lines.recv().await.unwrap_or_else(|| {
            Err(Error::new(
                Code::Transport,
                "The stage's lane to the guest ended before its final reply.",
            ))
        })
    }
}

impl Drop for Replies {
    fn drop(&mut self) {
        self.reading.abort();
    }
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "copies_tests.rs"]
mod copies_tests;
