//! The keeper's clients: Atlas's own processes, on `keeper.sock` in the VM's
//! private station folder. One request per connection, one JSON line each
//! way (a read's reply lines as they come, and a picture's bytes after the
//! keeper's own first line), and the client admitted on that very request:
//! this account's, its running code signed as Atlas.
//!
//! Resume is the person's alone: it is taken only with the capability the
//! keeper made at its start and keeps in its station folder, which the
//! person's own surface reads there. What a client says of itself, that it
//! is the viewer included, counts for nothing.
//!
//! An update asks `handover`, with its change: the keeper keeps the station's
//! Stop for the update, says it, leaves its generation, answers and ends,
//! all under its turn, so no Resume comes between.
//! `update-guest` asks the keeper to bring its guest to the keeper's own
//! build ([`super::update`]).
//!
//! The person's maintenance of the VM holds every agent's request here,
//! read afresh from the VM's lifecycle record at each one ([`super::held`]);
//! every request that carries an agent's work to the guest holds a ticket
//! for its lane ([`super::lanes`]), so a hold can cancel it.
use super::{
    Keeper, held,
    lanes::{Ticket, Work},
    read_until,
};
use atlas_control::{Resumed, StopRevision, StopWhy};
use atlas_core::{
    app_bundle::signature::{APP_IDENTIFIER, requirement_for},
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::{
    control::Outcome, keys::PairingSecret, lane::LaneKind, station::control::Answer,
};
use serde_json::{Value, json};
use std::{
    os::{
        fd::{AsFd, BorrowedFd},
        unix::fs::PermissionsExt,
    },
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::mpsc,
};

/// The keeper's protocol, which a client checks in its hello.
pub(crate) const PROTOCOL: u32 = 1;
/// What the keeper serves beside its hello.
pub(crate) const CAPABILITIES: [&str; 11] = [
    "read-v1",
    "stop-v1",
    "resume-v1",
    "handover-v1",
    super::observe::CAPABILITY,
    super::change::CAPABILITY,
    super::recover::CAPABILITY,
    super::stage::CAPABILITY,
    super::picture::CAPABILITY,
    super::held::CAPABILITY,
    super::update::CAPABILITY,
];
/// The Resume capability's name in the station folder.
pub(crate) const RESUME_KEY: &str = "resume.key";
/// The longest request.
const REQUEST: u64 = 1 << 20;
/// How long a client has to say its request.
const ASKING: Duration = Duration::from_secs(10);
/// A request's deadline when it names none, and the longest it may name.
const DEADLINE: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(120);

/// Whether a client may have `request` carried to the guest as a read on a
/// lane of `kind`: what the commands this build runs on a VM need, and
/// nothing more. A Stop, a Resume, its own proof and recovery are the
/// keeper's own; a job's reads are a list or one job (`show`) and its log,
/// never a stop of every job; `leftovers` is the guest's word on what work
/// of its may still run. `open`, `close` and a cleanup read the guest's
/// apps, and a screenshot of an app or a window its windows, always in the
/// session their proof pinned, and its copies, and what one of them holds
/// now; a live picture asks whether
/// its capture stopped and drained, naming the session it was proved in;
/// doctor asks which apps its Applications folder holds, naming them, in
/// the session its proof pinned; the native check reads its canary folder's
/// files or its fixture's scan, naming the folder, in the session its proof
/// pinned. On the input service's own lane, only a
/// look at an app's elements, held to the epoch the command's proof pinned
/// and naming its app.
pub(crate) fn carries(kind: &LaneKind, request: &Value) -> bool {
    match (kind, request["op"].as_str()) {
        (LaneKind::Read, Some("hello" | "outcome" | "leftovers" | "copies")) => true,
        (LaneKind::Read, Some("copy-contents")) => request["copy"].is_string(),
        (LaneKind::Read, Some("apps" | "windows" | "stream-drain-status")) => {
            request["session"].is_object()
        }
        (LaneKind::Read, Some("installed")) => {
            request["names"].is_array() && request["session"].is_object()
        }
        (LaneKind::Read, Some("native")) => {
            matches!(
                request["action"].as_str(),
                Some("files" | "scan" | "resolve")
            ) && request["directory"].is_string()
                && request["session"].is_object()
        }
        (LaneKind::Read, Some("job")) => {
            matches!(request["action"].as_str(), Some("show" | "log"))
        }
        (LaneKind::Input { change: None }, Some("elements")) => {
            request["epoch"].is_object() && request["expect"].is_object()
        }
        _ => false,
    }
}

/// The lane a client's `read` asks for: the guest's daemon's, unless it
/// names the input service's; none for any other.
fn read_lane(request: &Value) -> Option<LaneKind> {
    match request["lane"].as_str() {
        None | Some("read") => Some(LaneKind::Read),
        Some("input") => Some(LaneKind::Input { change: None }),
        Some(_) => None,
    }
}

/// The keeper's Resume capability: a random token made at its start.
pub(super) struct Grant(String);

impl Grant {
    pub(super) const fn new(token: String) -> Self {
        Self(token)
    }

    /// Whether `given` is the capability, compared whole in a time that
    /// does not tell how much of it matched.
    fn admits(&self, given: Option<&str>) -> bool {
        let Some(given) = given else {
            return false;
        };
        let (given, own) = (given.as_bytes(), self.0.as_bytes());
        given.len() == own.len()
            && given
                .iter()
                .zip(own)
                .fold(0_u8, |differ, (a, b)| differ | (a ^ b))
                == 0
    }
}

/// Make the keeper's Resume capability and keep it, 0600, in its station
/// `folder`, in place of one an earlier keeper kept.
///
/// # Errors
///
/// Fails when no random token can be made or it cannot be kept.
pub(super) fn capability(folder: &Path) -> Result<String> {
    let token = atlas_core::name::hex(PairingSecret::generate()?.as_bytes());
    atlas_core::files::write_atomic(&folder.join(RESUME_KEY), token.as_bytes(), 0o600)?;
    Ok(token)
}

/// Listen at `path`, 0600. What is left there by a keeper that ended is
/// replaced: this keeper holds the VM's lock, so none other serves it.
///
/// # Errors
///
/// Refuses anything there that is not a socket; fails when it cannot bind.
pub(super) fn bind(path: &Path) -> Result<UnixListener> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(path).map_err(|error| Error::writing(path, error))?;
        }
        Ok(_) => {
            return Err(Error::new(
                Code::State,
                format!("{} is not the keeper's socket.", path.display()),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::writing(path, error)),
    }
    let listener = UnixListener::bind(path).map_err(|error| Error::writing(path, error))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| Error::writing(path, error))?;
    Ok(listener)
}

