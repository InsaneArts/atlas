//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
//! The service retains its gate, registered work and engine through cleanup;
//! a vanished request or reply cannot acknowledge an unknown outcome.
use super::{
    accessibility_trusted, asking,
    ax::Looks,
    events::Engine,
    lease::{Gate, Grant, Who, busy},
    session_info,
};
use atlas_core::{
    actions::{self, Action, Expect},
    error::{Code, Error, Holder, Result},
    session::{Display, InputEpoch},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    signal::unix::{SignalKind, signal},
    sync::{Mutex, Semaphore, watch},
    task::JoinSet,
};

mod batch;
mod operations;
mod request;

use batch::{Done, Failure, Perform, run};
use operations::{
    Cancellation, ConnectionPhase, Operations, combine_cleanup, connection_outcome,
    drain_and_release, finish, on_engine, reset, worker_failed,
};
use request::Request;

/// Twice the controller's 32 KiB action limit, leaving room for the owner,
/// epoch and JSON re-encoding around any batch the controller accepts.
const MAX_REQUEST: usize = 64 * 1024;

/// What this service can do beyond plain input, as its hello lists it:
/// authenticated shutdown, the person's Stop, checking an agent's target
/// app, with the Dock covering where it takes clicks, and the window and
/// field it names, reading an app's elements as looks a click can name, and
/// acting on an element of a look through Accessibility.
const CAPABILITIES: [&str; 10] = [
    atlas_core::session::SHUTDOWN,
    atlas_core::session::HOLD,
    atlas_core::session::FOCUS_APP,
    atlas_core::session::FOCUS_DOCK,
    atlas_core::session::FOCUS_WINDOW,
    atlas_core::session::FOCUS_FIELD,
    atlas_core::session::LOOKS,
    atlas_core::session::ELEMENT_ACTIONS,
    atlas_core::session::TYPED_FIELDS,
    atlas_core::session::ASKS,
];

struct Service {
    engine: Arc<Mutex<Engine>>,
    operations: Operations,
    gate: Arc<Gate>,
    /// Shared with the engine, which finds referenced elements in them.
    looks: Arc<Looks>,
    /// Shared with recovery's engine work, which rebinds both together.
    epoch: Arc<RwLock<InputEpoch>>,
    /// SHA-256 of the executable this process started from. Hashed once at
    /// start: `install` later renames a new build over the same path.
    build: Option<String>,
    /// The file this process started from, by path and identity. When the
    /// file there is replaced (an app update, even with the same build), the
    /// running process no longer matches it and macOS may drop its events.
    executable: Option<Executable>,
    /// Signalled once an acknowledged shutdown has been answered.
    stopping: tokio::sync::Notify,
    /// Present while the person has stopped agents, so a stop outlives a
    /// restart of this service (recovery starts a new one).
    stop_marker: PathBuf,
    /// Where a read of an app's elements is told to the helper's other
    /// processes, as the engine tells of input.
    activity: PathBuf,
    /// Where the person typed into a password field while they held
    /// control, by their lease, until they give it back: the app and the
    /// page's host, never what was typed.
    seen: Arc<std::sync::Mutex<Option<(String, Value)>>>,
    /// How a password field is looked for: the Mac's Accessibility, or a
    /// test's script.
    look: Arc<dyn PasswordLook>,
}

/// A file by path and (device, inode).
type Executable = (PathBuf, (u64, u64));

/// The started-from file's SHA-256, and its path with its (device, inode).
/// Both come from one open descriptor, so they describe the same file even
/// if the path is replaced meanwhile.
fn started_from() -> Option<(Option<String>, Executable)> {
    use std::os::unix::fs::MetadataExt;

    // Started through the shared link, the executable is the file it names.
    let path = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()?;
    let file = atlas_core::files::open_plain(&path)?;
    let metadata = file.metadata().ok()?;

    let build = atlas_core::digest::sha256(&file);
    Some((build, (path, (metadata.dev(), metadata.ino()))))
}

impl Service {
    /// Whether the file this process started from has been replaced.
    fn replaced(&self) -> bool {
        use std::os::unix::fs::MetadataExt;

        self.executable.as_ref().is_none_or(|(path, identity)| {
            std::fs::metadata(path).map_or(true, |metadata| {
                (metadata.dev(), metadata.ino()) != *identity
            })
        })
    }

    fn epoch(&self) -> Result<InputEpoch> {
        self.epoch
            .read()
            .map(|value| value.clone())
            .map_err(|_| Error::new(Code::Native, "Input identity failed"))
    }

