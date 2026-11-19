//! The keeper's changes: each one a command carries, said on a lane of its
//! own in the generation that command proved, its reply lines passed on as
//! they come; and the settlement of the changes the station holds no claim
//! on any more, so the guest forgets what they came to.
//!
//! A change is refused unsaid, `"sent": false`, while nothing of it can have
//! reached the guest: no live generation, another one than its command
//! proved, a guest that keeps no change records, or a lane that did not
//! open. A client's change that asks the guest to update its Atlas is
//! refused so before anything else: only the keeper itself asks that. Once its request may have reached the guest, a failure says
//! `"sent": true, "uncertain": true`, and the guest's own record of it is
//! what tells. A client that hangs up before the final reply has its
//! change's lane cancelled over control.
use super::{Keeper, Session, final_reply, lanes::Ticket, reply_line};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::{
    compatibility::CHANGES,
    control::{NAMED, Said},
    lane::LaneKind,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering::SeqCst},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::UnixStream,
};

/// The keeper's capability to carry changes and settle them.
pub(super) const CAPABILITY: &str = "change-v1";

/// A change's deadline when it names none, and the longest it may name: a
/// command's ten minutes, the time its desktop has to end what it left,
/// and room for its lane.
const DEADLINE: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(15 * 60);

/// How long a lane has to open.
pub(super) const OPENING: Duration = Duration::from_secs(15);

/// How long a client is given to take the keeper's own word.
pub(super) const SAYING: Duration = Duration::from_secs(2);

/// Set in a build made for live checks of a lost reply: the keeper keeps
/// back the final reply of each change whose command names this word, or
/// whose request is this operation (such as `environment`), as if its lane
/// failed, and says so; the change then stays pending until the person's
/// Recover ([`KeptBack`]). Empty, it keeps back every change's.
/// `scripts/release.sh` refuses to release one.
const DROPS_REPLY: Option<&str> = option_env!("ATLAS_TEST_DROP_REPLY");

/// The words a test keeps back the final reply of, as a build made with
/// [`DROPS_REPLY`] does its own.
#[cfg(test)]
pub(super) static TEST_DROPS_REPLY: std::sync::Mutex<Vec<String>> =
    std::sync::Mutex::new(Vec::new());

/// The changes whose final reply this keeper kept back, as [`DROPS_REPLY`]
/// asks: no client's read of what one came to is carried to the guest, and
/// only a recovery asked of the keeper settles one, so each stays pending
/// until the person's Recover, as a reply truly lost with its guest away
/// would. Always empty in a build made without it.
#[derive(Debug, Default)]
pub(super) struct KeptBack(std::sync::Mutex<BTreeSet<Token>>);

impl KeptBack {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<Token>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn keep(&self, change: Token) {
        self.lock().insert(change);
    }

    /// Whether the change `change` is one whose final reply was kept back.
    pub(super) fn holds(&self, change: &str) -> bool {
        Token::parse(change).is_some_and(|change| self.lock().contains(&change))
    }

    /// Forget the change `change`, its claim cleared.
    pub(super) fn forget(&self, change: &Token) {
        self.lock().remove(change);
    }
}

/// The changes carried in one generation, and those its hello named that
/// were still claimed, settled once the station holds no claim on them.
#[derive(Debug, Default)]
pub(super) struct Carried(std::sync::Mutex<BTreeSet<Token>>);

impl Carried {
    /// The changes `changes` to settle once unclaimed.
    pub(super) fn of(changes: impl IntoIterator<Item = Token>) -> Self {
        Self(std::sync::Mutex::new(changes.into_iter().collect()))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<Token>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn keep(&self, change: Token) {
        self.lock().insert(change);
    }

    pub(super) fn all(&self) -> Vec<Token> {
        self.lock().iter().cloned().collect()
    }

    fn forget(&self, settled: &[Token]) {
        let mut carried = self.lock();
        for change in settled {
            carried.remove(change);
        }
    }
}

/// What a client's request asks be carried.
struct Asked {
    change: Token,
    generation: u64,
    kind: LaneKind,
    request: Value,
}

impl Asked {
    /// The change `request` asks for: its lane, its change named by its
    /// request too, and the generation it was proved in; refused when it
    /// asks the guest to update its Atlas, which only the keeper itself
    /// asks, or names none of these.
    fn of(request: &Value) -> std::result::Result<Self, Error> {
        if request["request"]["op"] == "update" {
            return Err(Error::new(
                Code::Denied,
                "Only Atlas's own update changes Atlas in the VM; nothing was sent.",
            ));
        }
        Self::named(request).ok_or_else(|| {
            Error::new(
                Code::BadRequest,
                "A change names its lane, its change, the generation it was proved in, and a \
                 request naming that change.",
            )
        })
    }