/// Who a client must be, given apart so tests stand in.
pub(super) trait Admit: Send + Sync {
    /// Whether the process at the other end of `socket` is admitted now. It
    /// may wait on the system, so it runs off the runtime.
    fn admits(&self, socket: BorrowedFd<'_>) -> bool;
}

/// This account's processes whose running code is signed as Atlas.
pub(super) struct Atlas;

impl Admit for Atlas {
    fn admits(&self, socket: BorrowedFd<'_>) -> bool {
        let peer = nix::unistd::getpeereid(socket)
            .ok()
            .map(|(uid, _)| uid.as_raw());
        let signed = atlas_native::peer::running(socket, &requirement_for(APP_IDENTIFIER))
            .is_some_and(|running| Some(running.uid) == peer);
        judge_client(peer, nix::unistd::getuid().as_raw(), signed)
    }
}

/// Whether a client whose account is `peer` and whose running code is
/// `signed` as Atlas is admitted by a keeper of the account `own`.
fn judge_client(peer: Option<u32>, own: u32, signed: bool) -> bool {
    peer == Some(own) && signed
}

/// Answer clients on `listener` until dropped.
pub(super) async fn serve(
    listener: UnixListener,
    keeper: Arc<Keeper>,
    grant: Grant,
    admit: Arc<dyn Admit>,
) {
    let grant = Arc::new(grant);
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let (keeper, grant, admit) = (keeper.clone(), grant.clone(), admit.clone());
        tokio::spawn(async move {
            answer(stream, &keeper, &grant, admit).await;
        });
    }
}

/// Whether the client on `stream` is admitted, judged off the runtime.
async fn admitted(stream: &UnixStream, admit: Arc<dyn Admit>) -> bool {
    let Ok(socket) = stream.as_fd().try_clone_to_owned() else {
        return false;
    };
    tokio::task::spawn_blocking(move || admit.admits(socket.as_fd()))
        .await
        .unwrap_or(false)
}

/// What a client asked the guest be told.
enum Telling {
    Stop,
    /// The person's Resume, and the station's Stop as it was read before it
    /// was asked: lifted by the keeper once the guest resumed.
    Resume(Option<StopRevision>),
}