    fn check(&self) -> Result<()> {
        self.operations.check()?;
        if InputEpoch::from_proof(&session_info()?, std::process::id())? != self.epoch()?
            || !accessibility_trusted()
        {
            return Err(Error::new(
                Code::Identity,
                "The input session or permission changed; release control",
            ));
        }

        Ok(())
    }

    async fn dispatch(
        &self,
        request: Request,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        // Stop, release, recovery and shutdown keep working after session or
        // permission loss, so only control and its uses need both intact. A
        // look proves its own session and goes on during a Stop or the
        // person's control: it sends nothing.
        match &request {
            Request::Renew { .. }
            | Request::Unknown { .. }
            | Request::Acquire { .. }
            | Request::Input { .. } => self.check()?,
            Request::Hello
            | Request::Shutdown { .. }
            | Request::Hold { .. }
            | Request::Release { .. }
            | Request::Reset { .. }
            | Request::Elements { .. } => {}
        }

        match request {
            Request::Hello => self.hello().await,
            Request::Shutdown { pid } => self.shutdown(pid, phase).await,
            Request::Hold { on } => self.set_hold(on?, phase).await,
            Request::Release { owner } => self.release(owner, phase).await,
            Request::Renew { owner } => self.renew(owner, phase),
            Request::Unknown { epoch } => self.unknown(epoch),
            Request::Reset { epoch } => self.recover(epoch?, phase).await,
            Request::Acquire { owner, epoch, who } => self.acquire(owner, epoch, who, phase).await,
            Request::Input {
                owner,
                epoch,
                actions,
                expect,
                app,
                asking,
            } => {
                self.input_asking(owner, epoch, actions, expect, app, asking, phase)
                    .await
            }
            Request::Elements { epoch, expect } => self.look(epoch, expect).await,
        }
    }

    /// One look at the elements of the app `expect` names, in the session
    /// `expected` names as the session proves itself now, whatever epoch
    /// input is pinned to; kept so that a reference names one of its
    /// objects alone. It grants nothing: a click finds its element again
    /// under its own control and checks. Missing Accessibility is the read's
    /// own failure, as for any other reader.
    async fn look(
        &self,
        expected: Result<InputEpoch>,
        expect: Result<Option<Expect>>,
    ) -> Result<Value> {
        let epoch = proven(expected?)?;
        let expect =
            expect?.ok_or_else(|| Error::new(Code::Input, "A look names the app it reads"))?;
        let activity = self.activity.clone();
        let read = tokio::task::spawn_blocking(move || {
            let _acting = super::activity::acting(&activity);
            super::ax::read(&expect)
        })
        .await
        .map_err(|cause| {
            Error::new(Code::Elements, "The accessibility read did not finish").with_source(cause)
        })??;

        // A session or display that changed while the app was read keeps
        // nothing of it.
        let epoch = proven(epoch)?;
        let (look, snapshot) = self.looks.keep(epoch, read, std::time::Instant::now())?;
        Ok(json!({
            "ok": true,
            "look": look.to_string(),
            "elements": snapshot.elements,
            "truncated": snapshot.incomplete,
            "reasons": snapshot.stops,
            "errors": snapshot.errors,
        }))
    }

    async fn hello(&self) -> Result<Value> {
        self.operations.reap().await?;
        let held_input = self
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.held_input());

