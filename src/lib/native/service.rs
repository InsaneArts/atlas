//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
use super::{
    events::Engine,
    focus::{self, Need},
    lease::{Gate, Who},
    session_info,
    windows::Bounds,
};
use crate::{
    actions::{self, Action, Expect},
    error::{Code, Error, Holder, Result},
    session::{Display, InputEpoch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Mutex as StdMutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{SignalKind, signal},
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

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

/// Request futures can disappear while AX is in flight. Registered tasks
/// own the engine guard and always join their blocking child before leaving.
#[derive(Default)]
struct Operations {
    tasks: StdMutex<Vec<Arc<Mutex<Work>>>>,
    failed: AtomicBool,
    recovery: Mutex<()>,
}

struct Work {
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Operations {
    async fn start<T: Send + 'static>(
        &self,
        work: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<tokio::sync::oneshot::Receiver<T>> {
        self.reap().await?;
        let mut tasks = self.tasks.lock().map_err(|_| worker_failed())?;
        self.check()?;
        let (send, receive) = tokio::sync::oneshot::channel();
        let (start, registered) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if registered.await.is_err() {
                return;
            }
            let result = work.await;
            let _ = send.send(result);
        });
        tasks.push(Arc::new(Mutex::new(Work { task: Some(task) })));
        drop(tasks);
        // AX cannot start before registration. No await separates this
        // handoff from registration, even if the request is aborted.
        let _ = start.send(());
        Ok(receive)
    }

    async fn join(&self, work: &mut Work) {
        // Await by reference: dropping cleanup leaves the handle registered.
        if let Some(task) = &mut work.task {
            self.failed.fetch_or(task.await.is_err(), Ordering::Relaxed);
            work.task = None;
        }
    }

    fn remove_joined(&self) -> Result<bool> {
        let mut tasks = self.tasks.lock().map_err(|_| worker_failed())?;
        tasks.retain(|task| task.try_lock().map_or(true, |work| work.task.is_some()));
        Ok(tasks.is_empty())
    }

    async fn reap(&self) -> Result<()> {
        let tasks = self.tasks.lock().map_err(|_| worker_failed())?.clone();
        for task in tasks {
            if let Ok(mut work) = task.try_lock()
                && work.task.as_ref().is_some_and(|task| task.is_finished())
            {
                self.join(&mut work).await;
            }
        }
        self.remove_joined()?;
        Ok(())
    }

    fn check(&self) -> Result<()> {
        if self.failed.load(Ordering::Relaxed) {
            Err(worker_failed())
        } else {
            Ok(())
        }
    }

    async fn join_all(&self) -> Result<()> {
        loop {
            let tasks = self.tasks.lock().map_err(|_| worker_failed())?.clone();
            for task in tasks {
                let mut work = task.lock().await;
                self.join(&mut work).await;
            }
            if self.remove_joined()? {
                return Ok(());
            }
        }
    }

    async fn drain(&self) -> Result<()> {
        self.join_all().await?;
        self.check()
    }
}

fn worker_failed() -> Error {
    Error::new(
        Code::Uncertain,
        "The input worker did not report its outcome",
    )
    .uncertain()
}

/// Cancelling a request changes ownership without waiting for the engine.
struct Cancellation {
    gate: Arc<Gate>,
    owner: String,
    armed: bool,
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.gate.cancel(Some(&self.owner));
        }
    }
}

async fn drain_and_release(
    operations: &Operations,
    engine: &Mutex<impl Perform>,
    gate: &Gate,
) -> Result<()> {
    let drained = operations.drain().await;
    let mut engine = engine.lock().await;
    let released = if !gate.active()? {
        engine.release()
    } else {
        Ok(())
    };
    drained.and(released)
}

/// Only explicit recovery can settle an unknown worker outcome. If recovery
/// is cancelled or any step fails, input stays blocked until another reset.
async fn reset<E>(
    operations: &Operations,
    engine: &Mutex<E>,
    gate: &Gate,
    rebind: impl FnOnce(&mut E) -> Result<()>,
) -> Result<()> {
    let _recovery = operations.recovery.lock().await;
    operations.failed.store(true, Ordering::Relaxed);
    gate.cancel(None)?;
    operations.join_all().await?;
    let mut engine = engine.lock().await;
    // An acquire admitted just before recovery may have held the engine
    // while the first cancellation ran. Cancel it before cleanup as well.
    gate.cancel(None)?;
    rebind(&mut engine)?;
    operations.failed.store(false, Ordering::Relaxed);
    Ok(())
}

/// Every service exit uses the same close, request cancellation and worker
/// drain boundary, including TERM and internal failures.
async fn finish(
    gate: &Gate,
    requests: &mut JoinSet<()>,
    operations: &Operations,
    engine: &Mutex<impl Perform>,
) -> Result<()> {
    let closed = gate.close();
    requests.abort_all();
    while requests.join_next().await.is_some() {}
    let cleanup = drain_and_release(operations, engine, gate).await;
    closed.and(cleanup)
}

/// The operations the input service answers, by their `op` string.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Hello,
    Shutdown,
    Hold,
    Release,
    Reset,
    Renew,
    Acquire,
    Input,
    Unknown,
}

impl Operation {
    fn of(request: &Value) -> Self {
        match request["op"].as_str().unwrap_or_default() {
            "hello" => Self::Hello,
            "shutdown" => Self::Shutdown,
            "hold" => Self::Hold,
            "release" => Self::Release,
            "reset" => Self::Reset,
            "renew" => Self::Renew,
            "acquire" => Self::Acquire,
            "input" => Self::Input,
            _ => Self::Unknown,
        }
    }
}

/// A file by path and (device, inode).
type Executable = (PathBuf, (u64, u64));