/// Take one request on `stream` and answer it, the client judged once its
/// request came.
async fn answer(mut writing: UnixStream, keeper: &Keeper, grant: &Grant, admit: Arc<dyn Admit>) {
    let mut line = Vec::new();
    let mut reader = BufReader::new((&mut writing).take(REQUEST));
    let asked = tokio::time::timeout(ASKING, reader.read_until(b'\n', &mut line)).await;
    // Whatever came after the request is read already: a stage's bytes may
    // follow only once it is told to send them.
    let early = !reader.buffer().is_empty();
    drop(reader);
    let Ok(Ok(1..)) = asked else {
        return;
    };
    if !admitted(&writing, admit).await {
        return;
    }
    // Only a request said whole, its newline last, is taken: one cut short
    // at the client's end or at the size bound may be a change its client
    // holds as never sent.
    if line.last() != Some(&b'\n') {
        let mut refused = refusal(
            Code::BadRequest,
            "The keeper takes only a whole request, ended by its newline; nothing was done.",
        );
        refused["sent"] = json!(false);
        let _ = say(&mut writing, &refused).await;
        return;
    }
    let Ok(request) = serde_json::from_slice::<Value>(&line) else {
        let _ = say(
            &mut writing,
            &refusal(
                Code::BadRequest,
                "The keeper did not understand that request.",
            ),
        )
        .await;
        return;
    };
    let deadline = request["deadline_ms"]
        .as_u64()
        .map_or(DEADLINE, Duration::from_millis)
        .min(LONGEST);
    let revision = request
        .get("revision")
        .and_then(|revision| serde_json::from_value::<Option<StopRevision>>(revision.clone()).ok());
    match request["op"].as_str() {
        Some("hello") => {
            let _ = say(&mut writing, &keeper.hello()).await;
        }
        Some("stop") => {
            let _ = say(&mut writing, &told(keeper, Telling::Stop, deadline).await).await;
        }
        Some("resume") if !grant.admits(request["capability"].as_str()) => {
            let _ = say(
                &mut writing,
                &refusal(
                    Code::Denied,
                    "Only the person resumes agents on this VM, from Atlas's own window.",
                ),
            )
            .await;
        }
        Some("resume") if revision.is_some() => {
            let telling = Telling::Resume(revision.flatten());
            let _ = say(&mut writing, &told(keeper, telling, deadline).await).await;
        }
        Some("read")
            if request["request"].is_object()
                && read_lane(&request).is_none_or(|lane| !carries(&lane, &request["request"])) =>
        {
            let refused = refusal(
                Code::Denied,
                "The keeper does not carry that read to the guest.",
            );
            let _ = say(&mut writing, &refused).await;
        }
        // A change whose final reply a test build kept back is recovered
        // only when the person's Recover asks: never read back here.
        Some("read")
            if request["request"]["op"] == "outcome"
                && request["request"]["change"]
                    .as_str()
                    .is_some_and(|change| keeper.kept_back.holds(change)) =>
        {
            let unknown = refusal(
                Code::NotReady,
                "This test build keeps back what the change came to until it is recovered.",
            );
            let _ = say(&mut writing, &unknown).await;
        }
        Some("read")
            if request["request"].is_object()
                && (request["generation"].is_null() || request["generation"].is_u64()) =>
        {
            let ticket = if held::served_while_held(&request["request"]) {
                None
            } else {
                let Some(ticket) = ticket(keeper, Work::Read, &mut writing).await else {
                    return;
                };
                Some(ticket)
            };
            let pinned = request["generation"].as_u64();
            let lane = read_lane(&request).unwrap_or(LaneKind::Read);
            let asked = Asked {
                lane,
                request: &request["request"],
                pinned,
                deadline,
                ticket: ticket.as_ref(),
            };
            carried(keeper, asked, &mut writing).await;
        }
        Some("change") => {
            if let Some(ticket) = ticket(keeper, Work::Change, &mut writing).await {
                super::change::changed(keeper, &request, writing, &ticket).await;
            }
        }
        Some("stage") => {
            if let Some(ticket) = ticket(keeper, Work::Stage, &mut writing).await {
                super::stage::staged(keeper, &request, writing, early, &ticket).await;
            }
        }
        Some("picture") => {
            if let Some(ticket) = ticket(keeper, Work::Picture, &mut writing).await {
                super::picture::pictured(keeper, &request, writing, early, &ticket).await;
            }
        }
        Some("update-guest") => {
            let answered = super::update::asked(keeper, deadline).await;
            let _ = say(&mut writing, &answered).await;
        }
        Some("hold") => {
            let held = held::hold(keeper, &request, deadline).await;
            let _ = say(&mut writing, &held).await;
        }
        Some("release") => {
            let released = held::release(keeper, &request).await;
            let _ = say(&mut writing, &released).await;
        }
        Some("settle") => {
            let settled = super::change::settled(keeper, &request).await;
            let _ = say(&mut writing, &settled).await;
        }
        Some("recover") if request["generation"].is_u64() => {
            let generation = request["generation"].as_u64().unwrap_or_default();
            let recovered = super::recover::asked(keeper, generation, deadline).await;
            let _ = say(&mut writing, &recovered).await;
        }
        // A proof is an agent's, held to the person's maintenance as every
        // agent's lane is.
        Some("observe") => {
            let challenge = request["challenge"].as_str();
            let observed = super::observe::observed(keeper, challenge, deadline).await;
            let _ = say(&mut writing, &observed).await;
        }
        Some("handover")
            if request["change"]
                .as_str()
                .is_some_and(|change| Token::parse(change).is_some()) =>
        {
            // Held to the end: no Resume comes between the Stop kept for
            // the update and this keeper's exit.
            let _turn = keeper.turn.lock().await;
            let handed = handed_over(keeper, &request["change"], deadline).await;
            let _ = say(&mut writing, &handed).await;
            if handed["ok"] == true {
                drop(writing);
                keeper.ended.notify_one();
            }
        }
        _ => {
            let mut refused = refusal(Code::BadRequest, "The keeper does not serve that.");
            refused["capabilities"] = json!(CAPABILITIES);
            let _ = say(&mut writing, &refused).await;
        }
    }
}