        Ok(json!({
            "ok": true,
            "protocolVersion": 1,
            "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"),
            "build": self.build,
            "replaced": self.replaced(),
            "capabilities": CAPABILITIES,
            "accessibility": accessibility_trusted(),
            "ready": self.check().is_ok(),
            "enforcedEpoch": self.epoch()?,
            "session": session_info()?,
            "controlActive": self.gate.active()?,
            "held": self.gate.held()?,
            "heldInput": held_input,
            "streaming": true,
        }))
    }

    async fn shutdown(&self, pid: u64, phase: &watch::Sender<ConnectionPhase>) -> Result<Value> {
        // Bound to this process: a replacement that has since bound the same
        // socket never honours a request meant for its predecessor.
        if pid != u64::from(std::process::id()) {
            return Err(Error::new(
                Code::Identity,
                "Shutdown names a different input service process",
            ));
        }

        self.check_uncontrolled()?;

        self.operations.drain().await?;

        // Release held input first; only an acknowledged release closes the
        // gate for good. Holding the engine lock keeps acquire out meanwhile,
        // and one that took the lock first is seen once it is held.
        let engine = self.engine.clone().lock_owned().await;
        self.check_uncontrolled()?;

        phase.send_replace(ConnectionPhase::Mutation("shutdown"));
        let (_engine, released) = self.operations.clean_up(engine, Engine::release).await?;
        if let Err(error) = released {
            return Err(Error::new(
                Code::Uncertain,
                format!("Held input could not be released: {}", error.error),
            )
            .uncertain()
            .with_source(error));
        }

        self.gate.close()?;
        Ok(json!({ "ok": true, "stopping": true, "pid": std::process::id() }))
    }

    fn check_uncontrolled(&self) -> Result<()> {
        if self.gate.active()? {
            return Err(Error::new(
                Code::Busy,
                "The input service has an active controller",
            ));
        }

        Ok(())
    }

    /// Stop and resume remain available after session or permission loss.
    async fn set_hold(&self, on: bool, phase: &watch::Sender<ConnectionPhase>) -> Result<Value> {
        let reply = hold(&self.gate, &self.stop_marker, on, phase)?;
        drain_and_release(&self.operations, &self.engine, &self.gate).await?;
        Ok(reply)
    }

    async fn release(
        &self,
        owner: String,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        self.gate.cancel(Some(&owner))?;
        phase.send_replace(ConnectionPhase::Mutation("release"));
        drain_and_release(&self.operations, &self.engine, &self.gate).await?;
        let mut reply = json!({ "ok": true, "released": true });
        if let Some(seen) = self.take_seen(&owner) {
            reply["signedIn"] = seen;
        }
        Ok(reply)
    }

    /// What the person's lease `owner` typed a password into, taken once.
    fn take_seen(&self, owner: &str) -> Option<Value> {
        let mut seen = self.seen.lock().ok()?;
        match seen.take() {
            Some((holder, value)) if holder == owner => Some(value),
            other => {
                *seen = other;
                None
            }
        }
    }

    fn renew(&self, owner: String, phase: &watch::Sender<ConnectionPhase>) -> Result<Value> {
        self.gate.renew(&owner)?;
        phase.send_replace(ConnectionPhase::Mutation("renew"));
        Ok(json!({ "ok": true }))
    }

    fn check_epoch(&self, expected: InputEpoch) -> Result<InputEpoch> {
        let epoch = self.epoch()?;
        if expected != epoch {
            return Err(Error::session_changed());
        }
        Ok(epoch)
    }

    fn unknown(&self, epoch: Result<InputEpoch>) -> Result<Value> {
        self.check_epoch(epoch?)?;
        Err(Error::new(Code::Input, "Unknown native operation"))
    }

    async fn recover(
        &self,
        expected: InputEpoch,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
        if expected != current || !accessibility_trusted() {
            return Err(Error::new(
                Code::Identity,
                "Recovery requires the current hidden session and permission",
            ));
        }

        phase.send_replace(ConnectionPhase::Mutation("reset"));
        let enforced = self.epoch.clone();
        reset(&self.operations, &self.engine, &self.gate, move |engine| {
            let rebound = engine.rebind(current);

            // Enforce whatever epoch the engine now holds, even when its
            // release failed: the two never disagree, and the timer keeps
            // retrying that release against the display it was bound to.
            *enforced
                .write()
                .map_err(|_| Error::new(Code::Native, "Input identity failed"))? =
                engine.epoch.clone();

            rebound
        })
        .await?;
        Ok(json!({ "ok": true, "released": true }))
    }

    async fn acquire(
        &self,
        owner: String,
        expected: Result<InputEpoch>,
        who: Who,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        self.check_epoch(expected?)?;
        let mut engine = self.engine.clone().lock_owned().await;
        self.operations.check()?;

        if let Some(holder) = self.gate.holder()? {
            return Err(busy(
                holder,
                who,
                "The native input service already has a controller",
            ));
        }

        engine.check()?;
        phase.send_replace(ConnectionPhase::Mutation("acquire"));
        let (mut engine, released) = self.operations.clean_up(engine, Engine::release).await?;
        released?;
        engine.grant = Some(self.gate.acquire(&owner, who)?);

        Ok(json!({ "ok": true }))
    }

    #[cfg(test)]
    async fn input(
        &self,
        owner: String,
        expected: Result<InputEpoch>,
        actions: Result<Vec<Action>>,
        expect: Result<Option<Expect>>,
        report_app: bool,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        self.input_asking(
            owner,
            expected,
            actions,
            expect,
            report_app,
            Ok(None),
            phase,
        )
        .await
    }

    /// [`Service::input`], an agent's request judged first by the accounts
    /// and grants it brings: refused before its first event when the person
    /// must be asked, or the account is Look Only.
    #[allow(clippy::too_many_arguments)]
    async fn input_asking(
        &self,
        owner: String,
        expected: Result<InputEpoch>,
        actions: Result<Vec<Action>>,
        expect: Result<Option<Expect>>,
        report_app: bool,
        asking: Result<Option<asking::Asking>>,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        let epoch = self.check_epoch(expected?)?;
        let grant = self.gate.grant(&owner)?;
        let actions = actions?;
        actions::check(
            &actions,
            &Display {
                width: epoch.width,
                height: epoch.height,
                scale: 1.0,
            },
        )?;
        let count = actions.len();
        let typed = actions
            .iter()
            .any(|action| matches!(action, Action::Type { .. } | Action::Key { .. }));
        let expect = expect?;
        let asking = asking?;
        agent_target(&self.gate, expect.as_ref(), &actions)?;
        // A command's check before its first request: judged, nothing sent.
        if let Some(asking) = asking.as_ref().filter(|asking| asking.dry)
            && grant.person().is_none()
        {
            drop(grant.authorize()?);
            let engine = self.engine.clone().lock_owned().await;
            let asking = asking.clone();
            let intended = expect
                .as_ref()
                .and_then(|expect| super::apps::bundle_id(expect.pid()));
            return tokio::task::spawn_blocking(move || {
                gate_request(&engine, &actions, &asking, intended)
                    .map(|()| json!({"ok": true, "performed": 0, "dry": true}))
            })
            .await
            .map_err(|cause| worker_failed().with_source(cause))?;
        }
        phase.send_replace(ConnectionPhase::Mutation("input"));
        let mut cancellation = Cancellation {
            gate: self.gate.clone(),
            owner: Some(owner),
        };
        let (done, dock_pressed) = match self
            .execute(grant, actions, expect.clone(), typed, asking)
            .await?
        {
            Ok(done) => done,
            Err(failure) => return Ok(failure.reply()),
        };

        let acted = done.acted.as_ref();
        let landed = if report_app {
            landed(dock_pressed, acted.map(|acted| acted.instance.pid)).await
        } else {
            None
        };
        let (app, bundle_id) = landed.map_or((None, None), |(app, id)| (Some(app), id));

        let mut reply = json!({
            "ok": true,
            "performed": done.performed(count),
            "app": app,
            "bundleId": bundle_id,
        });
        // Echoed, so the controller knows each action was checked.
        if let Some(expect) = expect {
            reply["expect"] = json!(expect);
        }
        // The role of the field it named, so an agent's activity may name
        // the field by the name it gave: never the field's own text.
        if let Some(role) = done.field {
            reply["field"] = json!({ "role": role });
        }
        // How an action on an element went, never what it set.
        if let Some(acted) = acted {
            reply["mechanism"] = json!("accessibility");
            reply["verification"] = json!(acted.verification);
        }
        // The element a referenced click or an action on an element reached,
        // as it was met just before: for the mark on the picture before it.
        if let Some(reached) = &done.reached {
            reply["element"] = json!(reached);
        }

        cancellation.owner = None;
        Ok(reply)
    }

    /// Run a batch as registered work, which owns the engine through its
    /// last effect: a request that vanishes meanwhile cannot end it early.
    /// The batch runs on a blocking thread, its pauses on this runtime.
    /// Once done, what it came to and whether it pressed the Dock; an
    /// action that failed gives its failure instead.
    async fn execute(
        &self,
        grant: Grant,
        actions: Vec<Action>,
        expect: Option<Expect>,
        typed: bool,
        asking: Option<asking::Asking>,
    ) -> Result<Performed> {
        let engine = self.engine.clone().lock_owned().await;
        let gate = self.gate.clone();
        let seen = self.seen.clone();
        let look = self.look.clone();
        // Who the batch was admitted for, kept with it: a Give Back that
        // lands while it ends still finds what the person typed into.
        let person = grant.person().map(str::to_owned);
        let runtime = tokio::runtime::Handle::current();
        let outcome = self
            .operations
            .start(on_engine(engine, move |engine| {
                // Work that keeps input out ends first; a Stop meanwhile is
                // seen as the grant is checked.
                let acting = engine.acting();
                runtime.block_on(async move {
                    drop(grant.authorize()?);
                    engine.grant = Some(grant);
                    engine.target = expect
                        .as_ref()
                        .map(super::ax::target_application)
                        .transpose()?;
                    engine.dock_pressed = false;
                    // An agent's request into an account the person listed is
                    // judged whole, under the engine's lock, before its first
                    // event; the person's own input never is.
                    if let Some(asking) = &asking
                        && person.is_none()
                    {
                        engine.recheck = Some(asking::recheck(&actions, asking));
                    }
                    let result = run(engine, &gate, actions, expect.as_ref()).await;
                    engine.recheck = None;
                    // Part of the batch's own work, so giving control back
                    // waits for it before it reads what was seen.
                    if typed && result.is_ok() {
                        note_password(person.as_deref(), &*look, &seen);
                    }
                    engine.target = None;
                    engine.keep_while_held(acting);
                    Ok::<_, Error>(result.map(|done| (done, engine.dock_pressed)))
                })
            }))
            .await?;
        let (_, performed) = outcome
            .await
            .map_err(|cause| worker_failed().with_source(cause))?;
        performed
    }
}