/// The started-from file's SHA-256, and its path with its (device, inode).
/// Both come from one open descriptor, so they describe the same file even
/// if the path is replaced meanwhile.
fn started_from() -> (Option<String>, Option<Executable>) {
    use std::os::unix::fs::MetadataExt;

    // Started through the shared link, the executable is the file it names.
    let Some(path) = std::env::current_exe().and_then(std::fs::canonicalize).ok() else {
        return (None, None);
    };

    let Some((file, metadata)) = crate::files::open_plain(&path)
        .and_then(|file| file.metadata().ok().map(|metadata| (file, metadata)))
    else {
        return (None, None);
    };

    let build = crate::digest::sha256(&file);
    (build, Some((path, (metadata.dev(), metadata.ino()))))
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
            || !Engine::trusted()
        {
            return Err(Error::new(
                Code::Identity,
                "The input session or permission changed; release control",
            ));
        }

        Ok(())
    }

    async fn cleanup(&self) -> Result<()> {
        drain_and_release(&self.operations, &self.engine, &self.gate).await
    }

    async fn dispatch(&self, request: &Value) -> Result<Value> {
        let owner = request["owner"].as_str().unwrap_or_default();
        let operation = Operation::of(request);
        if operation == Operation::Hello {
            self.operations.reap().await?;
            let held_input = self
                .engine
                .try_lock()
                .ok()
                .map(|engine| engine.held_input());

            return Ok(json!({
                "ok": true,
                "protocolVersion": 1,
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
                "build": self.build,
                "replaced": self.replaced(),
                "capabilities": CAPABILITIES,
                "accessibility": Engine::trusted(),
                "ready": self.check().is_ok(),
                "enforcedEpoch": self.epoch()?,
                "session": session_info()?,
                "controlActive": self.gate.active()?,
                "held": self.gate.held()?,
                "heldInput": held_input,
                "streaming": true,
            }));
        }

        if operation == Operation::Shutdown {
            // Bound to this process: a replacement that has since bound the same
            // socket never honours a request meant for its predecessor.
            if request["pid"].as_u64() != Some(std::process::id().into()) {
                return Err(Error::new(
                    Code::Identity,
                    "Shutdown names a different input service process",
                ));
            }

            if self.gate.active()? {
                return Err(Error::new(
                    Code::Busy,
                    "The input service has an active controller",
                ));
            }

            self.operations.drain().await?;

            // Release held input first; only an acknowledged release closes the
            // gate for good. Holding the engine lock keeps acquire out meanwhile,
            // and one that took the lock first is seen once it is held.
            let mut engine = self.engine.lock().await;
            if self.gate.active()? {
                return Err(Error::new(
                    Code::Busy,
                    "The input service has an active controller",
                ));
            }

            if let Err(error) = engine.release() {
                return Err(Error::new(
                    Code::Uncertain,
                    format!("Held input could not be released: {}", error.error),
                )
                .uncertain());
            }

            self.gate.close()?;
            return Ok(json!({ "ok": true, "stopping": true, "pid": std::process::id() }));
        }

        // A stop is the person's: it works whatever the session or
        // permission, and resuming only lets agents ask again.
        if operation == Operation::Hold {
            let on = request["on"] == true;
            let kept = if on {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(false)
                    .mode(0o644)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&self.stop_marker)
                    .map(drop)
            } else {
                match std::fs::remove_file(&self.stop_marker) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                    _ => Ok(()),
                }
            };

            // A resume whose marker stays would come back at the next
            // restart: agents stay stopped until it can be removed.
            if !on && kept.is_err() {
                return Err(Error::new(
                    Code::Native,
                    "Agents could not be resumed: the helper could not clear the stop",
                ));
            }

            self.gate.hold(on)?;
            self.cleanup().await?;
            return Ok(json!({ "ok": true, "held": on, "kept": kept.is_ok() }));
        }

        if operation == Operation::Release {
            self.gate.cancel(Some(owner))?;
            self.cleanup().await?;
            return Ok(json!({ "ok": true, "released": true }));
        }

        if operation == Operation::Reset {
            let expected = InputEpoch::deserialize(&request["epoch"])?;
            let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
            if expected != current || !Engine::trusted() {
                return Err(Error::new(
                    Code::Identity,
                    "Recovery requires the current hidden session and permission",
                ));
            }

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
            return Ok(json!({ "ok": true, "released": true }));
        }

        self.check()?;

        if operation == Operation::Renew {
            self.gate.renew(owner)?;
            return Ok(json!({ "ok": true }));
        }

        let expected = InputEpoch::deserialize(&request["epoch"])?;
        let epoch = self.epoch()?;
        if expected != epoch {
            return Err(Error::new(
                Code::SessionChanged,
                "Input coordinates belong to a different session or display",
            ));
        }

        if operation == Operation::Acquire {
            let mut engine = self.engine.lock().await;
            self.operations.check()?;

            if let Some(holder) = self.gate.holder()? {
                // An agent finding the person in control is told to wait.
                let agent = request["person"] != true;
                let error = Error::new(
                    Code::Busy,
                    if agent && holder == Holder::Person {
                        crate::error::PERSON_HAS_CONTROL
                    } else {
                        "The native input service already has a controller"
                    },
                );
                return Err(error.held_by(holder));
            }

            engine.check()?;
            engine.release()?;
            let who = if request["person"] == true {
                Who::Person
            } else {
                Who::Agent
            };
            engine.grant = Some(self.gate.acquire(owner, who)?);

            return Ok(json!({ "ok": true }));
        }

        if operation != Operation::Input {
            return Err(Error::new(Code::Input, "Unknown native operation"));
        }

        let grant = self.gate.grant(owner)?;
        let actions = actions::parse(
            &request["actions"],
            &Display {
                width: epoch.width,
                height: epoch.height,
                scale: 1.0,
            },
        )?;
        // Refused before anything is sent: an expectation this build cannot
        // check is never ignored.
        let expect = Expect::from_request(&request["expect"])?;
        agent_target(&self.gate, expect.as_ref())?;
        let mut cancellation = Cancellation {
            gate: self.gate.clone(),
            owner: owner.to_owned(),
            armed: true,
        };
        let mut engine = self.engine.clone().lock_owned().await;
        let gate = self.gate.clone();
        let expected = expect.clone();
        let outcome = self
            .operations
            .start(async move {
                drop(grant.authorize()?);
                engine.grant = Some(grant);
                engine.target = expected
                    .as_ref()
                    .map(super::ax::target_application)
                    .transpose()?;
                let result = run(&mut *engine, &gate, actions, expected.as_ref()).await;
                engine.target = None;
                Ok::<_, Error>(result)
            })
            .await?;
        let result = outcome.await.map_err(|_| worker_failed())?;
        let field = match result? {
            Ok(field) => field,
            Err(reply) => return Ok(reply),
        };

        // The app the input landed in, for an agent's activity log (only
        // when asked, so a person's own control pays nothing): the owner of
        // the frontmost ordinary window and its bundle identifier, read once
        // the engine is free.
        let landed = if request["app"] == true {
            tokio::task::spawn_blocking(|| {
                frontmost_app().map(|(app, pid)| (app, super::apps::bundle_id(pid)))
            })
            .await
            .ok()
            .flatten()
        } else {
            None
        };
        let (app, bundle_id) = landed.map_or((None, None), |(app, id)| (Some(app), id));

        let mut reply = json!({
            "ok": true,
            "performed": request["actions"].as_array().map(Vec::len),
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

        cancellation.armed = false;
        Ok(reply)
    }
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

/// What the input loop needs of the event engine; tests stand in for it.
trait Perform {
    /// Refuse unless `expect` holds for `need` now, before anything is
    /// posted, taking at most `budget` to tell; once it does, the role of
    /// the field it names, when it names one.
    async fn require(
        &mut self,
        expect: &Expect,
        need: Need,
        budget: Duration,
        prepare: bool,
    ) -> Result<Option<&'static str>>;
    /// Carry out one action, posting its events.
    async fn perform(&mut self, action: Action) -> Result<()>;
    /// Events posted so far.
    fn posted(&self) -> u64;
    fn held_input(&self) -> bool;
    /// Let go of every key and button still held.
    fn release(&mut self) -> Result<()>;
}

impl Perform for Engine {
    /// The reads block, so they run on a blocking thread; the engine stays
    /// locked by the caller meanwhile, so nothing posts in between.
    async fn require(
        &mut self,
        expect: &Expect,
        need: Need,
        budget: Duration,
        prepare: bool,
    ) -> Result<Option<&'static str>> {
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let deadline = Instant::now() + budget;
        let expect = expect.clone();
        self.check()?;
        let guard = self.preparation_guard();
        checked_preparation(
            move || {
                if prepare {
                    focus::prepare(&expect, need, &display, deadline, guard)
                } else {
                    guard()?;
                    let result = focus::check(&expect, need, &display, deadline);
                    guard()?;
                    result
                }
            },
            || {
                self.check().and_then(|()| {
                    super::ax::check_application(self.target)?;
                    drop(self.grant.as_ref().ok_or_else(worker_failed)?.authorize()?);
                    Ok(())
                })
            },
        )
        .await
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
        Engine::perform(self, action).await
    }

    fn posted(&self) -> u64 {
        self.posted
    }

    fn held_input(&self) -> bool {
        Engine::held_input(self)
    }

    fn release(&mut self) -> Result<()> {
        Engine::release(self)
    }
}