    fn named(request: &Value) -> Option<Self> {
        let change = Token::parse(request["change"].as_str()?)?;
        let generation = request["generation"].as_u64()?;
        let asked = request["request"].clone();
        if !asked.is_object() || asked["change"] != json!(change) {
            return None;
        }
        let kind = match request["lane"].as_str()? {
            "change" => LaneKind::Change {
                change: change.clone(),
            },
            "input" => LaneKind::Input {
                change: Some(change.clone()),
            },
            _ => return None,
        };
        Some(Self {
            change,
            generation,
            kind,
            request: asked,
        })
    }

    /// The owner an acquire on an input lane takes input for, when it names
    /// one by a token.
    fn owner(&self) -> Option<Token> {
        let acquire =
            matches!(self.kind, LaneKind::Input { .. }) && self.request["op"] == "acquire";
        acquire
            .then(|| self.request["owner"].as_str().and_then(Token::parse))
            .flatten()
    }
}

/// How a change's carrying ended.
enum Carrying {
    /// Its request never went whole: the guest cannot have acted on it.
    Unsent(Error),
    /// Its final reply was passed on.
    Answered,
    /// It may have reached the guest, and its final reply did not come.
    Lost(Error),
    /// Its final reply came and was kept back, as a test build asks.
    KeptBack(Error),
    /// The client went before its final reply.
    Gone,
}

/// Carry the change `request` asks for on its lane, answering the client on
/// `stream` with each reply line as it comes, or with why it could not be.
pub(super) async fn changed(
    keeper: &Keeper,
    request: &Value,
    stream: UnixStream,
    ticket: &Ticket<'_>,
) {
    let deadline = request["deadline_ms"]
        .as_u64()
        .map_or(DEADLINE, Duration::from_millis)
        .min(LONGEST);
    let (mut reading, mut writing) = stream.into_split();
    let asked = match Asked::of(request) {
        Ok(asked) => asked,
        Err(refused) => {
            say_within(&mut writing, &unsent(refused)).await;
            return;
        }
    };
    let session = match live_in(keeper, asked.generation) {
        Ok(session) => session,
        Err(refused) => {
            say_within(&mut writing, &unsent(refused)).await;
            return;
        }
    };
    // Remembered before anything is said, so it is settled once the
    // station holds no claim on it, whatever comes of its lane; so is the
    // owner input is taken for, which the guest keeps as a change of its
    // own.
    session.carried.keep(asked.change.clone());
    if let Some(owner) = asked.owner() {
        session.carried.keep(owner);
    }
    let until = tokio::time::Instant::now() + deadline;
    let opened = tokio::time::timeout(OPENING.min(deadline), session.desktop.open(asked.kind))
        .await
        .map_err(|_| {
            Error::new(
                Code::Transport,
                "The change's lane to the guest did not open in time; nothing was sent.",
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
    let dropping = drops_reply(&asked.request);
    // Whether the client's stream stands between two lines: a failure is
    // said only there, never after a line cut short.
    let between = AtomicBool::new(true);
    let carrying = async {
        match tokio::time::timeout_at(
            until,
            relay(
                opened.stream,
                &asked.request,
                &mut writing,
                dropping,
                &between,
            ),
        )
        .await
        {
            Ok(carried) => carried,
            Err(_) => Carrying::Lost(Error::new(
                Code::Transport,
                "The guest did not answer the change in time.",
            )),
        }
    };
    // The client says nothing after its request: anything more, or its
    // end, is its hanging up.
    let mut byte = [0; 1];
    let carried = tokio::select! {
        carried = carrying => carried,
        _ = reading.read(&mut byte) => Carrying::Gone,
        // The person's maintenance holds it: its lane is cancelled, and what
        // it came to is the guest's record's to tell.
        () = ticket.stopped() => Carrying::Lost(super::held::held_now(keeper)),
    };
    match carried {
        Carrying::Answered => {}
        Carrying::Unsent(error) => {
            session.desktop.cancel(lane);
            say_within(&mut writing, &unsent(error)).await;
        }
        // A line cut short is ended by the connection's end alone: the
        // client hears its reply lost, and asks what the change came to.
        Carrying::Lost(error) => {
            session.desktop.cancel(lane);
            if between.load(SeqCst) {
                say_within(&mut writing, &lost(error)).await;
            }
        }
        Carrying::KeptBack(error) => {
            keeper.kept_back.keep(asked.change.clone());
            session.desktop.cancel(lane);
            say_within(&mut writing, &lost(error)).await;
        }
        Carrying::Gone => session.desktop.cancel(lane),
    }
}

/// The live generation, when it is `generation`; else why nothing is said.
pub(super) fn live_in(keeper: &Keeper, generation: u64) -> Result<std::sync::Arc<Session>> {
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
            "The VM's session this change was proved in has ended; nothing was sent.",
        ));
    }
    if let Some(refused) = keeper.unserved(&session, &[CHANGES], |name| {
        format!("{name}'s Atlas keeps no record of changes, so nothing was sent.")
    }) {
        return Err(refused);
    }
    Ok(session)
}

/// Say `request` on the lane `lane` and pass each reply line on to
/// `client` until the final one: kept back instead, as if it never came,
/// when `dropping`.
async fn relay<S, W>(
    lane: S,
    request: &Value,
    client: &mut W,
    dropping: bool,
    between: &AtomicBool,
) -> Carrying
where
    S: AsyncRead + AsyncWrite + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lane = lane;
    let Ok(mut said) = serde_json::to_vec(request) else {
        return Carrying::Unsent(Error::new(Code::BadRequest, "The change cannot be said."));
    };
    said.push(b'\n');
    // Its newline is last: a request not written whole cannot have been
    // acted on. Once written, a flush that fails may have sent it.
    if let Err(error) = lane.write_all(&said).await {
        return Carrying::Unsent(lane_failed(&error));
    }
    if let Err(error) = lane.flush().await {
        return Carrying::Lost(lane_failed(&error));
    }
    // Each line is bounded on its own: a command says much in many events.
    let mut reader = BufReader::new(lane);
    loop {
        let mut reply = match reply_line(&mut reader).await {
            Ok(Some(reply)) => reply,
            Ok(None) => {
                return Carrying::Lost(Error::new(
                    Code::Transport,
                    "The change's lane to the guest ended before its final reply.",
                ));
            }
            Err(error) => return Carrying::Lost(lane_failed(&error)),
        };
        let last = final_reply(reply.trim_end());
        if last && dropping {
            return Carrying::KeptBack(Error::new(
                Code::Transport,
                "This test build kept back the change's final reply.",
            ));
        }
        reply.push('\n');
        between.store(false, SeqCst);
        if client.write_all(reply.as_bytes()).await.is_err() || client.flush().await.is_err() {
            return Carrying::Gone;
        }
        between.store(true, SeqCst);
        if last {
            return Carrying::Answered;
        }
    }
}