/// Say the person's Stop, or their Resume, on the live generation, and what
/// the guest answered for it within `deadline`; unconfirmed otherwise. A
/// Resume the guest answered resumed lifts the station's Stop it was asked
/// for, under the keeper's turn, before anything else is said: a Stop given
/// since stays, and one given before it was asked refuses it unsaid.
async fn told(keeper: &Keeper, telling: Telling, deadline: Duration) -> Value {
    let unsaid = |why: Option<String>| json!({"ok": true, "said": false, "outcome": "unconfirmed", "why": why});
    let Some(session) = keeper.shared.live() else {
        return unsaid(keeper.shared.state().why.clone());
    };
    let _turn = match &telling {
        Telling::Resume(revision) => {
            let turn = keeper.turn.lock().await;
            // A Resume of a Stop that is no longer the one kept is never said:
            // a Stop given since holds agents.
            let stop = keeper.stop.clone();
            let now = super::blocking(move || stop.stop_revision()).await;
            if now.as_ref().ok() != Some(revision) {
                return refusal(
                    Code::Stopped,
                    "A Stop given since this Resume read it still holds agents; nothing was said.",
                );
            }
            // Held by the turn from this look through the Stop's lift, so a
            // hold for the person's maintenance comes wholly before or after.
            if let Some(refused) = held::resume_refused(keeper).await {
                return refused;
            }
            // From here the guest may resume, answered in time or not: what it
            // applied of the Stop no longer holds, and the Stop is said again
            // unless this Resume lifts it.
            keeper.resumed();
            Some(turn)
        }
        Telling::Stop => None,
    };
    let asked = match telling {
        Telling::Resume(_) => session.control.resume(),
        Telling::Stop => session.control.stop(),
    };
    let asked = match asked {
        Ok(asked) => asked,
        Err(error) => return unsaid(Some(error.error)),
    };
    let seq = asked.seq();
    let answer = tokio::time::timeout(deadline, asked.answer()).await;
    let outcome = match answer {
        Ok(Answer::Applied(outcome)) => serde_json::to_value(outcome).unwrap_or(json!(null)),
        Ok(Answer::Unconfirmed) | Err(_) => json!("unconfirmed"),
    };
    let mut told = json!({"ok": true, "said": true, "seq": seq, "outcome": outcome});
    if let (Telling::Resume(revision), Ok(Answer::Applied(Outcome::Resumed))) = (telling, answer) {
        let stop = keeper.stop.clone();
        told["lifted"] = match super::blocking(move || stop.lift_stop(revision.as_ref())).await {
            Ok(Resumed::Cleared) => json!("cleared"),
            Ok(Resumed::AlreadyClear) => json!("already-clear"),
            Ok(Resumed::StopHeld) => json!("stop-held"),
            Err(error) => serde_json::to_value(&error).unwrap_or_default(),
        };
    }
    told
}