async fn preparation_worker<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|_| Err(worker_failed()))
}

async fn checked_preparation<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
    after: impl FnOnce() -> Result<()>,
) -> Result<T> {
    let result = preparation_worker(work).await;
    let after = after();
    match result {
        Err(error) if error.uncertain || matches!(error.code, Code::Stopped | Code::Cancelled) => {
            Err(error)
        }
        result => {
            after?;
            result
        }
    }
}

/// Carry out `actions` in order on `engine`, which the caller holds
/// locked, each checked against `expect` just before it starts: a key
/// after the batch's first against the app alone, since an earlier key
/// (tab, return, cmd+w) may move the window or field on purpose. On the
/// first failure, revoke `gate`, release what is held and return the
/// failure reply: what was performed, whether the failing action had posted
/// anything, and whether the outcome is unknown. Once all are done, the
/// role of the field `expect` names, when every check of it found the same.
async fn run(
    engine: &mut impl Perform,
    gate: &Gate,
    actions: Vec<Action>,
    expect: Option<&Expect>,
) -> std::result::Result<Option<&'static str>, Value> {
    let mut budget = focus::PREPARE_BUDGET;
    let later = expect.map(Expect::app_only);
    let mut pressed = false;
    let mut roles = BTreeSet::new();

    for (performed, action) in actions.into_iter().enumerate() {
        let before = engine.posted();
        let key = matches!(action, Action::Key { .. });
        let continuation = key && pressed;
        let expected = if continuation { later.as_ref() } else { expect };
        pressed |= key;
        let checked = match expected {
            Some(expected) => {
                let prepare = !continuation && !engine.held_input();
                require_within(engine, expected, Need::of(&action), &mut budget, prepare).await
            }
            None => Ok(None),
        };
        let step = match checked {
            Ok(role) => {
                roles.extend(role);
                engine.perform(action).await
            }
            Err(error) => Err(error),
        };

        if let Err(error) = step {
            let _ = gate.revoke();
            return Err(failed(engine, error, performed, before));
        }
    }

    let mut roles = roles.into_iter();
    Ok(match (roles.next(), roles.next()) {
        (Some(role), None) => Some(role),
        _ => None,
    })
}

/// [`Perform::require`], spending what it takes of `budget`: the time one
/// request may spend checking in all.
async fn require_within(
    engine: &mut impl Perform,
    expect: &Expect,
    need: Need,
    budget: &mut Duration,
    prepare: bool,
) -> Result<Option<&'static str>> {
    let started = Instant::now();
    let outcome = if need == Need::Nothing {
        Ok(None)
    } else {
        let available = if prepare {
            *budget
        } else {
            (*budget).min(focus::CHECK_BUDGET)
        };
        engine.require(expect, need, available, prepare).await
    };
    *budget = budget.saturating_sub(started.elapsed());
    outcome
}

/// The reply for an action that failed with `error` once `performed`
/// actions had finished and `before` events had been posted, after
/// releasing what is held.
fn failed(engine: &mut impl Perform, error: Error, performed: usize, before: u64) -> Value {
    // An action that posted some of its events before failing (a few typed
    // characters, one click of two) had part of its effect, and nobody can
    // say which part. A refused one posted nothing.
    let partial = engine.posted() > before;
    let cleanup = engine.release();

    let mut reply = json!(error);
    reply["performed"] = json!(performed);
    reply["partial"] = json!(partial);

    // A viewer's cancel is the person's doing, watching what landed; a stop
    // (the helper hidden) or any other break mid-action leaves that unknown.
    let unknown = partial && error.code != Code::Cancelled;
    reply["uncertain"] = json!(cleanup.is_err() || unknown || error.uncertain);

    if let Err(error) = cleanup {
        reply["cleanupError"] = json!(error.error);
    }

    reply
}

/// The name and process of the app owning the frontmost ordinary window,
/// if any.
fn frontmost_app() -> Option<(String, i64)> {
    super::windows::windows()
        .ok()?
        .into_iter()
        .find(|window| window.layer == 0)
        .and_then(|window| Some((window.owner?, window.pid)))
}

/// How often the service looks for its desktop, and how many looks in a
/// row must miss it before the service exits.
const SESSION_CHECK: Duration = Duration::from_secs(1);
const SESSION_GONE_CHECKS: u32 = 3;

