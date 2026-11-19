//! The person's grant of one lifecycle operation. Adding, removing,
//! uninstalling and maintaining a VM are the person's, never an agent's, and
//! a terminal does not prove who typed, so a command asks through Atlas.app:
//! its request is kept here, Atlas.app is opened with the request's ID, and
//! its approval window, a native window of the person's own session, shows
//! what, which VM and from which session. The person allows or refuses it
//! there, through a command only Atlas.app may run. An allowed request is a
//! grant bound to that one operation and its change, used once, and gone
//! after [`LIFE`]. No flag, variable, actor or terminal stands in for it.
//!
//! In `requests/`, each request is `<id>.json`, made by the command that
//! asks; `<id>.shown` once Atlas.app has read it, `<id>.answer` once the
//! person answered and `<id>.taken` once its grant was used, each made only
//! where none is, so each happens once.
use super::{Lifecycle, create, read, remove};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// How long the person is given to answer.
const ANSWERING: Duration = Duration::from_secs(5 * 60);
/// How long Atlas.app is given to show the request once it is opened.
const SHOWING: Duration = Duration::from_secs(60);
/// How long an allowed request stays a grant.
const LIFE: Duration = Duration::from_secs(10 * 60);
/// How often the answer is looked for while it is awaited.
const LOOKING: Duration = Duration::from_millis(250);
/// How long a request's files are kept, whatever came of it.
const KEPT: Duration = Duration::from_secs(24 * 60 * 60);

/// Where the requests are, in the lifecycle folder.
const REQUESTS: &str = "requests";

/// Atlas.app's bundle identifier, which `open` finds it by.
const APP: &str = atlas_core::app_bundle::APP_IDENTIFIER;

/// Set when this build was made for live checks, which allow every request
/// without the window; `scripts/release.sh` refuses to release one.
const FAKE_GRANT: bool = option_env!("ATLAS_TEST_FAKE_GRANT").is_some();

/// What a grant lets a command do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Add a VM.
    Add,
    /// Remove a VM.
    Remove,
    /// Uninstall Atlas, its VMs with it.
    Uninstall,
    /// Open a VM's window for the person's maintenance, or end it.
    Maintain,
}

/// The one operation a grant is for: its kind, the VM's identifier and
/// name (for an uninstall, every VM it removes, by name), the settings file
/// it changes, by its path, whether it calls an uninstall off, and for a
/// maintenance whether the VM is started again first or it is ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub kind: Kind,
    pub vm: DesktopId,
    pub name: String,
    pub settings: PathBuf,
    /// Calls the uninstall under way off, rather than uninstalling.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
    /// Stops the VM and starts it again before its maintenance.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
    /// Ends the maintenance, rather than beginning it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub end: bool,
}

/// A request as kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    id: Token,
    operation: Operation,
    /// The change the operation is to be made as, fresh for this request.
    change: Token,
    /// The session the asking command came from, as the person reads it.
    session: String,
    /// When it was asked, in milliseconds since 1970.
    asked: u64,
    /// When it is no longer answered.
    until: u64,
}

/// The person's answer, as kept.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    allowed: bool,
    /// When it was given, in milliseconds since 1970.
    at: u64,
}

/// When something happened to a request, in milliseconds since 1970.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Moment {
    at: u64,
}

/// What Atlas.app shows of a request, and all it reads of one.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    pub id: Token,
    pub kind: Kind,
    pub name: String,
    pub vm: DesktopId,
    /// The settings file, as the person reads its path.
    pub settings: String,
    pub session: String,
    /// When it is no longer answered, in milliseconds since 1970.
    pub until: u64,
    /// What the person answered already, when they did: nothing is left
    /// to answer then, whatever the time.
    /// Whether it calls the uninstall under way off.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
    /// Whether the VM is stopped and started again before its maintenance.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
    /// Whether it ends a maintenance.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub end: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered: Option<Answered>,
}

/// What became of a request the person answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Answered {
    /// Allowed, its grant not used yet.
    Allowed,
    /// Refused.
    Refused,
    /// Allowed, and its grant used.
    Used,
}

