//! The keeper's clients: Atlas's own processes, on `keeper.sock` in the VM's
//! private station folder. One request per connection, one JSON line each
//! way (a read's reply lines as they come), and the client admitted on that
//! very request: this account's, its running code signed as Atlas.
//!
//! Resume is the person's alone: it is taken only with the capability the
//! keeper made at its start and keeps in its station folder, which the
//! person's own surface reads there. What a client says of itself, that it
//! is the viewer included, counts for nothing.
//!
//! An update asks `handover`, with its change: the keeper keeps the station's
//! Stop for the update, says it, leaves its generation, answers and ends,
//! all under its turn, so no Resume comes between.
use super::{Keeper, read};
use atlas_control::{Resumed, StopRevision, StopWhy};
use atlas_core::{
    app_bundle::signature::{APP_IDENTIFIER, requirement_for},
    error::{Code, Error, Result},
    files::Token,
};
use atlas_session::{control::Outcome, keys::PairingSecret, station::control::Answer};
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
pub(super) const CAPABILITIES: [&str; 4] = ["read-v1", "stop-v1", "resume-v1", "handover-v1"];
/// The Resume capability's name in the station folder.
pub(crate) const RESUME_KEY: &str = "resume.key";
/// The longest request.
const REQUEST: u64 = 1 << 20;
/// How long a client has to say its request.
const ASKING: Duration = Duration::from_secs(10);
/// A request's deadline when it names none, and the longest it may name.
const DEADLINE: Duration = Duration::from_secs(30);
const LONGEST: Duration = Duration::from_secs(120);

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
    drop(reader);
    let Ok(Ok(1..)) = asked else {
        return;
    };
    if !admitted(&writing, admit).await {
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
        Some("read") if request["request"].is_object() => {
            carried(keeper, &request["request"], deadline, &mut writing).await;
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

/// Carry `request` on a read lane, passing each reply line on to the client
/// as it comes, and a refusal line when it could not be carried.
async fn carried(keeper: &Keeper, request: &Value, deadline: Duration, writing: &mut UnixStream) {
    let Some(session) = keeper.shared.live() else {
        let why = keeper.shared.state().why.clone();
        let refused = Error::new(
            Code::Transport,
            why.unwrap_or_else(|| "The VM's session is not live.".to_owned()),
        );
        let _ = say(writing, &serde_json::to_value(&refused).unwrap_or_default()).await;
        return;
    };
    let (lines, mut replies) = mpsc::unbounded_channel();
    let reading = read(&session, request, deadline, move |line| {
        lines.send(line).is_ok()
    });
    let passing = async {
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
mod tests;
