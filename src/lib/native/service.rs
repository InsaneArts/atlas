//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
//! The service retains its gate, registered work and engine through cleanup;
//! a vanished request or reply cannot acknowledge an unknown outcome.
use super::{
    accessibility_trusted,
    events::Engine,
    lease::{Gate, Grant, Who, busy},
    session_info,
};
use crate::{
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

use batch::{Failure, Perform, run};
use operations::{
    Cancellation, ConnectionPhase, Operations, combine_cleanup, connection_outcome,
    drain_and_release, finish, reset, worker_failed,
};
use request::Request;

/// Twice the controller's 32 KiB action limit, leaving room for the owner,
/// epoch and JSON re-encoding around any batch the controller accepts.
const MAX_REQUEST: usize = 64 * 1024;

/// What this service can do beyond plain input, as its hello lists it:
/// authenticated shutdown, the person's Stop, and checking an agent's
/// target app, with the Dock covering where it takes clicks, and the window
/// and field it names.
const CAPABILITIES: [&str; 6] = [
    crate::session::SHUTDOWN,
    crate::session::HOLD,
    crate::session::FOCUS_APP,
    crate::session::FOCUS_DOCK,
    crate::session::FOCUS_WINDOW,
    crate::session::FOCUS_FIELD,
];

struct Service {
    engine: Arc<Mutex<Engine>>,
    operations: Operations,
    gate: Arc<Gate>,
    epoch: RwLock<InputEpoch>,
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
    let file = crate::files::open_plain(&path)?;
    let metadata = file.metadata().ok()?;

    let build = crate::digest::sha256(&file);
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
            } => self.input(owner, epoch, actions, expect, app, phase).await,
        }
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
        let mut engine = self.engine.lock().await;
        self.check_uncontrolled()?;

        phase.send_replace(ConnectionPhase::Mutation("shutdown"));
        if let Err(error) = engine.release() {
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
        Ok(json!({ "ok": true, "released": true }))
    }

    fn renew(&self, owner: String, phase: &watch::Sender<ConnectionPhase>) -> Result<Value> {
        self.check()?;
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
        self.check()?;
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
        reset(&self.operations, &self.engine, &self.gate, |engine| {
            let rebound = engine.rebind(current);

            // Enforce whatever epoch the engine now holds, even when its
            // release failed: the two never disagree, and the timer keeps
            // retrying that release against the display it was bound to.
            *self
                .epoch
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
        self.check()?;

        self.check_epoch(expected?)?;
        let mut engine = self.engine.lock().await;
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
        engine.release()?;
        engine.grant = Some(self.gate.acquire(&owner, who)?);

        Ok(json!({ "ok": true }))
    }

    async fn input(
        &self,
        owner: String,
        expected: Result<InputEpoch>,
        actions: Result<Vec<Action>>,
        expect: Result<Option<Expect>>,
        report_app: bool,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        self.check()?;
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
        let expect = expect?;
        agent_target(&self.gate, expect.as_ref())?;
        phase.send_replace(ConnectionPhase::Mutation("input"));
        let mut cancellation = Cancellation {
            gate: self.gate.clone(),
            owner: Some(owner),
        };
        let (field, dock_pressed) = match self.execute(grant, actions, expect.clone()).await? {
            Ok(done) => done,
            Err(failure) => return Ok(failure.reply()),
        };

        let landed = if report_app {
            landed(dock_pressed).await
        } else {
            None
        };
        let (app, bundle_id) = landed.map_or((None, None), |(app, id)| (Some(app), id));

        let mut reply = json!({
            "ok": true,
            "performed": count,
            "app": app,
            "bundleId": bundle_id,
        });
        // Echoed, so the controller knows each action was checked.
        if let Some(expect) = expect {
            reply["expect"] = json!(expect);
        }
        // The role of the field it named, so an agent's activity may name
        // the field by the name it gave: never the field's own text.
        if let Some(role) = field {
            reply["field"] = json!({ "role": role });
        }

        cancellation.owner = None;
        Ok(reply)
    }

    /// Run a batch as registered work, which owns the engine through its
    /// last effect: a request that vanishes meanwhile cannot end it early.
    /// Once done, the role of the field it named and whether it pressed the
    /// Dock; an action that failed gives its failure instead.
    async fn execute(
        &self,
        grant: Grant,
        actions: Vec<Action>,
        expect: Option<Expect>,
    ) -> Result<Performed> {
        let mut engine = self.engine.clone().lock_owned().await;
        let gate = self.gate.clone();
        let outcome = self
            .operations
            .start(async move {
                drop(grant.authorize()?);
                engine.grant = Some(grant);
                engine.target = expect
                    .as_ref()
                    .map(super::ax::target_application)
                    .transpose()?;
                engine.dock_pressed = false;
                let result = run(&mut *engine, &gate, actions, expect.as_ref()).await;
                engine.target = None;
                Ok::<_, Error>(result.map(|field| (field, engine.dock_pressed)))
            })
            .await?;
        outcome
            .await
            .map_err(|cause| worker_failed().with_source(cause))?
    }
}

/// What a batch came to: see [`Service::execute`].
type Performed = std::result::Result<(Option<&'static str>, bool), Box<Failure>>;

/// The recipient for the agent's activity log: Dock after AXPress, otherwise
/// the owner of the frontmost ordinary window and its bundle identifier, read
/// once the engine is free.
async fn landed(dock_pressed: bool) -> Option<(String, Option<String>)> {
    if dock_pressed {
        return Some(("Dock".to_owned(), Some("com.apple.dock".to_owned())));
    }

    tokio::task::spawn_blocking(|| {
        let windows = super::windows::windows().ok()?;
        let window = windows.into_iter().find(|window| window.layer == 0)?;
        Some((window.owner?, super::apps::bundle_id(window.pid)))
    })
    .await
    .ok()
    .flatten()
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

fn agent_target(gate: &Gate, expect: Option<&Expect>) -> Result<()> {
    if expect.is_some() && gate.holder()? == Some(Holder::Person) {
        return Err(Error::new(
            Code::Input,
            "The person's input cannot name an agent target",
        ));
    }
    Ok(())
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
    crate::session::validate_target_uid(uid, controller_uid)?;

    if controller_uid == 0 {
        return Err(Error::new(
            Code::Identity,
            "The input service must run in a distinct helper account",
        ));
    }

    let mut engine = Engine::new(uid)?;
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

    engine.recover(socket.with_extension("held.json"))?;

    let stop_marker = socket.with_extension("stopped");
    let gate = Arc::new(Gate::default());
    if stop_marker.exists() {
        gate.hold(true)?;
    }

    let service = Arc::new(Service {
        build: build.flatten(),
        executable,
        stopping: tokio::sync::Notify::new(),
        epoch: RwLock::new(engine.epoch.clone()),
        engine: Arc::new(Mutex::new(engine)),
        operations: Operations::default(),
        gate,
        stop_marker,
    });
    service.listen(&listener, controller_uid).await
}

impl Service {
    /// Engine ownership spans execution and release. Posting briefly takes the gate;
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
                    if let Err(error) = self.expire() { break Err(error); }
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

    fn expire(&self) -> Result<()> {
        if self.check().is_err() {
            // Only a poisoned gate fails this, and the look below reports it.
            let _ = self.gate.revoke();
        }

        match self.gate.active() {
            Ok(true) => {}
            Ok(false) => {
                // A controller may have acquired (and pressed) since
                // the look above: look again once the engine is held.
                if let Ok(mut engine) = self.engine.try_lock()
                    && matches!(self.gate.active(), Ok(false))
                {
                    // What stays held stays recorded; the next tick tries again.
                    let _ = engine.release();
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
        Request::Hello => (ConnectionPhase::Read, None, false),
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