/// A request the person allowed, for the operation it names. Only
/// [`ask`] makes one, in the process that asked; [`Grant::take`] uses it.
#[derive(Debug)]
pub struct Grant {
    id: Token,
    operation: Operation,
    change: Token,
    lifecycle: Lifecycle,
}

impl Grant {
    /// The operation it is for.
    #[must_use]
    pub fn operation(&self) -> &Operation {
        &self.operation
    }

    /// The change the operation is made as.
    #[must_use]
    pub fn change(&self) -> &Token {
        &self.change
    }

    /// Use the grant for `operation`, once, before anything of it is done:
    /// the change it is made as.
    ///
    /// # Errors
    ///
    /// Refuses another operation (another VM, kind, name or settings file),
    /// a grant the person did not give, one used already and one past its
    /// life; fails when its files cannot be read or kept.
    pub fn take(self, operation: &Operation) -> Result<Token> {
        self.take_at(operation, atlas_core::time::now_ms())
    }

    fn take_at(self, operation: &Operation, now: u64) -> Result<Token> {
        let mismatch = if operation.kind != self.operation.kind
            || operation.cancel != self.operation.cancel
            || operation.restart != self.operation.restart
            || operation.end != self.operation.end
        {
            Some("another kind of change")
        } else if operation.vm != self.operation.vm {
            Some("another VM")
        } else if operation != &self.operation {
            Some("another name or settings file")
        } else {
            None
        };
        if let Some(other) = mismatch {
            return Err(refusal(format!(
                "The person's grant is for {other}; nothing was done."
            )));
        }
        let requests = self.lifecycle.inner(REQUESTS)?;
        let request: Option<Request> = read(&file(&requests, &self.id, "json"), "A request")?;
        let answer: Option<Answer> = read(&file(&requests, &self.id, "answer"), "An answer")?;
        let granted = request.is_some_and(|request| {
            request.operation == self.operation && request.change == self.change
        }) && answer.as_ref().is_some_and(|answer| answer.allowed);
        let Some(answer) = answer.filter(|_| granted) else {
            return Err(refusal(
                "The person has not allowed this in Atlas.app; nothing was done.",
            ));
        };
        if now >= answer.at.saturating_add(millis(LIFE)) {
            return Err(refusal(
                "The person's grant has expired; nothing was done. Ask again.",
            ));
        }
        if !create(&file(&requests, &self.id, "taken"), &Moment { at: now })? {
            return Err(refusal(
                "The person's grant was used already; nothing was done. Ask again.",
            ));
        }
        Ok(self.change)
    }
}

/// Ask the person, through Atlas.app, to allow `operation`, and wait for
/// their answer, bounded: the grant they gave.
///
/// # Errors
///
/// Refuses when the person refused, did not answer in time, or Atlas.app
/// is missing or did not show the request; fails when the request cannot
/// be kept.
pub fn ask(operation: Operation) -> Result<Grant> {
    ask_with(&Lifecycle::own()?, &App, &Wall, operation, &session())
}

/// What Atlas.app shows of the request `id`, now marked shown.
///
/// # Errors
///
/// Refuses a request that is not waiting for an answer; fails when it
/// cannot be read.
pub fn request(id: &Token) -> Result<Shown> {
    request_at(&Lifecycle::own()?, id, atlas_core::time::now_ms())
}

/// Keep the person's answer to the request `id`: whether they `allowed`
/// it.
///
/// # Errors
///
/// Refuses a request that is not waiting for an answer and one answered
/// already; fails when the answer cannot be kept.
pub fn answer(id: &Token, allowed: bool) -> Result<()> {
    answer_at(&Lifecycle::own()?, id, allowed, atlas_core::time::now_ms())
}

/// What opens Atlas.app's approval window, given apart so tests stand in.
trait Opener {
    /// Open Atlas.app to show the request `id`.
    fn open(&self, id: &Token) -> Result<()>;
}