/// What a batch came to: see [`Service::execute`].
type Performed = std::result::Result<(Done, bool), Box<Failure>>;

/// The recipient for the agent's activity log: Dock after AXPress, the app
/// an action on an element acted in (process `acted`), otherwise the owner
/// of the frontmost ordinary window; with its bundle identifier, read once
/// the engine is free.
async fn landed(dock_pressed: bool, acted: Option<i32>) -> Option<(String, Option<String>)> {
    if dock_pressed {
        return Some(("Dock".to_owned(), Some("com.apple.dock".to_owned())));
    }

    tokio::task::spawn_blocking(move || {
        let windows = super::windows::windows().ok()?;
        let window = windows.into_iter().find(|window| {
            window.layer == 0 && acted.is_none_or(|pid| window.pid == i64::from(pid))
        })?;
        Some((window.owner?, super::apps::bundle_id(window.pid)))
    })
    .await
    .ok()
    .flatten()
}

/// `expected`, when this process's hidden session proves it is the one
/// now: the session, display and process it names.
fn proven(expected: InputEpoch) -> Result<InputEpoch> {
    let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
    if current != expected {
        return Err(Error::session_changed());
    }
    Ok(current)
}

fn hold(
    gate: &Gate,
    stop_marker: &Path,
    on: bool,
    phase: &watch::Sender<ConnectionPhase>,
) -> Result<Value> {
    phase.send_replace(ConnectionPhase::Mutation("hold"));
    let kept = if on {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(stop_marker)
            .map(drop)
    } else {
        match std::fs::remove_file(stop_marker) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    };

    // A resume whose marker stays would come back at the next
    // restart: agents stay stopped until it can be removed.
    if !on && let Err(error) = kept {
        return Err(Error::new(
            Code::Native,
            "Agents could not be resumed: the helper could not clear the stop",
        )
        .with_source(error));
    }

    gate.hold(on)?;
    Ok(json!({ "ok": true, "held": on, "kept": kept.is_ok() }))
}