/// Whether this build keeps back the final reply of `request`.
fn drops_reply(request: &Value) -> bool {
    #[cfg(test)]
    if TEST_DROPS_REPLY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|word| names(request, word))
    {
        return true;
    }
    DROPS_REPLY.is_some_and(|word| names(request, word))
}

/// Whether `request` names `word`, as its operation or one of its
/// command's arguments; every request names the empty word.
fn names(request: &Value, word: &str) -> bool {
    word.is_empty()
        || request["op"] == word
        || request["argv"]
            .as_array()
            .is_some_and(|argv| argv.iter().any(|argument| argument == word))
}

/// Settle on the live generation the changes `request` names that the
/// station holds no claim on, its marker read again: what was settled, or
/// why nothing was.
pub(super) async fn settled(keeper: &Keeper, request: &Value) -> Value {
    let named: Option<Vec<Token>> = request["changes"].as_array().and_then(|changes| {
        changes
            .iter()
            .map(|change| change.as_str().and_then(Token::parse))
            .collect()
    });
    let Some(named) = named.filter(|named| !named.is_empty() && named.len() <= NAMED) else {
        return refusal(
            Code::BadRequest,
            "A settlement names from 1 to 1024 changes.",
        );
    };
    let Some(session) = keeper.shared.live() else {
        return refusal(Code::Transport, "The VM's session is not live.");
    };
    let Some(claimed) = keeper.claimed().await else {
        return refusal(
            Code::Busy,
            "The station's pending changes could not be read; nothing was settled.",
        );
    };
    let settling: Vec<Token> = named
        .into_iter()
        .filter(|change| !claimed.iter().any(|claim| claim == change.as_str()))
        .collect();
    if !settling.is_empty()
        && let Err(error) = session.control.say(Said::Settled {
            changes: settling.clone(),
        })
    {
        return serde_json::to_value(&error).unwrap_or_default();
    }
    session.carried.forget(&settling);
    json!({"ok": true, "op": "settle", "settled": settling})
}