/// Atlas.app, opened by Launch Services as a new instance, so it shows
/// the request whether or not Atlas runs already, set up or not.
struct App;

impl Opener for App {
    fn open(&self, id: &Token) -> Result<()> {
        let opened = atlas_core::spawn::output(
            std::process::Command::new("/usr/bin/open")
                .args(["-n", "-b", APP, "--args", "--approve"])
                .arg(id.as_str()),
        );
        match opened {
            Ok(output) if output.status.success() => Ok(()),
            _ => Err(refusal(
                "Atlas.app could not be opened to ask the person, so nothing was done. Install \
                 Atlas.app, then try again.",
            )),
        }
    }
}

/// The time, and the pause between looks, given apart so tests move time
/// by hand.
trait Clock {
    /// Now, in milliseconds since 1970.
    fn now(&self) -> u64;
    /// Wait `pause`.
    fn pause(&self, pause: Duration);
}

/// The wall clock.
struct Wall;

impl Clock for Wall {
    fn now(&self) -> u64 {
        atlas_core::time::now_ms()
    }

    fn pause(&self, pause: Duration) {
        std::thread::sleep(pause);
    }
}

fn ask_with(
    lifecycle: &Lifecycle,
    opener: &impl Opener,
    clock: &impl Clock,
    operation: Operation,
    session: &str,
) -> Result<Grant> {
    let requests = lifecycle.inner(REQUESTS)?;
    sweep(&requests);
    let asked = clock.now();
    let request = Request {
        id: Token::new(),
        operation,
        change: Token::new(),
        session: session.to_owned(),
        asked,
        until: asked.saturating_add(millis(ANSWERING)),
    };
    let id = request.id.clone();
    if !create(&file(&requests, &id, "json"), &request)? {
        return Err(Error::new(Code::State, "A request could not be made."));
    }
    // A request that came to nothing is taken back, so no late answer
    // allows it.
    let withdrawn = |error: Error| {
        withdraw(&requests, &id);
        error
    };
    if FAKE_GRANT {
        create(&file(&requests, &id, "shown"), &Moment { at: asked })?;
        create(
            &file(&requests, &id, "answer"),
            &Answer {
                allowed: true,
                at: asked,
            },
        )?;
    } else {
        opener.open(&id).map_err(withdrawn)?;
    }
    loop {
        if let Some(answer) = read::<Answer>(&file(&requests, &id, "answer"), "An answer")? {
            // A refused request is kept as answered, so the window shows
            // the refusal if opened again; it is swept later.
            if !answer.allowed {
                return Err(refusal(
                    "The person did not allow it in Atlas.app; nothing was done.",
                ));
            }
            return Ok(Grant {
                id,
                operation: request.operation,
                change: request.change,
                lifecycle: lifecycle.clone(),
            });
        }
        let now = clock.now();
        let shown = file(&requests, &id, "shown").exists();
        if !shown && now >= asked.saturating_add(millis(SHOWING)) {
            return Err(withdrawn(refusal(
                "Atlas.app did not show the request, so nothing was done. Open Atlas.app, then \
                 try again.",
            )));
        }
        if now >= request.until {
            return Err(withdrawn(refusal(
                "Nobody answered in Atlas.app in time, so nothing was done. Try again, and \
                 answer there.",
            )));
        }
        clock.pause(LOOKING);
    }
}

fn request_at(lifecycle: &Lifecycle, id: &Token, now: u64) -> Result<Shown> {
    let requests = lifecycle.inner(REQUESTS)?;
    let request = kept(&requests, id)?;
    // An answered request shows what was answered, never asks again, and
    // is judged so before its time is.
    let answered = answered(&requests, id)?;
    if answered.is_none() {
        expiry(&request, now)?;
    }
    // Shown once; showing it again changes nothing.
    create(&file(&requests, id, "shown"), &Moment { at: now })?;
    Ok(Shown {
        id: request.id,
        kind: request.operation.kind,
        name: request.operation.name,
        vm: request.operation.vm,
        settings: atlas_core::config::tilde(&request.operation.settings),
        session: request.session,
        cancel: request.operation.cancel,
        restart: request.operation.restart,
        end: request.operation.end,
        until: request.until,
        answered,
    })
}