/// Refuse the person's input that names an agent's target: an expectation,
/// or a referenced element, which brings its own.
fn agent_target(gate: &Gate, expect: Option<&Expect>, actions: &[Action]) -> Result<()> {
    let named = expect.is_some() || actions.iter().any(|action| action.reference().is_some());
    if named && gate.holder()? == Some(Holder::Person) {
        return Err(Error::new(
            Code::Input,
            "The person's input cannot name an agent target",
        ));
    }
    Ok(())
}

/// Check `action`, the next of an agent's request, where it lands now: see
/// [`recheck`].
pub(super) fn recheck_action(engine: &mut Engine, action: &Action) -> Result<()> {
    recheck(engine, |recheck, look| recheck.check(action, look))
}

/// Check the next event of the action under way: see [`recheck`].
pub(super) fn recheck_event(engine: &mut Engine) -> Result<()> {
    recheck(engine, |recheck, look| recheck.event(look))
}

/// Judge the next of an agent's request where it lands now, by `judge`:
/// what the grants it brought don't cover stops there, with the rest of the
/// request, as any refused action does.
fn recheck(
    engine: &mut Engine,
    judge: impl FnOnce(&mut asking::Recheck, &mut asking::live::OnScreen<'_>) -> asking::Decision,
) -> Result<()> {
    let Some(mut recheck) = engine.recheck.take() else {
        return Ok(());
    };
    // What it lands in now, as found: the app the request named stands in
    // for what is in front only in a typing command's check.
    let mut look = asking::live::OnScreen::observing(
        &engine.looks,
        &engine.epoch,
        std::time::Instant::now() + ASK_BUDGET,
        recheck.held.take(),
    );
    let decision = judge(&mut recheck, &mut look);
    recheck.held = look.held;
    engine.recheck = Some(recheck);
    refusal(decision, false)
}

/// How long the gate may take to read where a request lands, in all.
const ASK_BUDGET: Duration = Duration::from_millis(300);

/// Judge `actions` by `asking` on the helper's screen as it is now, a
/// typing command's check before its first character: refused with
/// `waiting-for-person` while the person must be asked, or `denied` in a
/// Look Only account. Nothing is sent either way.
fn gate_request(
    engine: &Engine,
    actions: &[Action],
    asking: &asking::Asking,
    intended: Option<String>,
) -> Result<()> {
    let mut look = asking::live::OnScreen {
        looks: &engine.looks,
        epoch: &engine.epoch,
        deadline: std::time::Instant::now() + ASK_BUDGET,
        intended,
        held: None,
    };
    refusal(asking::decide(actions, asking, &mut look), asking.dry)
}

/// The refusal `decision` is, when it isn't free; `dry` for a typing
/// command's check.
fn refusal(decision: asking::Decision, dry: bool) -> Result<()> {
    match decision {
        asking::Decision::Free => Ok(()),
        asking::Decision::Ask(question) => {
            let mut details = question.details();
            details["dry"] = json!(dry);
            Err(Error::new(
                Code::WaitingForPerson,
                format!(
                    "{} is signed in: Atlas asked the person before sending this, and it wasn't sent.",
                    question.account
                ),
            )
            .with("ask", details))
        }
        asking::Decision::Deny { account } => Err(Error::new(
            Code::Denied,
            format!(
                "{account} is Look Only: the person lets agents read it, not click or type there. \
                 It wasn't sent."
            ),
        )),
    }
}

/// How long the whole look at a password field after the person's typing
/// may take.
const PASSWORD_READ: Duration = Duration::from_millis(250);

/// After the person's keys or typing, admitted for their lease `person`
/// (none for an agent's batch, which is never looked at): when keyboard
/// focus is in a password field, remember its app and page for that lease,
/// in `seen`. One deadline covers the whole look, and nothing typed is read.
fn note_password(
    person: Option<&str>,
    look: &dyn PasswordLook,
    seen: &std::sync::Mutex<Option<(String, Value)>>,
) {
    let Some(owner) = person else {
        return;
    };
    let deadline = std::time::Instant::now() + PASSWORD_READ;
    if let (Some(value), Ok(mut seen)) = (look.look(deadline), seen.lock()) {
        *seen = Some((owner.to_owned(), value));
    }
}

/// Where a password field is looked for: the app with keyboard focus, and
/// the page its focused password field sits in.
pub(super) trait PasswordLook: Send + Sync {
    /// The app and page of the focused password field, by `deadline`; none
    /// when focus is elsewhere or the look doesn't finish in time.
    fn look(&self, deadline: std::time::Instant) -> Option<Value>;
}

/// The Mac's own Accessibility.
pub(super) struct LiveLook;

impl PasswordLook for LiveLook {
    fn look(&self, deadline: std::time::Instant) -> Option<Value> {
        let left = || super::ax::remaining(deadline, std::time::Instant::now()).ok();
        let pid = super::ax::focused_pid(left()?).ok()??;
        let page = super::ax::password_page(pid, left()?).ok()??;
        Some(seen_at(
            &super::apps::bundle_id(pid)?,
            super::apps::name(pid),
            page.as_deref(),
        ))
    }
}

/// What the helper saw: the app by its bundle identifier and name, and the
/// host of the page's `address`, never the address itself.
pub(super) fn seen_at(app: &str, name: Option<String>, address: Option<&str>) -> Value {
    json!({
        "app": app,
        "name": name,
        "host": address.and_then(page_host),
    })
}

/// The host of a web page's address, as accounts are matched by it (see
/// [`atlas_core::name::web_host`]): only for `http` and `https`, with any
/// user, port, path, query and fragment dropped.
pub(super) fn page_host(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !["http", "https"]
        .iter()
        .any(|web| scheme.eq_ignore_ascii_case(web))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']')?.0,
        None => host.split(':').next()?,
    };
    let host = atlas_core::name::web_host(host);
    (!host.is_empty()).then_some(host)
}