impl Keeper {
    /// The changes the station holds a claim on now, its marker read
    /// without waiting for its lock; none when it is busy or cannot be
    /// read, which claims every change.
    pub(super) async fn claimed(&self) -> Option<Vec<String>> {
        let stop = self.stop.clone();
        super::blocking(move || stop.pending_within(Duration::ZERO))
            .await
            .ok()
    }

    /// Settle the changes carried in `session` that the station holds no
    /// claim on any more: a marker that cannot be read now waits for the
    /// next time.
    pub(super) async fn settling(&self, session: &Session) {
        let carried = session.carried.all();
        if carried.is_empty() {
            return;
        }
        let Some(claimed) = self.claimed().await else {
            return;
        };
        let settling: Vec<Token> = carried
            .into_iter()
            .filter(|change| !claimed.iter().any(|claim| claim == change.as_str()))
            .take(NAMED)
            .collect();
        if !settling.is_empty()
            && session
                .control
                .say(Said::Settled {
                    changes: settling.clone(),
                })
                .is_ok()
        {
            session.carried.forget(&settling);
        }
    }
}

/// `error`, said before anything of the change reached the guest.
pub(super) fn unsent(error: Error) -> Value {
    serde_json::to_value(error.certain().with("sent", json!(false))).unwrap_or_default()
}

/// `error`, said once the change may have reached the guest.
pub(super) fn lost(error: Error) -> Value {
    serde_json::to_value(error.uncertain().with("sent", json!(true))).unwrap_or_default()
}

fn refusal(code: Code, why: &str) -> Value {
    serde_json::to_value(Error::new(code, why)).unwrap_or_default()
}

pub(super) fn lane_failed(error: &std::io::Error) -> Error {
    Error::new(
        Code::Transport,
        format!("The change's lane to the guest failed: {error}."),
    )
}

/// Say `said` to the client, given up after [`SAYING`]: one that stopped
/// reading never holds the keeper.
pub(super) async fn say_within<W: AsyncWrite + Unpin>(writing: &mut W, said: &Value) {
    let _ = tokio::time::timeout(SAYING, say(writing, said)).await;
}

async fn say<W: AsyncWrite + Unpin>(writing: &mut W, said: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(said)?;
    line.push(b'\n');
    writing.write_all(&line).await?;
    writing.flush().await
}

#[cfg(test)]
#[path = "change_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod jobs_tests;

#[cfg(test)]
#[path = "input_tests.rs"]
mod input_tests;

#[cfg(test)]
#[path = "targeted_tests.rs"]
mod targeted_tests;