pub async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    let (build, executable) = started_from();
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
        build,
        executable,
        stopping: tokio::sync::Notify::new(),
        epoch: RwLock::new(engine.epoch.clone()),
        engine: Arc::new(Mutex::new(engine)),
        operations: Operations::default(),
        gate,
        stop_marker,
    });
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
            _ = service.stopping.notified() => break Ok(()),
            _ = session_checks.tick() => {
                session_gone = if super::cf::session().is_none() { session_gone + 1 } else { 0 };
                if session_gone >= SESSION_GONE_CHECKS {
                    break Ok(());
                }
            }
            _ = timer.tick() => {
                if service.check().is_err() {
                    let _ = service.gate.revoke();
                }

                match service.gate.active() {
                    Ok(true) => {}
                    Ok(false) => {
                        // A controller may have acquired (and pressed) since
                        // the look above: look again once the engine is held.
                        if let Ok(mut engine) = service.engine.try_lock()
                            && matches!(service.gate.active(), Ok(false))
                        {
                            let _ = engine.release();
                        }
                    }
                    Err(error) => break Err(error),
                }
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            accepted = listener.accept() => {
                // Accept and peer lookup failures (for example descriptor
                // exhaustion) concern one connection, not the service.
                let Ok((stream, _)) = accepted else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                };

                if !admitted(&stream, controller_uid) {
                    continue;
                }

                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    // Nothing was dispatched: say so, rather than hang up in
                    // a way the controller must treat as uncertain.
                    super::endpoint::turn_away(stream, &json!(Error::new(
                        Code::Busy,
                        "The input service is serving too many connections; try again",
                    )));
                    continue;
                };

                let service = service.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let _ = handle(stream, service).await;
                });
            }
        }
    };

    let cleanup = finish(
        &service.gate,
        &mut tasks,
        &service.operations,
        &service.engine,
    )
    .await;
    outcome.and(cleanup)
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
async fn read_request<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Value> {
    let line = super::endpoint::read_line(stream, MAX_REQUEST).await?;
    Ok(serde_json::from_slice(&line)?)
}

async fn handle(mut stream: UnixStream, service: Arc<Service>) -> Result<()> {
    // Nothing was dispatched yet, so answer definitively. A silent hang-up
    // would look like an unacknowledged mutation to the controller.
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(error) => return reply(&mut stream, Err(error)).await,
    };

    let operation = Operation::of(&request);
    let result = if operation == Operation::Input {
        let mut dispatch = std::pin::pin!(service.dispatch(&request));
        let mut extra = [0; 1];
        tokio::select! {
            reply = &mut dispatch => reply,
            _ = stream.read(&mut extra) => {
                if let Some(owner) = request["owner"].as_str() {
                    let _ = service.gate.cancel(Some(owner));
                }
                dispatch.await
            }
        }
    } else {
        service.dispatch(&request).await
    };

    let stopping = operation == Operation::Shutdown && result.is_ok();
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
mod tests {
    use super::{
        Action, CAPABILITIES, Duration, Error, Expect, Gate, MAX_REQUEST, Need, Perform, Result,
        admitted, read_request, run,
    };
    use crate::error::Code;
    use serde_json::{Value, json};
    use std::sync::{Arc, Weak};
    use tokio::{io::AsyncWriteExt, net::UnixStream, sync::Mutex};

    /// What a stand-in engine does at one action.
    #[derive(Clone, Copy, PartialEq)]
    enum Step {
        Post,
        Refuse,
        Unreadable,
        /// Post one event, then fail.
        Break,
    }

    /// An engine that logs each check and post, and does at each action
    /// what its script says. It holds a handle to the lock it lives in, to
    /// see that the lock is held whenever it is asked anything.
    struct Stand {
        script: Vec<Step>,
        log: Vec<String>,
        /// What each check was asked to find.
        asked: Vec<Expect>,
        /// The role each check of a field finds it of, a text field's past
        /// the list.
        roles: Vec<&'static str>,
        preparations: Vec<bool>,
        budgets: Vec<Duration>,
        check_delay: Duration,
        held: bool,
        sheet: bool,
        opens_sheet: bool,
        posted: u64,
        releases: usize,
        release_fails: bool,
        lock: Weak<Mutex<Stand>>,
    }

    impl Stand {
        fn locked(script: &[Step]) -> Arc<Mutex<Self>> {
            Arc::new_cyclic(|lock| {
                Mutex::new(Self {
                    script: script.to_vec(),
                    log: Vec::new(),
                    asked: Vec::new(),
                    roles: Vec::new(),
                    preparations: Vec::new(),
                    budgets: Vec::new(),
                    check_delay: Duration::ZERO,
                    held: false,
                    sheet: false,
                    opens_sheet: false,
                    posted: 0,
                    releases: 0,
                    release_fails: false,
                    lock: lock.clone(),
                })
            })
        }

        fn assert_locked(&self) {
            let lock = self.lock.upgrade().unwrap();
            assert!(lock.try_lock().is_err(), "asked outside the engine lock");
        }

        /// How many lines of the log start with `what`.
        fn logged(&self, what: &str) -> usize {
            self.log
                .iter()
                .filter(|line| line.starts_with(what))
                .count()
        }
    }

    impl Perform for Stand {
        async fn require(
            &mut self,
            expect: &Expect,
            _: Need,
            budget: Duration,
            prepare: bool,
        ) -> Result<Option<&'static str>> {
            self.assert_locked();
            self.preparations.push(prepare);
            self.budgets.push(budget);
            if !self.check_delay.is_zero() {
                tokio::time::sleep(self.check_delay).await;
            }
            if prepare && self.sheet {
                return Err(Error::new(Code::FocusChanged, "A sheet blocks the window")
                    .with("reason", "sheet"));
            }
            self.asked.push(expect.clone());
            let index = self.logged("check");
            self.log.push(format!("check {index}"));
            if self.script.get(index) == Some(&Step::Refuse) {
                return Err(
                    Error::new(Code::FocusChanged, "Calculator has keyboard focus")
                        .with("reason", "app")
                        .with("found", json!({"app": "Calculator", "pid": 812})),
                );
            }
            if self.script.get(index) == Some(&Step::Unreadable) {
                return Err(super::focus::unreadable());
            }
            Ok(expect
                .field()
                .map(|_| self.roles.get(index).copied().unwrap_or("AXTextField")))
        }

        async fn perform(&mut self, _: Action) -> Result<()> {
            self.assert_locked();
            let index = self.logged("post");
            self.log.push(format!("post {index}"));
            self.posted += 1;
            if self.opens_sheet {
                self.sheet = true;
            }
            match self.script.get(index) {
                Some(Step::Break) => Err(Error::new(Code::Identity, "The session changed")),
                _ => Ok(()),
            }
        }

        fn posted(&self) -> u64 {
            self.posted
        }

        fn held_input(&self) -> bool {
            self.held
        }