/// How often the service looks for its desktop, and how many looks in a
/// row must miss it before the service exits.
const SESSION_CHECK: Duration = Duration::from_secs(1);
const SESSION_GONE_CHECKS: u32 = 3;

/// Serve the configured helper's authenticated input socket until shutdown.
/// The listener, expiry timer, signal receivers and registered work share this
/// lifetime so every exit closes ownership and drains work before cleanup.
///
/// # Errors
///
/// Rejects invalid helper identity, permissions or socket ownership, and returns service
/// setup or shutdown failures. Unknown input and cleanup outcomes remain owned until
/// recovery.
pub async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    let (build, executable) = started_from().unzip();
    atlas_core::session::validate_target_uid(uid, controller_uid)?;

    if controller_uid == 0 {
        return Err(Error::new(
            Code::Identity,
            "The input service must run in a distinct helper account",
        ));
    }

    let mut engine = Engine::new(uid)?;
    let activity = super::activity::beside(&socket);
    engine.activity = Some(activity.clone());
    // Until this process has listed the window server's windows, asking
    // Accessibility which app has keyboard focus fails at once, so the first
    // checked keys would be refused. Listing them only reads; nothing is posted.
    let _ = super::windows::windows();
    let (listener, _endpoint) = super::endpoint::bind(
        &socket,
        uid,
        "input service",
        super::endpoint::Directory::Shared(0o755),
    )
    .await?;

    // Recovery reads and writes the journal, so it runs off the runtime too.
    let journal = socket.with_extension("held.json");
    let engine = tokio::task::spawn_blocking(move || engine.recover(journal).map(|()| engine))
        .await
        .map_err(|cause| worker_failed().with_source(cause))??;

    let stop_marker = socket.with_extension("stopped");
    let gate = Arc::new(Gate::default());
    if stop_marker.exists() {
        gate.hold(true)?;
    }

    let service = Arc::new(Service {
        build: build.flatten(),
        executable,
        stopping: tokio::sync::Notify::new(),
        looks: engine.looks.clone(),
        epoch: Arc::new(RwLock::new(engine.epoch.clone())),
        engine: Arc::new(Mutex::new(engine)),
        operations: Operations::default(),
        gate,
        stop_marker,
        activity,
        seen: Arc::default(),
        look: Arc::new(LiveLook),
    });
    service.listen(&listener, controller_uid).await
}