/// Hand the session over for the update `change`, the caller holding the
/// keeper's turn: the station's Stop kept with the update as its reason,
/// unless a Stop is noted already, which holds agents and stays as it is;
/// said to the guest and answered within `deadline` (unconfirmed otherwise),
/// and the generation left for good. Any work whose outcome is uncertain
/// stays as it is, for the next keeper. A Stop that cannot be kept refuses
/// it, and nothing else is done.
async fn handed_over(keeper: &Keeper, change: &Value, deadline: Duration) -> Value {
    let stop = keeper.stop.clone();
    let noted = match super::blocking(move || stop.note_stop_unless_noted(StopWhy::Update)).await {
        Ok(noted) => noted,
        Err(error) => return serde_json::to_value(&error).unwrap_or_default(),
    };
    let mut handed = told(keeper, Telling::Stop, deadline).await;
    handed["noted"] = json!(noted);
    keeper.hand_over();
    handed["op"] = json!("handover");
    handed["change"] = change.clone();
    handed["instance"] = json!(keeper.instance);
    handed
}

/// A ticket for an agent's lane carrying `work`, or none, its refusal said
/// to the client on `writing` while the person's maintenance holds the VM.
async fn ticket<'a>(
    keeper: &'a Keeper,
    work: Work,
    writing: &mut UnixStream,
) -> Option<Ticket<'a>> {
    match held::admitted(keeper, work).await {
        Ok(ticket) => Some(ticket),
        Err(refused) => {
            let _ = say(writing, &refused).await;
            None
        }
    }
}

/// A read a client asked carried to the guest.
struct Asked<'a> {
    lane: LaneKind,
    request: &'a Value,
    /// The generation it is carried only in, when it names one.
    pinned: Option<u64>,
    deadline: Duration,
    /// Its agent's lane's ticket; none for a read served while agents are
    /// held.
    ticket: Option<&'a Ticket<'a>>,
}

/// Carry the read `asked`, passing each reply line on to the client as it
/// comes, and a refusal line when it could not be carried. A read pinned to
/// a generation is carried only in that generation; an agent's read is
/// cancelled once the person's maintenance holds its lane.
async fn carried(keeper: &Keeper, asked: Asked<'_>, writing: &mut UnixStream) {
    let Asked {
        lane,
        request,
        pinned,
        deadline,
        ticket,
    } = asked;
    let Some(session) = keeper.shared.live() else {
        let why = keeper.shared.state().why.clone();
        let refused = Error::new(
            Code::Transport,
            why.unwrap_or_else(|| "The VM's session is not live.".to_owned()),
        );
        let _ = say(writing, &serde_json::to_value(&refused).unwrap_or_default()).await;
        return;
    };
    if pinned.is_some_and(|pinned| pinned != session.control.generation().0) {
        let refused = Error::new(
            Code::SessionChanged,
            "The VM's session this read was meant for has ended; nothing was read.",
        );
        let _ = say(writing, &serde_json::to_value(&refused).unwrap_or_default()).await;
        return;
    }
    let (lines, replies) = mpsc::unbounded_channel();
    let stop = async {
        match ticket {
            Some(ticket) => {
                ticket.stopped().await;
                held::held_now(keeper)
            }
            None => std::future::pending().await,
        }
    };
    let reading = read_until(&session, lane, request, deadline, stop, move |line| {
        lines.send(line).is_ok()
    });
    let carrying = async {
        // Owning what the read hands on, so a client that went stops the
        // read, and its lane is cancelled.
        let passing = async {
            let mut replies = replies;
            while let Some(line) = replies.recv().await {
                let mut line = line.into_bytes();
                line.push(b'\n');
                if writing.write_all(&line).await.is_err() {
                    return false;
                }
            }
            true
        };
        let (read, open) = tokio::join!(reading, passing);
        if let (Err(error), true) = (read, open) {
            let _ = say(writing, &serde_json::to_value(&error).unwrap_or_default()).await;
        }
    };
    // A client that stops reading never holds the keeper past the read's
    // deadline: it is left with its connection's end, never a line cut short
    // and a word after it.
    let _ = tokio::time::timeout(deadline + super::change::SAYING, carrying).await;
}

fn refusal(code: Code, why: &str) -> Value {
    serde_json::to_value(Error::new(code, why)).unwrap_or_default()
}

async fn say(writing: &mut UnixStream, said: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(said)?;
    line.push(b'\n');
    writing.write_all(&line).await?;
    writing.flush().await
}

#[cfg(test)]
#[path = "clients_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "installed_tests.rs"]
mod installed_tests;