        fn release(&mut self) -> Result<()> {
            self.releases += 1;
            if self.release_fails {
                return Err(
                    Error::new(Code::Uncertain, "Held input could not be released").uncertain(),
                );
            }
            self.held = false;
            Ok(())
        }
    }

    fn keys(count: usize) -> Vec<Action> {
        (0..count)
            .map(|_| serde_json::from_value(json!({"type": "key", "key": "a"})).unwrap())
            .collect()
    }

    /// Run `count` key actions on a stand-in following `script`, under
    /// its lock, with or without an expectation.
    async fn ran(
        script: &[Step],
        count: usize,
        expect: Option<Expect>,
    ) -> (
        std::result::Result<Option<&'static str>, Value>,
        Arc<Mutex<Stand>>,
    ) {
        let stand = Stand::locked(script);
        let gate = Arc::new(Gate::default());
        let outcome = {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, keys(count), expect.as_ref()).await
        };
        (outcome, stand)
    }

    fn app() -> Option<Expect> {
        Some(Expect::new(812).unwrap())
    }

    #[tokio::test]
    async fn the_target_is_checked_under_the_engine_lock_just_before_each_action() {
        let (outcome, stand) = ran(&[Step::Post, Step::Post], 2, app()).await;
        assert!(outcome.is_ok());
        assert_eq!(
            stand.lock().await.log,
            ["check 0", "post 0", "check 1", "post 1"]
        );
    }

    #[tokio::test]
    async fn a_mismatch_posts_nothing_and_is_not_uncertain() {
        let (outcome, stand) = ran(&[Step::Refuse], 2, app()).await;
        let reply = outcome.unwrap_err();
        assert_eq!(reply["code"], "focus-changed");
        assert_eq!(reply["reason"], "app");
        assert_eq!(reply["found"], json!({"app": "Calculator", "pid": 812}));
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], false);

        let stand = stand.lock().await;
        assert_eq!(stand.log, ["check 0"], "nothing was performed");
        assert_eq!(stand.posted, 0);
        assert_eq!(stand.releases, 1, "what was held is released");
    }

    #[tokio::test]
    async fn a_mismatch_after_earlier_actions_says_how_many_were_sent() {
        let (outcome, stand) = ran(&[Step::Post, Step::Post, Step::Refuse], 4, app()).await;
        let reply = outcome.unwrap_err();
        assert_eq!(reply["code"], "focus-changed");
        assert_eq!(reply["performed"], 2);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], false);
        assert_eq!(stand.lock().await.posted, 2);
    }

    #[tokio::test]
    async fn an_unreadable_first_check_posts_nothing_and_is_not_uncertain() {
        for action in [
            Action::Type {
                text: "hello".to_owned(),
            },
            Action::Key {
                key: "a".to_owned(),
            },
        ] {
            let stand = Stand::locked(&[Step::Unreadable]);
            let gate = Arc::new(Gate::default());
            let grant = gate
                .acquire(&crate::files::token(), super::Who::Agent)
                .unwrap();
            assert!(gate.active().unwrap());

            let mut engine = stand.lock().await;
            assert_eq!(engine.posted, 0);
            let reply = run(&mut *engine, &gate, vec![action], app().as_ref())
                .await
                .unwrap_err();
            assert_eq!(reply["code"], "focus-changed");
            assert_eq!(reply["reason"], "unreadable");
            assert_eq!(reply["found"], Value::Null);
            assert_eq!(reply["performed"], 0);
            assert_eq!(reply["partial"], false);
            assert_eq!(reply["uncertain"], false);
            assert_eq!(engine.preparations, [true]);
            assert_eq!(engine.log, ["check 0"]);
            assert_eq!(engine.posted, 0);
            assert_eq!(engine.releases, 1);
            assert!(!gate.active().unwrap());
            drop(grant);
        }
    }

    #[tokio::test]
    async fn an_unreadable_later_check_does_not_repeat_earlier_input() {
        let stand = Stand::locked(&[Step::Post, Step::Unreadable]);
        let gate = Arc::new(Gate::default());
        let grant = gate
            .acquire(&crate::files::token(), super::Who::Agent)
            .unwrap();
        assert!(gate.active().unwrap());

        let mut engine = stand.lock().await;
        let reply = run(&mut *engine, &gate, keys(2), app().as_ref())
            .await
            .unwrap_err();
        assert_eq!(reply["code"], "focus-changed");
        assert_eq!(reply["reason"], "unreadable");
        assert_eq!(reply["found"], Value::Null);
        assert_eq!(reply["performed"], 1);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["uncertain"], false);
        assert_eq!(engine.preparations, [true, false]);
        assert_eq!(engine.log, ["check 0", "post 0", "check 1"]);
        assert_eq!(engine.posted, 1);
        assert_eq!(engine.releases, 1);
        assert!(!gate.active().unwrap());
        drop(grant);
    }

    #[tokio::test]
    async fn an_action_that_fails_partway_stays_uncertain() {
        let (outcome, _) = ran(&[Step::Break], 2, app()).await;
        let reply = outcome.unwrap_err();
        assert_eq!(reply["code"], "identity");
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], true);
        assert_eq!(reply["uncertain"], true);
    }

    /// Run `actions` on a stand-in that posts each, expecting `expect`.
    async fn ran_actions(actions: Vec<Value>, expect: &Expect) -> Arc<Mutex<Stand>> {
        let stand = Stand::locked(&[]);
        let gate = Arc::new(Gate::default());
        let actions = actions
            .into_iter()
            .map(|action| serde_json::from_value(action).unwrap())
            .collect();
        {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, actions, Some(expect))
                .await
                .unwrap();
        }
        stand
    }

    /// The window and field each check was asked for.
    fn asked_for(stand: &Stand) -> Vec<(Option<i64>, Option<String>)> {
        stand
            .asked
            .iter()
            .map(|expect| (expect.window(), expect.field().map(str::to_owned)))
            .collect()
    }

    #[tokio::test]
    async fn keys_are_checked_once_before_the_first_key() {
        let expect = Expect::new(812)
            .unwrap()
            .in_window(4120)
            .unwrap()
            .in_field("Email")
            .unwrap();
        let key = |key: &str| json!({"type": "key", "key": key});
        let stand = ran_actions(vec![key("tab"), key("a"), key("return")], &expect).await;
        let stand = stand.lock().await;
        assert_eq!(
            asked_for(&stand),
            [
                (Some(4120), Some("Email".to_owned())),
                (None, None),
                (None, None)
            ],
            "the app before every key, the window and field before the first"
        );
        assert!(stand.asked.iter().all(|asked| asked.pid() == 812));
    }

    #[tokio::test]
    async fn text_is_checked_whole_before_every_part() {
        let expect = Expect::new(812).unwrap().in_field("Email").unwrap();
        let part = |text: &str| json!({"type": "type", "text": text});
        let stand = ran_actions(vec![part("sam@"), part("example"), part(".com")], &expect).await;
        assert_eq!(
            asked_for(&*stand.lock().await),
            vec![(None, Some("Email".to_owned())); 3]
        );
    }

    /// Type `parts` on a stand-in whose checks find the field of each of
    /// `roles` in turn, expecting the field named Email.
    async fn typed_into(parts: &[&str], roles: &[&'static str]) -> Option<&'static str> {
        let stand = Stand::locked(&[]);
        stand.lock().await.roles = roles.to_vec();
        let gate = Arc::new(Gate::default());
        let expect = Expect::new(812).unwrap().in_field("Email").unwrap();
        let actions = parts
            .iter()
            .map(|text| serde_json::from_value(json!({"type": "type", "text": text})).unwrap())
            .collect();
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, actions, Some(&expect))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn typing_into_a_named_field_says_the_role_every_check_found_it_of() {
        assert_eq!(
            typed_into(&["sam@", "example"], &["AXSearchField", "AXSearchField"]).await,
            Some("AXSearchField")
        );
        assert_eq!(
            typed_into(&["sam@", "example"], &["AXTextField", "AXTextArea"]).await,
            None,
            "checks that disagree name no role"
        );

        let (unnamed, _) = ran(&[Step::Post], 1, app()).await;
        assert_eq!(unnamed.unwrap(), None, "a check of no field says none");
        let (refused, _) = ran(
            &[Step::Refuse],
            1,
            Some(Expect::new(812).unwrap().in_field("Email").unwrap()),
        )
        .await;
        assert!(refused.is_err(), "a refusal is a reply of its own");
    }

    #[tokio::test]
    async fn input_without_an_expectation_is_never_checked() {
        let (outcome, stand) = ran(&[Step::Refuse, Step::Refuse], 2, None).await;
        assert!(outcome.is_ok());
        assert_eq!(stand.lock().await.log, ["post 0", "post 1"]);
    }

    #[test]
    fn the_input_service_offers_the_target_check() {
        assert!(CAPABILITIES.contains(&crate::session::FOCUS_APP));
        assert!(CAPABILITIES.contains(&crate::session::FOCUS_DOCK));
        assert_eq!(crate::session::FOCUS_APP, "focus-app");
        assert_eq!(crate::session::FOCUS_DOCK, "focus-dock");
    }

    #[test]
    fn the_input_service_offers_the_window_and_field_checks() {
        assert!(CAPABILITIES.contains(&crate::session::FOCUS_WINDOW));
        assert!(CAPABILITIES.contains(&crate::session::FOCUS_FIELD));
        assert_eq!(crate::session::FOCUS_WINDOW, "focus-window");
        assert_eq!(crate::session::FOCUS_FIELD, "focus-field");
    }

    /// Only the controller is answered: a process in the helper's own
    /// account, where the apps under test run, is turned away.
    #[tokio::test]
    async fn only_the_controller_is_admitted() {
        let me = nix::unistd::getuid().as_raw();
        let (_client, server) = UnixStream::pair().unwrap();

        assert!(admitted(&server, me), "the controller");
        assert!(
            !admitted(&server, me.wrapping_add(1)),
            "the helper's own account, or any other"
        );
    }

    /// Deliver `line` in `chunk`-byte writes and parse it as the service would.
    async fn deliver(line: Vec<u8>, chunk: usize) -> crate::error::Result<serde_json::Value> {
        let (mut client, mut server) = tokio::io::duplex(chunk);
        let writer = tokio::spawn(async move {
            for part in line.chunks(chunk) {
                if client.write_all(part).await.is_err() {
                    break;
                }
            }
        });

        let result = read_request(&mut server).await;
        writer.abort();
        result
    }

    fn padded(length: usize) -> Vec<u8> {
        let mut line = br#"{"op":"hello"}"#.to_vec();
        line.resize(length, b' ');
        line.push(b'\n');
        line
    }

    #[tokio::test]
    async fn the_line_limit_holds_however_the_request_is_fragmented() {
        for chunk in [1000, 4096, 65536] {
            let reply = deliver(padded(MAX_REQUEST), chunk).await.unwrap();
            assert_eq!(reply["op"], "hello");
            let error = deliver(padded(MAX_REQUEST + 1), chunk).await.unwrap_err();
            assert_eq!(error.code, "protocol");
        }
    }

    #[tokio::test]
    async fn malformed_and_truncated_requests_are_errors() {
        assert_eq!(
            deliver(b"{not json}\n".to_vec(), 64)
                .await
                .unwrap_err()
                .code,
            "invalid-json"
        );
        assert_eq!(
            deliver(b"{\"op\":".to_vec(), 64).await.unwrap_err().code,
            "protocol"
        );
    }
    #[tokio::test]
    async fn a_key_that_opens_a_sheet_keeps_later_keys_as_read_only_continuations() {
        let stand = Stand::locked(&[]);
        stand.lock().await.opens_sheet = true;
        let gate = Arc::new(Gate::default());
        let expect = Expect::new(812).unwrap().in_window(42).unwrap();
        let mut engine = stand.lock().await;
        run(&mut *engine, &gate, keys(2), Some(&expect))
            .await
            .unwrap();
        assert_eq!(engine.preparations, [true, false]);
        assert_eq!(engine.budgets[0], super::focus::PREPARE_BUDGET);
        assert_eq!(engine.budgets[1], super::focus::CHECK_BUDGET);
        assert_eq!(engine.posted, 2);
        let refused = run(&mut *engine, &gate, keys(1), Some(&expect))
            .await
            .unwrap_err();
        assert_eq!(refused["reason"], "sheet");
        assert_eq!(refused["performed"], 0);
        assert_eq!(engine.posted, 2);
    }

    #[tokio::test]
    async fn held_input_and_waits_never_prepare_a_target() {
        let stand = Stand::locked(&[]);
        let gate = Arc::new(Gate::default());
        let expect = Expect::new(812).unwrap();
        let mut engine = stand.lock().await;
        engine.held = true;
        let actions = serde_json::from_value(json!([
            {"type":"move", "x": 20, "y":20},
            {"type":"wait", "ms":1},
            {"type":"pointer", "phase":"up", "x":20, "y":20, "button":"left"}
        ]))
        .unwrap();
        run(&mut *engine, &gate, actions, Some(&expect))
            .await
            .unwrap();
        assert_eq!(engine.preparations, [false, false]);
        assert_eq!(engine.budgets, [super::focus::CHECK_BUDGET; 2]);
        assert_eq!(engine.posted, 3);
        assert!(engine.budgets.windows(2).all(|pair| pair[1] <= pair[0]));
        engine.script = vec![Step::Post, Step::Post, Step::Refuse];
        let reply = run(
            &mut *engine,
            &gate,
            vec![Action::Move { x: 20.0, y: 20.0 }],
            Some(&expect),
        )
        .await
        .unwrap_err();
        assert_eq!(reply["performed"], 0);
        assert_eq!(engine.posted, 3);
        assert_eq!(engine.preparations.last(), Some(&false));
        assert_eq!(engine.releases, 1);
    }

    #[test]
    fn the_persons_input_cannot_name_an_agent_target() {
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let _grant = gate.acquire(&owner, super::Who::Person).unwrap();
        let expect = Expect::new(812).unwrap();
        let error = super::agent_target(&gate, Some(&expect)).unwrap_err();
        assert_eq!(error.code, Code::Input);
        assert_eq!(
            error.error,
            "The person's input cannot name an agent target"
        );
        super::agent_target(&gate, None).unwrap();
    }

    struct Blocked {
        grant: super::super::lease::Grant,
        started: Option<tokio::sync::oneshot::Sender<()>>,
        resume: Option<std::sync::mpsc::Receiver<()>>,
        writes: Arc<std::sync::atomic::AtomicUsize>,
        posted: u64,
        releases: usize,
    }

    impl Perform for Blocked {
        async fn require(
            &mut self,
            _: &Expect,
            _: Need,
            _: Duration,
            _: bool,
        ) -> Result<Option<&'static str>> {
            let grant = self.grant.clone();
            let writes = self.writes.clone();
            let started = self.started.take().unwrap();
            let resume = self.resume.take().unwrap();
            let final_grant = grant.clone();
            super::checked_preparation(
                move || {
                    drop(grant.authorize()?);
                    writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    started.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(3)).unwrap();
                    drop(grant.authorize()?);
                    writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(None)
                },
                || {
                    drop(final_grant.authorize()?);
                    Ok(())
                },
            )
            .await
        }
        async fn perform(&mut self, _: Action) -> Result<()> {
            self.posted += 1;
            Ok(())
        }
        fn posted(&self) -> u64 {
            self.posted
        }
        fn held_input(&self) -> bool {
            false
        }
        fn release(&mut self) -> Result<()> {
            self.releases += 1;
            Ok(())
        }
    }

    enum Ending {
        Abort,
        Term,
        Stop,
        Cancel,
    }

    /// A blocked write uses the real grant and operation registry. The
    /// request owns no execution guard once the operation has started.
    async fn blocked_operation(ending: Ending) {
        let shutdown = matches!(ending, Ending::Term);
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let operations = Arc::new(super::Operations::default());
        let (started, entered) = tokio::sync::oneshot::channel();
        let (resume, barrier) = std::sync::mpsc::channel();
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let engine = Arc::new(Mutex::new(Blocked {
            grant,
            started: Some(started),
            resume: Some(barrier),
            writes: writes.clone(),
            posted: 0,
            releases: 0,
        }));
        let observed = Arc::new(std::sync::Mutex::new(None));
        let mut requests = tokio::task::JoinSet::new();
        let request = {
            let engine = engine.clone();
            let gate = gate.clone();
            let operations = operations.clone();
            let observed = observed.clone();
            requests.spawn(async move {
                let mut cancel = super::Cancellation {
                    gate: gate.clone(),
                    owner,
                    armed: true,
                };
                let mut engine = engine.lock_owned().await;
                let outcome = operations
                    .start(async move { run(&mut *engine, &gate, keys(1), app().as_ref()).await })
                    .await
                    .unwrap();
                let reply = outcome.await.unwrap();
                cancel.armed = false;
                *observed.lock().unwrap() = Some(reply);
            })
        };
        entered.await.unwrap();
        assert!(engine.try_lock().is_err());
        if matches!(ending, Ending::Stop | Ending::Cancel) {
            if matches!(ending, Ending::Stop) {
                gate.hold(true).unwrap();
            } else {
                gate.cancel(None).unwrap();
            }
            assert!(!gate.active().unwrap(), "cancellation did not wait for AX");
        } else if !shutdown {
            request.abort();
            while requests.join_next().await.is_some() {}
            assert!(
                !gate.active().unwrap(),
                "request cancellation did not wait for AX"
            );
            let abandoned = super::drain_and_release(&operations, &engine, &gate);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), abandoned)
                    .await
                    .is_err()
            );
            assert_eq!(
                operations.tasks.lock().unwrap().len(),
                1,
                "cancelling cleanup kept the worker registered"
            );
        }
        {
            let cleanup = async {
                if shutdown {
                    super::finish(&gate, &mut requests, &operations, &engine).await
                } else {
                    super::drain_and_release(&operations, &engine, &gate).await
                }
            };
            tokio::pin!(cleanup);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), &mut cleanup)
                    .await
                    .is_err(),
                "cleanup returned while a write was in flight"
            );
            assert!(!gate.active().unwrap());
            let competing = async {
                let _engine = engine.lock().await;
                let who = if matches!(ending, Ending::Stop) {
                    super::Who::Person
                } else {
                    super::Who::Agent
                };
                gate.acquire(&"b".repeat(48), who).map(drop)
            };
            tokio::pin!(competing);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), &mut competing)
                    .await
                    .is_err(),
                "a second holder passed the blocked worker"
            );
            resume.send(()).unwrap();
            let (cleaned, _) = tokio::join!(cleanup, competing);
            cleaned.unwrap();
        }
        assert_eq!(
            writes.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "only the bounded in-flight write finished"
        );
        let engine = engine.lock().await;
        assert_eq!(engine.posted, 0);
        assert!(engine.releases >= 1);
        assert!(operations.tasks.lock().unwrap().is_empty());
        if matches!(ending, Ending::Stop | Ending::Cancel) {
            requests.join_next().await.unwrap().unwrap();
            let reply = observed.lock().unwrap().take().unwrap().unwrap_err();
            let (code, instruction) = if matches!(ending, Ending::Stop) {
                (Code::Stopped, crate::error::STOPPED)
            } else {
                (Code::Cancelled, "Native control was stopped")
            };
            assert_eq!(reply["code"], json!(code));
            assert_eq!(reply["error"], instruction);
            assert_eq!(reply["performed"], 0);
            assert_eq!(reply["uncertain"], false);
        }
    }

    #[tokio::test]
    async fn aborting_a_request_waits_for_its_preparation_before_cleanup() {
        blocked_operation(Ending::Abort).await;
    }

    #[tokio::test]
    async fn term_shutdown_drains_preparation_before_returning() {
        blocked_operation(Ending::Term).await;
    }

    #[tokio::test]
    async fn repeated_input_under_one_retained_lease_keeps_the_operation_registry_bounded() {
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let operations = super::Operations::default();
        let engine = Stand::locked(&[]);
        for _ in 0..1000 {
            let mut engine = engine.clone().lock_owned().await;
            let grant = grant.clone();
            let gate = gate.clone();
            operations
                .start(async move {
                    drop(grant.authorize()?);
                    Ok::<_, Error>(run(&mut *engine, &gate, keys(1), None).await)
                })
                .await
                .unwrap()
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(operations.tasks.lock().unwrap().len() <= 1);
        }
        assert!(gate.active().unwrap());
        drop(grant.authorize().unwrap());
        let engine = engine.lock().await;
        assert_eq!(engine.posted, 1000);
        assert_eq!(engine.releases, 0);
        operations.drain().await.unwrap();
        assert!(operations.tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reaping_completed_operations_keeps_running_work_registered() {
        let operations = super::Operations::default();
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        let running = operations.start(barrier).await.unwrap();
        for _ in 0..100 {
            operations.start(async {}).await.unwrap().await.unwrap();
            assert_eq!(operations.tasks.lock().unwrap().len(), 2);
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(10), operations.drain())
                .await
                .is_err()
        );
        assert_eq!(operations.tasks.lock().unwrap().len(), 2);
        resume.send(()).unwrap();
        running.await.unwrap().unwrap();
        operations.drain().await.unwrap();
        assert!(operations.tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_reaped_operation_join_failure_remains_uncertain_during_cleanup() {
        let operations = super::Operations::default();
        let outcome = operations
            .start(async { panic!("worker failed") })
            .await
            .unwrap();
        assert!(outcome.await.is_err());
        let error = operations.start(async {}).await.unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
        assert!(operations.tasks.lock().unwrap().is_empty());
        let error = operations.drain().await.unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
    }

    #[tokio::test]
    async fn a_successful_reset_recovers_named_and_unnamed_input_after_a_join_failure() {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let old_grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let stand = Stand::locked(&[]);
        stand.lock().await.held = true;
        let outcome = operations
            .start(async { panic!("worker failed") })
            .await
            .unwrap();
        assert!(outcome.await.is_err());
        operations.reap().await.unwrap();
        assert!(operations.check().unwrap_err().uncertain);

        super::reset(&operations, &stand, &gate, |engine| {
            assert!(!gate.active()?);
            engine.assert_locked();
            engine.release()
        })
        .await
        .unwrap();
        assert!(old_grant.authorize().is_err());
        operations.check().unwrap();
        assert!(!stand.lock().await.held);
        assert_eq!(stand.lock().await.releases, 1);

        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let named = Expect::new(812)
            .unwrap()
            .in_window(42)
            .unwrap()
            .in_field("Email")
            .unwrap();
        for expect in [Some(named), None] {
            let mut engine = stand.clone().lock_owned().await;
            let grant = grant.clone();
            let gate = gate.clone();
            operations
                .start(async move {
                    drop(grant.authorize()?);
                    Ok::<_, Error>(run(&mut *engine, &gate, keys(1), expect.as_ref()).await)
                })
                .await
                .unwrap()
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
        assert_eq!(stand.lock().await.posted, 2);
        gate.cancel(None).unwrap();
        super::drain_and_release(&operations, &stand, &gate)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reset_joins_registered_work_before_cleanup_and_clearing_failure() {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let _grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
        let stand = Stand::locked(&[]);
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        let running = operations.start(barrier).await.unwrap();
        let outcome = operations
            .start(async { panic!("worker failed") })
            .await
            .unwrap();
        assert!(outcome.await.is_err());
        operations.reap().await.unwrap();
        let recovery = super::reset(&operations, &stand, &gate, Perform::release);
        tokio::pin!(recovery);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut recovery)
                .await
                .is_err()
        );
        assert!(!gate.active().unwrap());
        assert!(operations.check().unwrap_err().uncertain);
        assert_eq!(stand.lock().await.releases, 0);
        assert_eq!(operations.tasks.lock().unwrap().len(), 1);
        resume.send(()).unwrap();
        recovery.await.unwrap();
        running.await.unwrap().unwrap();
        assert_eq!(stand.lock().await.releases, 1);
        operations.drain().await.unwrap();
        assert!(operations.tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_reset_cleanup_keeps_input_uncertain_until_a_successful_reset() {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let _grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
        let stand = Stand::locked(&[]);
        {
            let mut engine = stand.lock().await;
            engine.held = true;
            engine.release_fails = true;
        }
        let error = super::reset(&operations, &stand, &gate, Perform::release)
            .await
            .unwrap_err();
        assert!(error.uncertain);
        assert!(!gate.active().unwrap());
        assert!(operations.check().unwrap_err().uncertain);
        assert!(operations.drain().await.unwrap_err().uncertain);
        assert!(stand.lock().await.held);
        for expect in [app(), None] {
            let mut engine = stand.clone().lock_owned().await;
            let gate = gate.clone();
            assert!(
                operations
                    .start(async move { run(&mut *engine, &gate, keys(1), expect.as_ref()).await })
                    .await
                    .unwrap_err()
                    .uncertain
            );
        }
        assert_eq!(stand.lock().await.posted, 0);
        // A timer or release can finish cleanup but cannot acknowledge recovery.
        stand.lock().await.release_fails = false;
        assert!(
            super::drain_and_release(&operations, &stand, &gate)
                .await
                .unwrap_err()
                .uncertain
        );
        assert!(!stand.lock().await.held);
        assert!(operations.check().is_err());
        super::reset(&operations, &stand, &gate, Perform::release)
            .await
            .unwrap();
        operations.check().unwrap();
        operations.drain().await.unwrap();
    }

    #[tokio::test]
    async fn a_preparation_worker_join_failure_keeps_the_outcome_unknown() {
        let error = super::preparation_worker::<()>(|| panic!("worker failed"))
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
        let stand = Stand::locked(&[]);
        let mut engine = stand.lock().await;
        let reply = super::failed(&mut *engine, error, 0, 0);
        assert_eq!(reply["uncertain"], true);
        assert_eq!(reply["partial"], false);
        assert_eq!(reply["performed"], 0);
    }
    #[tokio::test]
    async fn stopping_during_preparation_cancels_ownership_without_waiting_for_the_engine() {
        blocked_operation(Ending::Stop).await;
    }

    #[tokio::test]
    async fn cancelling_during_preparation_keeps_the_observed_cancellation_reply() {
        blocked_operation(Ending::Cancel).await;
    }

    #[tokio::test]
    async fn all_text_parts_spend_the_same_focus_budget() {
        let stand = Stand::locked(&[]);
        let mut engine = stand.lock().await;
        engine.check_delay = Duration::from_millis(5);
        let gate = Gate::default();
        let actions = (0..3)
            .map(|_| Action::Type {
                text: "hello".into(),
            })
            .collect();
        run(&mut *engine, &gate, actions, app().as_ref())
            .await
            .unwrap();
        assert_eq!(engine.budgets[0], super::focus::PREPARE_BUDGET);
        assert!(engine.budgets.windows(2).all(|pair| pair[1] < pair[0]));
        assert_eq!(engine.preparations, [true, true, true]);
    }
}