impl Service {
    /// Engine ownership spans execution and release, both on blocking threads, so
    /// this loop never waits for the journal. Posting briefly takes the gate;
    /// cancellation takes only the gate. Recovery serializes before draining work
    /// and acquiring the engine, never holding a gate guard across that wait.
    async fn listen(self: &Arc<Self>, listener: &UnixListener, controller_uid: u32) -> Result<()> {
        let mut timer = tokio::time::interval(Duration::from_millis(50));

        // Once its login ends (a log-out) the service can never act again, yet
        // would keep the socket from the next login's service: it exits after
        // its desktop has been gone for a few checks in a row.
        let mut session_checks = tokio::time::interval(SESSION_CHECK);
        // A stalled loop must not take its missed looks all at once.
        session_checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut session_gone = 0u32;

        let slots = Arc::new(Semaphore::new(16));
        let mut tasks = JoinSet::new();

        // Both listeners exist before the loop, so a signal that arrives between
        // two passes is still delivered.
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;

        // Every exit path, including an internal failure, closes ownership and
        // releases held input before returning.
        let outcome = loop {
            tokio::select! {
                _ = interrupt.recv() => break Ok(()),
                _ = terminate.recv() => break Ok(()),
                _ = self.stopping.notified() => break Ok(()),
                _ = session_checks.tick() => {
                    session_gone = match super::cf::session() {
                        Some(_) => 0,
                        None => session_gone + 1,
                    };
                    if session_gone >= SESSION_GONE_CHECKS {
                        break Ok(());
                    }
                }
                _ = timer.tick() => {
                    if let Err(error) = self.expire().await { break Err(error); }
                }
                Some(outcome) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                    if let Err(error) = connection_outcome(outcome, &self.operations, false) {
                        eprintln!("{}", error.error);
                    }
                }
                accepted = listener.accept() => {
                    // Accept and peer lookup failures (for example descriptor
                    // exhaustion) concern one connection, not the service.
                    let Ok((stream, _)) = accepted else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    };

                    if let Err(error) = self.connect(stream, controller_uid, &slots, &mut tasks) {
                        break Err(error);
                    }
                }
            }
        };

        let cleanup = finish(&self.gate, &mut tasks, &self.operations, &self.engine).await;
        combine_cleanup(outcome, cleanup)
    }

    /// End a lost controller, and release what it left held as registered
    /// cleanup: shutdown waits for it, and it keeps the engine until it ends,
    /// so no later tick starts another meanwhile.
    async fn expire(&self) -> Result<()> {
        if self.check().is_err() {
            // Only a poisoned gate fails this, and the look below reports it.
            let _ = self.gate.revoke();
        }

        match self.gate.active() {
            Ok(true) => {}
            Ok(false) => {
                // A controller may have acquired (and pressed) since
                // the look above: look again once the engine is held.
                if let Ok(engine) = self.engine.clone().try_lock_owned()
                    && engine.unsettled()
                    && matches!(self.gate.active(), Ok(false))
                {
                    self.operations
                        .begin_clean_up(engine, |engine| {
                            // What stays held stays recorded; a later tick
                            // tries again.
                            let _ = engine.release();
                        })
                        .await?;
                }
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn connect(
        self: &Arc<Self>,
        stream: UnixStream,
        controller_uid: u32,
        slots: &Arc<Semaphore>,
        tasks: &mut JoinSet<Result<()>>,
    ) -> Result<()> {
        if !admitted(&stream, controller_uid) {
            return Ok(());
        }
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            // No dispatch: a definitive refusal keeps the caller's outcome known.
            super::endpoint::turn_away(
                stream,
                &json!(Error::new(
                    Code::Busy,
                    "The input service is serving too many connections; try again",
                )),
            );
            return Ok(());
        };
        let mut connections = self
            .operations
            .connections
            .lock()
            .map_err(|_| worker_failed())?;
        let connected = self.clone();
        let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
        let task = tasks.spawn(async move {
            let _slot = slot;
            handle(stream, connected, phase).await
        });
        connections.insert(task.id(), (observed, false));
        Ok(())
    }
}

/// Whether the peer on `stream` is the controller, the only account the
/// service answers. Nothing Atlas runs in the helper asks the input service
/// anything; an app there (the one under test, an installer) must not cancel
/// the controller's actions, take control or stop the service.
fn admitted(stream: &UnixStream, controller_uid: u32) -> bool {
    stream
        .peer_cred()
        .is_ok_and(|peer| peer.uid() == controller_uid)
}

/// One JSON request of at most `MAX_REQUEST` bytes.
async fn read_request<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Request> {
    let line = super::endpoint::read_line(stream, MAX_REQUEST).await?;
    Request::decode(&line)
}

async fn handle(
    mut stream: UnixStream,
    service: Arc<Service>,
    phase: watch::Sender<ConnectionPhase>,
) -> Result<()> {
    // Nothing was dispatched yet, so answer definitively. A silent hang-up
    // would look like an unacknowledged mutation to the controller.
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(error) => return reply(&mut stream, Err(error)).await,
    };

    let (initial, owner, shutdown) = match &request {
        Request::Hello | Request::Elements { .. } => (ConnectionPhase::Read, None, false),
        Request::Shutdown { .. } => (ConnectionPhase::Refusal, None, true),
        Request::Input { owner, .. } => (ConnectionPhase::Refusal, Some(owner.clone()), false),
        Request::Hold { .. }
        | Request::Release { .. }
        | Request::Renew { .. }
        | Request::Acquire { .. }
        | Request::Unknown { .. }
        | Request::Reset { .. } => (ConnectionPhase::Refusal, None, false),
    };
    phase.send_replace(initial);
    let result = if let Some(owner) = owner {
        let mut dispatch = std::pin::pin!(service.dispatch(request, &phase));
        let mut extra = [0; 1];
        tokio::select! {
            reply = &mut dispatch => reply,
            _ = stream.read(&mut extra) => {
                if service.gate.cancel(Some(&owner)).is_ok() {
                    phase.send_replace(ConnectionPhase::Mutation("input"));
                }
                dispatch.await
            }
        }
    } else {
        service.dispatch(request, &phase).await
    };

    let stopping = shutdown && result.is_ok();
    let written = reply(&mut stream, result).await;

    if stopping {
        service.stopping.notify_one();
    }

    written
}

async fn reply(stream: &mut UnixStream, result: Result<Value>) -> Result<()> {
    let mut bytes = serde_json::to_vec(&result.unwrap_or_else(|e| json!(e)))?;
    bytes.push(b'\n');

    tokio::time::timeout(Duration::from_secs(1), stream.write_all(&bytes))
        .await
        .map_err(|_| Error::new(Code::Transport, "Native reply timed out"))??;

    Ok(())
}

#[cfg(test)]
mod tests;