fn answer_at(lifecycle: &Lifecycle, id: &Token, allowed: bool, now: u64) -> Result<()> {
    let requests = lifecycle.inner(REQUESTS)?;
    let request = kept(&requests, id)?;
    if answered(&requests, id)?.is_some() {
        return Err(answered_already());
    }
    expiry(&request, now)?;
    if !create(&file(&requests, id, "answer"), &Answer { allowed, at: now })? {
        return Err(answered_already());
    }
    Ok(())
}

fn answered_already() -> Error {
    refusal("This request was answered already.")
}

/// The request `id` in `requests`, while it is kept.
fn kept(requests: &Path, id: &Token) -> Result<Request> {
    read(&file(requests, id, "json"), "A request")?
        .ok_or_else(|| refusal("This request is no longer waiting for an answer."))
}

/// What the person answered to the request `id` in `requests`; none while
/// they have not.
fn answered(requests: &Path, id: &Token) -> Result<Option<Answered>> {
    if file(requests, id, "taken").exists() {
        return Ok(Some(Answered::Used));
    }
    let answer: Option<Answer> = read(&file(requests, id, "answer"), "An answer")?;
    Ok(answer.map(|answer| {
        if answer.allowed {
            Answered::Allowed
        } else {
            Answered::Refused
        }
    }))
}

/// Refuse `request` once its time is up at `now`.
fn expiry(request: &Request, now: u64) -> Result<()> {
    if now >= request.until {
        return Err(refusal(
            "This request has expired; it was not answered in time.",
        ));
    }
    Ok(())
}

/// The file of the request `id` in `requests` that ends in `suffix`.
fn file(requests: &Path, id: &Token, suffix: &str) -> PathBuf {
    requests.join(format!("{}.{suffix}", id.as_str()))
}

/// Take the request `id` in `requests` back: best effort, as a request
/// left is refused once it expires and swept later.
fn withdraw(requests: &Path, id: &Token) {
    for suffix in ["json", "shown", "answer"] {
        let _ = remove(&file(requests, id, suffix));
    }
}

/// Remove what requests left in `requests` past [`KEPT`]: best effort, as
/// each is refused long before then.
fn sweep(requests: &Path) {
    let Ok(entries) = std::fs::read_dir(requests) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > KEPT);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Where the asking command comes from, as the person reads it.
fn session() -> String {
    session_of(parent_is_app(), || {
        atlas_core::host::HostSession::from_env().map(|session| match session.folder() {
            Some(folder) => format!("{} in {folder}", session.host().label()),
            None => session.host().label().to_owned(),
        })
    })
}

/// Where a command comes from, as the person reads it: Atlas when it is
/// `from_app`, else the agent host and its project that `host` names, or a
/// plain command line.
fn session_of(from_app: bool, host: impl FnOnce() -> Option<String>) -> String {
    if from_app {
        return "Atlas".to_owned();
    }
    host().unwrap_or_else(|| "a command line no agent names".to_owned())
}

/// Whether this command's parent is the app of the Atlas.app it runs from,
/// running code signed as Atlas, as its code says: nothing it says of
/// itself, an environment variable among it, makes it so. It is only what
/// the card says; the grant's safety never rests on it.
fn parent_is_app() -> bool {
    let parent = std::os::unix::process::parent_id();
    let ours = std::env::current_exe()
        .ok()
        .and_then(|executable| std::fs::canonicalize(executable).ok())
        .and_then(|executable| atlas_core::app_bundle::app_of(&executable));
    ours.is_some_and(|app| atlas_core::app_bundle::runs_app(parent, &app))
        // The one judged is still this command's parent, never another
        // process given its number since.
        && std::os::unix::process::parent_id() == parent
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn refusal(message: impl Into<String>) -> Error {
    Error::new(Code::Denied, message)
}

#[cfg(test)]
#[path = "grant_tests.rs"]
mod tests;
