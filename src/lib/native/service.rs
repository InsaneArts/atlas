//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
//! The service retains its gate, registered work and engine through cleanup;
//! a vanished request or reply cannot acknowledge an unknown outcome.
use super::{
    accessibility_trusted,
    events::Engine,
    focus::{self, Need},
    lease::{Gate, Who},
    session_info,
};
use crate::{
    actions::{self, Action, Expect},
    error::{Code, Error, Holder, Result},
    screen::geometry::Bounds,
    session::{Display, InputEpoch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap},
    error::Error as StdError,
    fmt,
    path::{Path, PathBuf},
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
    sync::{Mutex, Semaphore, watch},
    task::{Id, JoinError, JoinSet},
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
    connections: StdMutex<HashMap<Id, (watch::Receiver<ConnectionPhase>, bool)>>,
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

struct CleanupFailures {
    primary: Error,
    cleanup: Error,
}

impl fmt::Debug for CleanupFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CleanupFailures")
            .field("primary", &self.primary.code)
            .field("cleanup", &self.cleanup.code)
            .field(
                "uncertain",
                &(self.primary.uncertain || self.cleanup.uncertain),
            )
            .finish()
    }
}

impl fmt::Display for CleanupFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Native work and cleanup failed ({})", self.cleanup.code)
    }
}

impl StdError for CleanupFailures {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.primary)
    }
}

fn combine_cleanup(primary: Result<()>, cleanup: Result<()>) -> Result<()> {
    match (primary, cleanup) {
        (Ok(()), outcome) | (outcome, Ok(())) => outcome,
        (Err(primary), Err(cleanup)) => {
            let uncertain = primary.uncertain || cleanup.uncertain;
            let mut error = Error::new(
                primary.code,
                format!(
                    "{}; cleanup: {} (recovery pending: {uncertain})",
                    primary.error, cleanup.code,
                ),
            );
            error.uncertain = uncertain;
            error.holder = primary.holder;
            error.details = primary.details.clone();
            Err(error.with_source(CleanupFailures { primary, cleanup }))
        }
    }
}

/// Cancelling a request changes ownership without waiting for the engine.
struct Cancellation {
    gate: Arc<Gate>,
    owner: Option<String>,
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Some(owner) = &self.owner {
            // Drop can only try to cancel ownership. It cannot acknowledge
            // release, clear sticky worker failure or settle the controller's
            // pending record, even if cancellation succeeds.
            let _ = self.gate.cancel(Some(owner));
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
    let released = match gate.active() {
        Ok(false) => engine.release(),
        Ok(true) => Ok(()),
        Err(error) => Err(error),
    };
    combine_cleanup(drained, released)
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
    let mut connections = operations.connections.lock().map_err(|_| worker_failed())?;
    for (phase, recovered) in connections.values_mut() {
        // A closed sender has finished dispatch and reply delivery. Its
        // unobserved outcome belongs to the work this reset just settled.
        *recovered |= phase.has_changed().is_err();
    }
    operations.failed.store(false, Ordering::Relaxed);
    Ok(())
}

/// Every service exit uses the same close, request cancellation and worker
/// drain boundary, including TERM and internal failures.
async fn finish(
    gate: &Gate,
    requests: &mut JoinSet<Result<()>>,
    operations: &Operations,
    engine: &Mutex<impl Perform>,
) -> Result<()> {
    let closed = gate.close();
    let stopping = closed.is_ok();
    requests.abort_all();
    let mut connections = Ok(());
    while let Some(outcome) = requests.join_next_with_id().await {
        connections = combine_cleanup(
            connections,
            connection_outcome(outcome, operations, stopping),
        );
    }
    let cleanup = drain_and_release(operations, engine, gate).await;
    combine_cleanup(closed, combine_cleanup(connections, cleanup))
}

#[derive(Clone, Copy, Debug)]
enum ConnectionPhase {
    Refusal,
    Read,
    Mutation(&'static str),
}

fn connection_outcome(
    outcome: std::result::Result<(Id, Result<()>), JoinError>,
    operations: &Operations,
    stopping: bool,
) -> Result<()> {
    let id = match &outcome {
        Ok((id, _)) => *id,
        Err(error) => error.id(),
    };
    let mut connections = operations.connections.lock().map_err(|_| worker_failed())?;
    let (phase, recovered) = connections
        .remove(&id)
        .map(|(phase, recovered)| (*phase.borrow(), recovered))
        .unwrap_or((ConnectionPhase::Mutation("connection"), false));
    match outcome {
        Ok((_, Ok(()))) => Ok(()),
        // Refusals have no effects, and a read client need not receive its
        // reply. Neither transport failure degrades the input engine.
        Ok((_, Err(_))) if matches!(phase, ConnectionPhase::Refusal | ConnectionPhase::Read) => {
            Ok(())
        }
        Err(error) if stopping && error.is_cancelled() => Ok(()),
        // Reset already joined the work and rebound the engine after this
        // connection finished. Observing it cannot undo that recovery.
        _ if recovered => Ok(()),
        outcome => {
            operations.failed.store(true, Ordering::Relaxed);
            let operation = match phase {
                ConnectionPhase::Refusal => "refusal",
                ConnectionPhase::Read => "hello",
                ConnectionPhase::Mutation(operation) => operation,
            };
            let error = Error::new(
                Code::Uncertain,
                format!("Native {operation} connection outcome is unknown; recovery pending"),
            )
            .uncertain();
            match outcome {
                Ok((_, Err(cause))) => Err(error.with_source(cause)),
                Err(cause) => Err(error.with_source(cause)),
                Ok((_, Ok(()))) => Ok(()),
            }
        }
    }
}

/// Field refusals remain typed until the operation reaches its original guard.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Hello,
    Shutdown {
        #[serde(default, deserialize_with = "Request::pid")]
        pid: u64,
    },
    Hold {
        #[serde(default = "Request::missing_hold", deserialize_with = "Request::hold")]
        on: Result<bool>,
    },
    Release {
        #[serde(default, deserialize_with = "Request::owner")]
        owner: String,
    },
    Renew {
        #[serde(default, deserialize_with = "Request::owner")]
        owner: String,
    },
    Acquire {
        #[serde(default, deserialize_with = "Request::owner")]
        owner: String,
        #[serde(
            default = "Request::missing_epoch",
            deserialize_with = "Request::input_epoch"
        )]
        epoch: Result<InputEpoch>,
        #[serde(
            rename = "person",
            default = "Request::agent",
            deserialize_with = "Request::who"
        )]
        who: Who,
    },
    Reset {
        #[serde(
            default = "Request::missing_epoch",
            deserialize_with = "Request::input_epoch"
        )]
        epoch: Result<InputEpoch>,
    },
    #[serde(skip)]
    Unknown {
        epoch: Result<InputEpoch>,
    },
    Input {
        #[serde(default, deserialize_with = "Request::owner")]
        owner: String,
        #[serde(
            default = "Request::missing_epoch",
            deserialize_with = "Request::input_epoch"
        )]
        epoch: Result<InputEpoch>,
        #[serde(
            default = "Request::missing_actions",
            deserialize_with = "Request::actions"
        )]
        actions: Result<Vec<Action>>,
        #[serde(default = "Request::no_expect", deserialize_with = "Request::expect")]
        expect: Result<Option<Expect>>,
        #[serde(default, deserialize_with = "Request::app")]
        app: bool,
    },
    #[serde(other)]
    Unrecognized,
}

impl Request {
    fn pid<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<u64, D::Error> {
        Ok(Value::deserialize(deserializer)?
            .as_u64()
            .unwrap_or_default())
    }

    fn hold<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Result<bool>, D::Error> {
        Ok(Value::deserialize(deserializer)?
            .as_bool()
            .ok_or_else(|| Error::new(Code::Input, "Hold requires a boolean on")))
    }

    fn missing_hold() -> Result<bool> {
        Err(Error::new(Code::Input, "Hold requires a boolean on"))
    }

    fn app<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<bool, D::Error> {
        Ok(Value::deserialize(deserializer)? == true)
    }

    fn input_epoch<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Result<InputEpoch>, D::Error> {
        Ok(InputEpoch::deserialize(Value::deserialize(deserializer)?).map_err(Error::from))
    }

    fn actions<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Result<Vec<Action>>, D::Error> {
        Ok(actions::decode(&Value::deserialize(deserializer)?))
    }

    fn missing_epoch() -> Result<InputEpoch> {
        InputEpoch::deserialize(Value::Null).map_err(Error::from)
    }

    fn missing_actions() -> Result<Vec<Action>> {
        actions::decode(&Value::Null)
    }

    fn no_expect() -> Result<Option<Expect>> {
        Ok(None)
    }

    fn expect<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Result<Option<Expect>>, D::Error> {
        Ok(Expect::from_request(&Value::deserialize(deserializer)?))
    }

    fn agent() -> Who {
        Who::Agent
    }

    fn who<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Who, D::Error> {
        Ok(if Value::deserialize(deserializer)? == true {
            Who::Person
        } else {
            Who::Agent
        })
    }

    fn owner<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<String, D::Error> {
        Ok(Value::deserialize(deserializer)?
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        // Parse once to preserve JSON's last-member value and sorted field
        // refusal order, including within actions, epochs and expectations.
        let value: Value = serde_json::from_slice(bytes)?;
        if value.get("op").and_then(Value::as_str).is_none() {
            return Err(Error::new(
                Code::Input,
                "Native operation requires a string op",
            ));
        }
        // Every operation must choose whether to retain its fields or bind
        // an unrecognized tag to the epoch checks, without another name table.
        Ok(match Self::deserialize(&value)? {
            Self::Unrecognized => Self::Unknown {
                epoch: InputEpoch::deserialize(&value["epoch"]).map_err(Error::from),
            },
            request @ (Self::Hello
            | Self::Shutdown { .. }
            | Self::Hold { .. }
            | Self::Release { .. }
            | Self::Renew { .. }
            | Self::Acquire { .. }
            | Self::Reset { .. }
            | Self::Input { .. }
            | Self::Unknown { .. }) => request,
        })
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
            || !accessibility_trusted()
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
            Request::Unrecognized => self.unknown(Request::missing_epoch()),
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

        phase.send_replace(ConnectionPhase::Mutation("shutdown"));
        if let Err(error) = engine.release() {
            return Err(Error::new(
                Code::Uncertain,
                format!("Held input could not be released: {}", error.error),
            )
            .uncertain());
        }

        self.gate.close()?;
        Ok(json!({ "ok": true, "stopping": true, "pid": std::process::id() }))
    }

    /// Stop and resume remain available after session or permission loss.
    async fn set_hold(&self, on: bool, phase: &watch::Sender<ConnectionPhase>) -> Result<Value> {
        let reply = hold(&self.gate, &self.stop_marker, on, phase)?;
        self.cleanup().await?;
        Ok(reply)
    }

    async fn release(
        &self,
        owner: String,
        phase: &watch::Sender<ConnectionPhase>,
    ) -> Result<Value> {
        self.gate.cancel(Some(&owner))?;
        phase.send_replace(ConnectionPhase::Mutation("release"));
        self.cleanup().await?;
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
            return Err(Error::new(
                Code::SessionChanged,
                "Input coordinates belong to a different session or display",
            ));
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
            // An agent finding the person in control is told to wait.
            let error = Error::new(
                Code::Busy,
                if who == Who::Agent && holder == Holder::Person {
                    crate::error::PERSON_HAS_CONTROL
                } else {
                    "The native input service already has a controller"
                },
            );
            return Err(error.held_by(holder));
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
                engine.dock_pressed = false;
                let result = run(&mut *engine, &gate, actions, expected.as_ref()).await;
                engine.target = None;
                Ok::<_, Error>(result.map(|field| (field, engine.dock_pressed)))
            })
            .await?;
        let result = outcome.await.map_err(|_| worker_failed())?;
        let (field, dock_pressed) = match result? {
            Ok(field) => field,
            Err(reply) => return Ok(reply),
        };

        // The recipient for the agent's activity log, only when asked:
        // Dock after AXPress, otherwise the owner of the frontmost ordinary
        // window and its bundle identifier, read once the engine is free.
        let landed = if report_app && dock_pressed {
            Some(("Dock".to_owned(), Some("com.apple.dock".to_owned())))
        } else if report_app {
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
    if !on && kept.is_err() {
        return Err(Error::new(
            Code::Native,
            "Agents could not be resumed: the helper could not clear the stop",
        ));
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

/// What the input loop needs of the event engine; tests stand in for it.
trait Perform {
    /// Monotonic time used to charge the request's shared focus budget.
    fn now(&self) -> Instant {
        Instant::now()
    }

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
    /// Carry out one action through posted events or a Dock AXPress.
    async fn perform(&mut self, action: Action) -> Result<()>;
    /// Events posted or AX presses acknowledged so far.
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
        let key = match action {
            Action::Key { .. } => true,
            Action::Pointer { .. }
            | Action::Move { .. }
            | Action::Click { .. }
            | Action::Drag { .. }
            | Action::Scroll { .. }
            | Action::Type { .. }
            | Action::Wait { .. } => false,
        };
        let continuation = key && pressed;
        let expected = if continuation { later.as_ref() } else { expect };
        pressed |= key;
        let checked = match expected {
            Some(expected) => {
                let prepare = !continuation && !engine.held_input();
                require_within(engine, expected, action.target(), &mut budget, prepare).await
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
    let started = engine.now();
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
    *budget = budget.saturating_sub(engine.now().saturating_duration_since(started));
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

/// Serve the configured helper's authenticated input socket until shutdown.
/// The listener, expiry timer, signal receivers and registered work share this
/// lifetime so every exit closes ownership and drains work before cleanup.
///
/// Returns an error if identity, permission, endpoint creation or cleanup fails.
/// Cleanup failure preserves uncertainty rather than acknowledging release.
///
/// # Errors
///
/// Rejects invalid helper identity, permissions or socket ownership, and returns service
/// setup or shutdown failures. Unknown input and cleanup outcomes remain owned until
/// recovery.
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
            Some(outcome) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                if let Err(error) = connection_outcome(outcome, &service.operations, false) {
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

                let mut connections = match service.operations.connections.lock() {
                    Ok(connections) => connections,
                    Err(_) => break Err(worker_failed()),
                };
                let connected = service.clone();
                let (phase, observed) = watch::channel(ConnectionPhase::Refusal);
                let task = tasks.spawn(async move {
                    let _slot = slot;
                    handle(stream, connected, phase).await
                });
                connections.insert(task.id(), (observed, false));
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
    combine_cleanup(outcome, cleanup)
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
        | Request::Unrecognized
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
mod tests {
    use super::{
        Action, CAPABILITIES, Duration, Error, Expect, Gate, Instant, MAX_REQUEST, Need, Perform,
        Request, Result, admitted, hold, read_request, run,
    };
    use crate::error::Code;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::sync::{Arc, Weak};
    use tokio::{io::AsyncWriteExt, net::UnixStream, sync::Mutex};

    fn service(marker: &std::path::Path) -> super::Service {
        let epoch = crate::session::InputEpoch {
            uid: 0,
            security_session_id: 0,
            display_id: 0,
            width: 1.0,
            height: 1.0,
            process_id: std::process::id(),
        };
        super::Service {
            engine: Arc::new(Mutex::new(super::Engine::idle(epoch.clone()))),
            operations: super::Operations::default(),
            gate: Arc::new(Gate::default()),
            epoch: super::RwLock::new(epoch),
            build: None,
            executable: None,
            stopping: tokio::sync::Notify::new(),
            stop_marker: marker.to_owned(),
        }
    }

    fn input_body(fields: &str) -> String {
        format!(
            r#"{{"op":"input","owner":"{}","epoch":{{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}},{fields}}}"#,
            "a".repeat(48)
        )
    }

    #[test]
    fn input_decoding_keeps_optional_compatibility_and_legacy_refusal_words() {
        let display = crate::session::Display {
            width: 800.0,
            height: 600.0,
            scale: 1.0,
        };
        for options in [
            "",
            r#", "app":false"#,
            r#", "app":true"#,
            r#", "expect":null"#,
            r#", "expect":{"pid":812,"window":42,"field":"Search"}"#,
        ] {
            let body = input_body(&format!(
                r#""actions":[{{"type":"move","x":10,"y":20}}]{options},"other":1,"other":2,"person":null,"person":false"#
            ));
            let Request::Input {
                owner,
                epoch,
                actions,
                expect,
                app,
            } = Request::decode(body.as_bytes()).unwrap()
            else {
                panic!("expected typed input");
            };
            assert_eq!(owner, "a".repeat(48));
            assert_eq!(epoch.unwrap().width, display.width);
            assert_eq!(actions.unwrap().len(), 1);
            assert_eq!(app, options.contains("true"));
            assert_eq!(expect.unwrap().is_some(), options.contains("812"));
        }
        for actions in [
            "null",
            "false",
            "1",
            r#""actions""#,
            "{}",
            "[]",
            r#"[{"type":"move","x":1}]"#,
            r#"[{"type":"move","x":null,"y":20}]"#,
            r#"[{"type":"click","x":null,"y":20,"count":null}]"#,
            r#"[{"type":"click","count":null,"y":20,"x":null}]"#,
            r#"[{"type":"click","x":10,"y":20,"count":0}]"#,
            r#"[{"type":"key","key":"unknown"}]"#,
            r#"[{"type":"wait","ms":251}]"#,
            r#"[{"type":"move","x":800,"y":20}]"#,
            r#"[{"type":"unknown"}]"#,
        ] {
            let body = input_body(&format!(r#""actions":{actions}"#));
            let wire: Value = serde_json::from_str(&body).unwrap();
            let old = crate::actions::parse(&wire["actions"], &display).unwrap_err();
            let Request::Input { actions, .. } = Request::decode(body.as_bytes()).unwrap() else {
                panic!("expected typed input");
            };
            let error = match actions {
                Ok(actions) => crate::actions::check(&actions, &display).unwrap_err(),
                Err(error) => Error::new(error.code, &error.error),
            };
            assert_eq!(json!(error), json!(old), "{body}");
        }
        for expect in [
            "false",
            "0",
            r#""expect""#,
            "[]",
            "{}",
            r#"{"pid":null}"#,
            r#"{"pid":812,"window":0}"#,
            r#"{"pid":812,"field":""}"#,
            r#"{"pid":812,"other":true}"#,
            r#"{"pid":812,"start":false}"#,
        ] {
            let body = input_body(&format!(
                r#""actions":[{{"type":"key","key":"a"}}],"expect":{expect}"#
            ));
            let wire: Value = serde_json::from_str(&body).unwrap();
            let old = Expect::from_request(&wire["expect"]).unwrap_err();
            let Request::Input { expect, .. } = Request::decode(body.as_bytes()).unwrap() else {
                panic!("expected typed input");
            };
            let error = expect.unwrap_err();
            assert_eq!(json!(error), json!(old), "{body}");
        }
    }

    #[tokio::test]
    async fn input_fields_preserve_legacy_acceptance_and_reject_the_complete_invalid_batch()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let display = crate::session::Display {
            width: 800.0,
            height: 600.0,
            scale: 1.0,
        };
        let mut cases = Vec::new();
        let alternatives = [
            json!({"type":"pointer","phase":"down","x":10,"y":20}),
            json!({"type":"move","x":10,"y":20}),
            json!({"type":"click","x":10,"y":20}),
            json!({"type":"drag","fromX":10,"fromY":20,"toX":30,"toY":40}),
            json!({"type":"scroll","x":10,"y":20}),
            json!({"type":"type","text":"a"}),
            json!({"type":"key","key":"a"}),
            json!({"type":"wait","ms":0}),
        ];
        for action in alternatives {
            let fields = action.as_object().ok_or("expected as_object")?;
            for (name, value) in fields {
                let mut missing = fields.clone();
                missing.remove(name);
                cases.push(input_body(&format!(
                    r#""actions":[{{"type":"key","key":"a"}},{missing}]"#,
                    missing = json!(missing)
                )));
                for wrong in [Value::Null, json!(false), json!({}), json!([])] {
                    let mut malformed = fields.clone();
                    malformed.insert(name.clone(), wrong);
                    cases.push(input_body(&format!(
                        r#""actions":[{{"type":"key","key":"a"}},{malformed}]"#,
                        malformed = json!(malformed)
                    )));
                }
                let rest = json!(missing).to_string();
                for (first, second) in [
                    (value.to_string(), value.to_string()),
                    ("null".into(), value.to_string()),
                    (value.to_string(), "null".into()),
                ] {
                    cases.push(input_body(&format!(r#""actions":[{{"type":"key","key":"a"}},{{"{name}":{first},"{name}":{second},{rest}}}]"#, rest=rest.trim_start_matches('{').trim_end_matches('}'))));
                }
            }
            for (name, value) in [
                ("button", json!("left")),
                ("count", json!(1)),
                ("modifiers", json!([])),
                ("dx", json!(0)),
                ("dy", json!(1)),
            ] {
                let applies = match fields["type"].as_str().ok_or("expected as_str")? {
                    "pointer" | "click" => ["button", "count", "modifiers"].contains(&name),
                    "drag" => name == "button",
                    "scroll" => ["dx", "dy"].contains(&name),
                    _ => false,
                };
                if applies {
                    for wrong in [
                        Value::Null,
                        json!(false),
                        json!({}),
                        json!([]),
                        json!("invalid"),
                    ] {
                        if wrong == value {
                            continue;
                        }
                        let mut malformed = fields.clone();
                        malformed.insert(name.into(), wrong);
                        cases.push(input_body(&format!(
                            r#""actions":[{{"type":"key","key":"a"}},{malformed}]"#,
                            malformed = json!(malformed)
                        )));
                    }
                    cases.push(input_body(&format!(r#""actions":[{{"type":"key","key":"a"}},{{{rest},"{name}":{value},"{name}":{value}}}]"#, rest=action.to_string().trim_start_matches('{').trim_end_matches('}'))));
                }
            }
        }
        for actions in [
            "null".to_owned(),
            "[]".into(),
            serde_json::to_string(&vec![json!({"type":"key","key":"a"}); 33])?,
            serde_json::to_string(&vec![json!({"type":"wait","ms":250}); 9])?,
        ] {
            cases.push(input_body(&format!(r#""actions":{actions}"#)));
        }
        for action in [
            json!({"type":"pointer","phase":"down","x":800,"y":20}),
            json!({"type":"click","x":10,"y":20,"count":0}),
            json!({"type":"click","x":10,"y":20,"count":6}),
            json!({"type":"click","x":10,"y":20,"modifiers":["invalid"]}),
            json!({"type":"drag","fromX":10,"fromY":20,"toX":800,"toY":40}),
            json!({"type":"scroll","x":10,"y":20,"dx":101}),
            json!({"type":"scroll","x":10,"y":20,"dy":-101}),
            json!({"type":"type","text":""}),
            json!({"type":"type","text":"x".repeat(257)}),
            json!({"type":"key","key":"invalid"}),
            json!({"type":"wait","ms":251}),
        ] {
            cases.push(input_body(&format!(
                r#""actions":[{{"type":"key","key":"a"}},{action}]"#
            )));
        }
        let base: Value =
            serde_json::from_str(&input_body(r#""actions":[{"type":"key","key":"a"}]"#))?;
        for (field, value) in base["epoch"].as_object().ok_or("expected as_object")? {
            let mut members = base["epoch"]
                .as_object()
                .ok_or("expected as_object")?
                .clone();
            members.remove(field);
            let rest = json!(members).to_string();
            let rest = rest.trim_start_matches('{').trim_end_matches('}');
            let mut missing = base.clone();
            missing["epoch"]
                .as_object_mut()
                .ok_or("expected as_object_mut")?
                .remove(field);
            cases.push(missing.to_string());
            for wrong in [
                Value::Null,
                json!(false),
                json!("invalid"),
                json!([]),
                json!({}),
            ] {
                let mut malformed = base.clone();
                malformed["epoch"][field] = wrong;
                cases.push(malformed.to_string());
            }
            for (first, second) in [
                (value.to_string(), value.to_string()),
                ("null".into(), value.to_string()),
                (value.to_string(), "null".into()),
            ] {
                cases.push(format!(r#"{{"op":"input","actions":[{{"type":"key","key":"a"}}],"epoch":{{{rest},"{field}":{first},"{field}":{second}}}}}"#));
            }
        }
        for field in ["op", "owner", "epoch", "actions", "expect", "app"] {
            let base: Value = serde_json::from_str(&input_body(
                r#""actions":[{"type":"key","key":"a"}],"expect":{"pid":812},"app":false"#,
            ))?;
            let value = &base[field];
            for (first, second) in [
                (value.to_string(), value.to_string()),
                ("null".into(), value.to_string()),
                (value.to_string(), "null".into()),
            ] {
                let mut members = base.as_object().ok_or("expected as_object")?.clone();
                members.remove(field);
                let rest = json!(members).to_string();
                cases.push(format!(
                    r#"{{{rest},"{field}":{first},"{field}":{second}}}"#,
                    rest = rest.trim_start_matches('{').trim_end_matches('}')
                ));
            }
        }
        for field in ["pid", "start", "window", "field"] {
            let values = json!({"pid":812,"start":[1,0],"window":42,"field":"Search"});
            let value = &values[field];
            let mut rest = values.as_object().ok_or("expected as_object")?.clone();
            rest.remove(field);
            let rest = json!(rest).to_string();
            cases.push(input_body(&format!(r#""actions":[{{"type":"key","key":"a"}}],"expect":{{{rest},"{field}":{value},"{field}":{value}}}"#, rest=rest.trim_start_matches('{').trim_end_matches('}'))));
        }
        for app in ["null", "0", r#""false""#, "[]", "{}"] {
            cases.push(input_body(&format!(
                r#""actions":[{{"type":"key","key":"a"}}],"app":{app}"#
            )));
        }
        for body in cases {
            let wire: Value = serde_json::from_str(&body)?;
            let legacy = (|| -> Result<Vec<Action>> {
                if wire["op"].as_str() != Some("input") {
                    return Err(Error::new(
                        Code::Input,
                        "Native operation requires a string op",
                    ));
                }
                crate::session::InputEpoch::deserialize(&wire["epoch"])?;
                let actions = crate::actions::parse(&wire["actions"], &display)?;
                Expect::from_request(&wire["expect"])?;
                Ok(actions)
            })();
            let result = (|| -> Result<Vec<Action>> {
                let Request::Input {
                    epoch,
                    actions,
                    expect,
                    app,
                    ..
                } = Request::decode(body.as_bytes())?
                else {
                    return Err(Error::new(
                        Code::Input,
                        "Native operation requires a string op",
                    ));
                };
                epoch?;
                let actions = actions?;
                crate::actions::check(&actions, &display)?;
                expect?;
                assert_eq!(app, wire["app"] == true, "{body}");
                Ok(actions)
            })();
            match legacy {
                Ok(expected) => {
                    let actual = result?;
                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{body}");
                }
                Err(expected) => {
                    let stand = Stand::locked(&[]);
                    let mut engine = stand.lock().await;
                    let actual = match result {
                        Err(error) => error,
                        Ok(actions) => {
                            let outcome = run(&mut *engine, &Gate::default(), actions, None).await;
                            panic!("invalid input reached execution for {body}: {outcome:?}");
                        }
                    };
                    assert_eq!(json!(actual), json!(expected), "{body}");
                    assert_eq!(engine.posted, 0, "{body}");
                    assert!(engine.log.is_empty(), "{body}");
                    assert_eq!(engine.releases, 0, "{body}");
                }
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn input_availability_still_precedes_deferred_field_refusals() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(&directory.path().join("stopped"));
        let owner = "a".repeat(48);
        let grant = service.gate.acquire(&owner, super::Who::Agent).unwrap();
        service
            .operations
            .failed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        for fields in [
            r#""actions":null"#,
            r#""actions":[{"type":"click","x":null,"y":20}]"#,
            r#""actions":[{"type":"key","key":"a"}],"expect":false"#,
        ] {
            let mut wire: Value = serde_json::from_str(&input_body(fields)).unwrap();
            for epoch in [wire["epoch"].clone(), Value::Null, json!({"uid":false})] {
                wire["epoch"] = epoch;
                let request = Request::decode(wire.to_string().as_bytes()).unwrap();
                let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
                let error = service.dispatch(request, &phase).await.unwrap_err();
                assert_eq!(json!(error), json!(super::worker_failed()));
                assert!(matches!(
                    *observed.borrow(),
                    super::ConnectionPhase::Refusal
                ));
                assert!(grant.authorize().is_ok());
                assert!(service.operations.tasks.lock().unwrap().is_empty());
                let engine = service.engine.lock().await;
                assert_eq!(engine.posted, 0);
                assert!(!engine.held_input());
            }
        }
    }

    #[tokio::test]
    async fn unknown_operations_keep_availability_and_epoch_refusals()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let service = service(&directory.path().join("stopped"));
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        service
            .operations
            .failed
            .store(true, super::Ordering::Relaxed);
        for fields in [
            "",
            r#", "epoch":null"#,
            r#", "epoch":false, "owner":null, "actions":null"#,
            r#", "epoch":{}, "epoch":false"#,
        ] {
            let body = format!(r#"{{"op":"unknown"{fields}}}"#);
            let request = Request::decode(body.as_bytes())?;
            let error = service
                .dispatch(request, &phase)
                .await
                .err()
                .ok_or("expected refusal")?;
            assert_eq!(json!(error), json!(super::worker_failed()), "{body}");
            assert!(matches!(
                *observed.borrow(),
                super::ConnectionPhase::Refusal
            ));
        }
        service
            .operations
            .failed
            .store(false, super::Ordering::Relaxed);
        for epoch in [Value::Null, json!(false), json!({})] {
            let body = json!({"op":"unknown","epoch":epoch}).to_string();
            let Request::Unknown { epoch: actual } = Request::decode(body.as_bytes())? else {
                panic!("expected unknown operation");
            };
            let expected = crate::session::InputEpoch::deserialize(&epoch)
                .err()
                .ok_or("expected refusal")?;
            assert_eq!(
                json!(actual.err().ok_or("expected refusal")?),
                json!(Error::from(expected))
            );
        }
        let mut stale = service.epoch()?;
        stale.display_id += 1;
        let error = service.check_epoch(stale).err().ok_or("expected refusal")?;
        assert_eq!(error.code, Code::SessionChanged);
        assert!(!error.uncertain);
        Ok(())
    }

    #[test]
    fn only_a_literal_true_person_flag_selects_personal_control()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let service = service(&directory.path().join("stopped"));
        let epoch = serde_json::to_string(&service.epoch()?)?;
        for person in ["null", r#""true""#, "0", "1", "[]", "{}", "false", "true"] {
            let body = format!(
                r#"{{"op":"acquire","owner":"{}","epoch":{epoch},"person":{person}}}"#,
                "a".repeat(48)
            );
            let Request::Acquire { who, epoch, .. } = Request::decode(body.as_bytes())? else {
                panic!("expected acquire");
            };
            assert_eq!(epoch?, service.epoch()?);
            assert_eq!(
                who,
                if person == "true" {
                    super::Who::Person
                } else {
                    super::Who::Agent
                }
            );
            assert!(!service.gate.active()?);
        }
        Ok(())
    }

    #[test]
    fn an_absent_person_flag_remains_an_agent()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let service = service(&directory.path().join("stopped"));
        let epoch = serde_json::to_string(&service.epoch()?)?;
        let owner = "a".repeat(48);
        for (person, expected) in [
            ("", super::Who::Agent),
            (r#", "person":false"#, super::Who::Agent),
            (r#", "person":true"#, super::Who::Person),
        ] {
            let body = format!(r#"{{"epoch":{epoch},"owner":"{owner}"{person},"op":"acquire"}}"#);
            let Request::Acquire {
                owner,
                epoch: actual,
                who,
            } = Request::decode(body.as_bytes())?
            else {
                panic!("expected typed acquire");
            };
            assert_eq!(actual?, service.epoch()?);
            assert_eq!(who, expected);
            let gate = Arc::new(Gate::default());
            gate.hold(true)?;
            let grant = gate.acquire(&owner, who);
            if who == super::Who::Agent {
                assert_eq!(
                    grant.err().ok_or("expected stopped grant")?.code,
                    Code::Stopped
                );
                assert!(!gate.active()?);
            } else {
                assert!(grant?.authorize().is_ok());
                assert_eq!(gate.holder()?, Some(crate::error::Holder::Person));
            }
        }
        Ok(())
    }

    #[test]
    fn acquire_and_reset_ignore_only_unrelated_extensions()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let epoch = r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800,"height":600,"processId":1}"#;
        let body = format!(
            r#"{{"op":"acquire","epoch":{epoch},"other":1,"other":2,"on":null,"on":false}}"#
        );
        let Request::Acquire { owner, who, .. } = Request::decode(body.as_bytes())? else {
            panic!("expected typed acquire");
        };
        assert_eq!(owner, "");
        assert_eq!(who, super::Who::Agent);
        for owner in ["", "not-an-owner", &"a".repeat(48)] {
            let body = format!(r#"{{"op":"acquire","owner":"{owner}","epoch":{epoch}}}"#);
            let Request::Acquire { owner: actual, .. } = Request::decode(body.as_bytes())? else {
                panic!("the gate checks token identity");
            };
            assert_eq!(actual, owner);
        }
        let body = format!(
            r#"{{"op":"reset","epoch":{epoch},"owner":null,"owner":false,"person":[],"person":{{}},"other":1,"other":2}}"#
        );
        assert!(matches!(
            Request::decode(body.as_bytes())?,
            Request::Reset { .. }
        ));
        // InputEpoch's existing sequence representation remains compatible.
        for op in ["acquire", "reset"] {
            let body = format!(r#"{{"op":"{op}","epoch":[501,1,2,800,600,1]}}"#);
            let (Request::Acquire { epoch: actual, .. } | Request::Reset { epoch: actual }) =
                Request::decode(body.as_bytes())?
            else {
                panic!("expected a typed epoch");
            };
            assert_eq!(
                actual?,
                crate::session::InputEpoch::deserialize(serde_json::from_str::<Value>(epoch)?)?
            );
        }
        Ok(())
    }

    #[test]
    fn epoch_refusals_keep_their_existing_words_and_order() {
        let fixtures = [
            (
                "[501,1,2,800,600,1,0]",
                "invalid length 7, expected fewer elements in array",
            ),
            (
                r#"{"z":0,"a":0}"#,
                "unknown field `a`, expected one of `uid`, `securitySessionId`, `displayId`, `width`, `height`, `processId`",
            ),
            (
                r#"{"uid":null,"displayId":"wrong"}"#,
                "invalid type: string \"wrong\", expected u64",
            ),
            (r#"{"z":0,"uid":null}"#, "invalid type: null, expected u32"),
            ("null", "invalid type: null, expected struct InputEpoch"),
            (
                "[]",
                "invalid length 0, expected struct InputEpoch with 6 elements",
            ),
            ("{}", "missing field `uid`"),
        ];
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        for op in ["acquire", "reset"] {
            for (epoch, message) in fixtures {
                let value = serde_json::from_str::<Value>(epoch).unwrap();
                let legacy = crate::session::InputEpoch::deserialize(&value).unwrap_err();
                assert_eq!(legacy.to_string(), message);
                let body = format!(r#"{{"op":"{op}","epoch":{epoch}}}"#);
                let error = match Request::decode(body.as_bytes()) {
                    Err(error) => json!(error),
                    Ok(
                        Request::Acquire {
                            epoch: Err(error), ..
                        }
                        | Request::Reset { epoch: Err(error) },
                    ) => json!(error),
                    request => panic!("expected an epoch refusal for {body}: {request:?}"),
                };
                actual.push((body.clone(), error));
                expected.push((
                    body,
                    json!({
                        "code": "invalid-json",
                        "error": message,
                        "ok": false,
                        "uncertain": false,
                    }),
                ));
            }
        }
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn typed_acquire_and_reset_still_require_fresh_authority() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(&directory.path().join("stopped"));
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let epoch = service.epoch().unwrap();
        for who in [super::Who::Agent, super::Who::Person] {
            let grant = service.gate.acquire(&"a".repeat(48), who).unwrap();
            for member in [
                "uid",
                "securitySessionId",
                "displayId",
                "width",
                "height",
                "processId",
            ] {
                let mut stale = json!(epoch);
                stale[member] = json!(2);
                let stale = crate::session::InputEpoch::deserialize(stale).unwrap();
                assert_ne!(stale, epoch);
                for request in [
                    Request::Acquire {
                        owner: "wrong-owner".into(),
                        epoch: Ok(stale.clone()),
                        who,
                    },
                    Request::Reset { epoch: Ok(stale) },
                ] {
                    let error = service.dispatch(request, &phase).await.unwrap_err();
                    assert_eq!(error.code, Code::Identity);
                    assert!(!error.uncertain);
                    assert!(grant.authorize().is_ok());
                    assert_eq!(service.epoch().unwrap(), epoch);
                    assert_eq!(service.engine.lock().await.posted, 0);
                    assert!(matches!(
                        *observed.borrow(),
                        super::ConnectionPhase::Refusal
                    ));
                }
            }
            service.gate.cancel(None).unwrap();
        }
        service
            .operations
            .failed
            .store(true, super::Ordering::Relaxed);
        let request = Request::decode(br#"{"op":"acquire","epoch":null}"#).unwrap();
        let error = service.dispatch(request, &phase).await.unwrap_err();
        assert_eq!(json!(error), json!(super::worker_failed()));
    }

    #[tokio::test]
    async fn an_acquire_cannot_pass_cleanup_still_in_progress() {
        for who in [super::Who::Agent, super::Who::Person] {
            let operations = super::Operations::default();
            let gate = Arc::new(Gate::default());
            let old = gate.acquire(&"a".repeat(48), who).unwrap();
            let stand = Stand::locked(&[]);
            let engine = stand.clone().lock_owned().await;
            let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
            let running = operations
                .start(async move {
                    barrier.await.unwrap();
                    drop(engine);
                })
                .await
                .unwrap();
            gate.cancel(None).unwrap();
            assert!(old.authorize().is_err());
            let cleanup = super::drain_and_release(&operations, &stand, &gate);
            let acquire = async {
                let mut engine = stand.lock().await;
                operations.check()?;
                engine.release()?;
                gate.acquire(&"b".repeat(48), who)
            };
            tokio::pin!(cleanup, acquire);
            std::future::poll_fn(|cx| {
                assert!(std::future::Future::poll(cleanup.as_mut(), cx).is_pending());
                assert!(std::future::Future::poll(acquire.as_mut(), cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            assert!(!gate.active().unwrap());
            assert_eq!(operations.tasks.lock().unwrap().len(), 1);
            resume.send(()).unwrap();
            let (cleaned, grant) = tokio::join!(cleanup, acquire);
            cleaned.unwrap();
            assert!(grant.unwrap().authorize().is_ok());
            running.await.unwrap();
        }
    }

    fn malformed_acquire_and_reset_requests() -> Vec<String> {
        let epoch = r#"{"uid":501,"securitySessionId":1,"displayId":2,"width":800.0,"height":600.0,"processId":1}"#;
        let owner = "a".repeat(48);
        let mut cases = Vec::new();
        for op in ["acquire", "reset"] {
            let prefix = format!(r#""op":"{op}","owner":"{owner}""#);
            cases.push(format!("{{{prefix}}}"));
            for value in ["null", "false", "0", r#""epoch""#, "[]", "{}"] {
                cases.push(format!(r#"{{{prefix},"epoch":{value}}}"#));
            }
            for member in ["op", "epoch"] {
                let value = if member == "op" {
                    format!(r#""{op}""#)
                } else {
                    epoch.to_owned()
                };
                for (first, second) in [
                    (value.as_str(), value.as_str()),
                    ("null", value.as_str()),
                    (value.as_str(), "null"),
                ] {
                    let fields = if member == "op" {
                        format!(r#""owner":"{owner}","epoch":{epoch}"#)
                    } else {
                        prefix.clone()
                    };
                    cases.push(format!(
                        r#"{{{fields},"{member}":{first},"{member}":{second}}}"#
                    ));
                }
            }
            let fields = serde_json::from_str::<serde_json::Map<String, Value>>(epoch).unwrap();
            for (member, valid) in &fields {
                let mut missing = fields.clone();
                missing.remove(member);
                cases.push(format!(r#"{{{prefix},"epoch":{}}}"#, json!(missing)));
                let mut invalid = vec!["null", "false", r#""1""#, "[]", "{}"];
                if member != "width" && member != "height" {
                    invalid.extend(["-1", "1.0", "18446744073709551616"]);
                }
                if member == "uid" || member == "processId" {
                    invalid.push("4294967296");
                }
                for value in invalid {
                    let mut malformed = fields.clone();
                    malformed.insert(member.clone(), serde_json::from_str(value).unwrap());
                    cases.push(format!(r#"{{{prefix},"epoch":{}}}"#, json!(malformed)));
                }
                for (first, second) in [
                    (valid.to_string(), valid.to_string()),
                    ("null".into(), valid.to_string()),
                    (valid.to_string(), "null".into()),
                ] {
                    cases.push(format!(
                        r#"{{{prefix},"epoch":{{"{member}":{first},"{member}":{second},{}}}}}"#,
                        serde_json::to_string(&missing)
                            .unwrap()
                            .trim_start_matches('{')
                            .trim_end_matches('}')
                    ));
                }
                let escaped = format!(r#"\u{:04x}{}"#, member.as_bytes()[0], &member[1..]);
                cases.push(format!(
                    r#"{{{prefix},"epoch":{{"{escaped}":{valid},{}}}}}"#,
                    epoch.trim_start_matches('{').trim_end_matches('}')
                ));
            }
            cases.push(format!(
                r#"{{{prefix},"epoch":{{"extension":1,{}}}}}"#,
                epoch.trim_start_matches('{').trim_end_matches('}')
            ));
        }
        for member in ["owner", "person"] {
            for value in ["null", "0", "1", "[]", "{}"] {
                let other = if member == "person" {
                    format!(r#""owner":"{owner}""#)
                } else {
                    r#""person":false"#.to_owned()
                };
                cases.push(format!(
                    r#"{{"op":"acquire","epoch":{epoch},{other},"{member}":{value}}}"#
                ));
            }
            let values = if member == "owner" {
                vec![format!(r#""{owner}""#), "false".into(), "null".into()]
            } else {
                vec![
                    "false".into(),
                    "true".into(),
                    r#""true""#.into(),
                    "null".into(),
                ]
            };
            for first in &values {
                for second in &values {
                    let other = if member == "person" {
                        format!(r#""owner":"{owner}","epoch":{epoch}"#)
                    } else {
                        format!(r#""epoch":{epoch}"#)
                    };
                    cases.push(format!(
                        r#"{{"op":"acquire",{other},"{member}":{first},"{member}":{second}}}"#
                    ));
                }
            }
            let value = if member == "owner" {
                "false"
            } else {
                r#""false""#
            };
            cases.push(format!(
                r#"{{"op":"acquire","epoch":{epoch},"{member}":{value}}}"#
            ));
            let escaped = if member == "owner" {
                r#"\u006fwner"#
            } else {
                r#"\u0070erson"#
            };
            let value = if member == "owner" {
                format!(r#""{owner}""#)
            } else {
                "true".into()
            };
            cases.push(format!(
                r#"{{"op":"acquire","epoch":{epoch},"{member}":{value},"{escaped}":{value}}}"#
            ));
        }
        cases
    }

    #[tokio::test]
    async fn malformed_acquire_and_reset_fields_leave_ownership_and_recovery_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        for who in [super::Who::Agent, super::Who::Person] {
            for failed in [false, true] {
                let marker = directory.path().join("stopped");
                let service = service(&marker);
                let grant = service.gate.acquire(&"a".repeat(48), who).unwrap();
                let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
                let running = service.operations.start(barrier).await.unwrap();
                service
                    .operations
                    .failed
                    .store(failed, super::Ordering::Relaxed);
                let _recovery = service.operations.recovery.lock().await;
                let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
                for body in malformed_acquire_and_reset_requests() {
                    match Request::decode(body.as_bytes()) {
                        Ok(request) => {
                            let error = service.dispatch(request, &phase).await.unwrap_err();
                            let Ok(wire) = serde_json::from_str::<Value>(&body) else {
                                panic!("invalid fixture");
                            };
                            if wire["op"] == "acquire" {
                                assert_eq!(
                                    json!(error),
                                    json!(service.check().unwrap_err()),
                                    "{body}"
                                );
                            } else if let Err(expected) =
                                crate::session::InputEpoch::deserialize(&wire["epoch"])
                            {
                                assert_eq!(json!(error), json!(Error::from(expected)), "{body}");
                            }
                        }
                        Err(error) => {
                            assert!(
                                matches!(error.code, Code::InvalidJson | Code::Input),
                                "{body}"
                            );
                            assert!(!error.uncertain, "{body}");
                        }
                    }
                    assert!(grant.authorize().is_ok(), "{body}");
                    assert!(!service.gate.held().unwrap(), "{body}");
                    assert!(!marker.exists(), "{body}");
                    let engine = service.engine.lock().await;
                    assert_eq!(engine.posted, 0, "{body}");
                    assert!(!engine.held_input(), "{body}");
                    assert!(engine.grant.is_none(), "{body}");
                    assert_eq!(engine.epoch, service.epoch().unwrap(), "{body}");
                    assert_eq!(
                        service.operations.failed.load(super::Ordering::Relaxed),
                        failed,
                        "{body}"
                    );
                    assert_eq!(service.operations.tasks.lock().unwrap().len(), 1, "{body}");
                    assert!(service.operations.recovery.try_lock().is_err(), "{body}");
                    assert!(
                        matches!(*observed.borrow(), super::ConnectionPhase::Refusal),
                        "{body}"
                    );
                }
                resume.send(()).unwrap();
                running.await.unwrap().unwrap();
                service
                    .operations
                    .failed
                    .store(false, super::Ordering::Relaxed);
                service.operations.drain().await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn lifecycle_members_preserve_last_value_refusals_and_effects()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("stopped");
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let pid = std::process::id();
        let mut cases = Vec::new();
        for fields in [
            "",
            r#""op":null"#,
            r#""op":false"#,
            r#""op":0"#,
            r#""op":[]"#,
            r#""op":{}"#,
            r#""op":"hold","op":"hello","on":true"#,
            r#""op":"hello","op":"hold","on":true"#,
            r#""op":"hello","op":"hello""#,
            r#""op":null,"op":"hello""#,
            r#""op":"hello","op":null"#,
            r#""op":"hello","\u006fp":"hello""#,
        ] {
            cases.push(format!("{{{fields}}}\n"));
        }
        for fields in [
            "",
            r#", "pid":null"#,
            r#", "pid":false"#,
            r#", "pid":"1""#,
            r#", "pid":-1"#,
            r#", "pid":1.0"#,
            r#", "pid":[]"#,
            r#", "pid":{}"#,
        ] {
            cases.push(format!(r#"{{"op":"shutdown"{fields}}}"#));
        }
        for fields in [
            format!(r#""pid":{pid},"pid":0"#),
            format!(r#""pid":0,"pid":{pid}"#),
            format!(r#""pid":{pid},"pid":{pid}"#),
            format!(r#""pid":{pid},"\u0070id":{pid}"#),
        ] {
            cases.push(format!(r#"{{"op":"shutdown",{fields}}}"#));
        }
        for operation in ["release", "renew"] {
            for fields in [
                r#""owner":"a","owner":"b""#,
                r#""owner":"b","owner":"a""#,
                r#""owner":"a","owner":"a""#,
                r#""owner":null,"owner":"a""#,
                r#""owner":"a","owner":null"#,
                r#""owner":"a","\u006fwner":"a""#,
            ] {
                cases.push(format!(r#"{{"op":"{operation}",{fields}}}"#));
            }
        }
        for stopped in [false, true] {
            for body in &cases {
                let parsed: Value = serde_json::from_str(body)?;
                let canonical = parsed.to_string();
                let reference_marker = directory.path().join("reference");
                let reference = self::service(&reference_marker);
                let actual = self::service(&marker);
                for (instance, path) in [(&reference, &reference_marker), (&actual, &marker)] {
                    instance.gate.acquire(&"a".repeat(48), super::Who::Person)?;
                    instance.gate.hold(stopped)?;
                    if stopped {
                        std::fs::write(path, b"stop")?;
                    } else if path.exists() {
                        std::fs::remove_file(path)?;
                    }
                }
                let (reference_phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
                let reference_result = match Request::decode(canonical.as_bytes()) {
                    Ok(request) => reference.dispatch(request, &reference_phase).await,
                    Err(error) => Err(error),
                };
                let actual_result = match deliver(format!("{body}\n").into_bytes(), 64).await {
                    Ok(request) => actual.dispatch(request, &phase).await,
                    Err(error) => Err(error),
                };
                let refusal = match parsed["op"].as_str() {
                    None => Some((Code::Input, "Native operation requires a string op")),
                    Some("shutdown") if parsed["pid"].as_u64() != Some(u64::from(pid)) => Some((
                        Code::Identity,
                        "Shutdown names a different input service process",
                    )),
                    _ => None,
                };
                if let Some((code, message)) = refusal {
                    let Err(error) = &actual_result else {
                        panic!("invalid lifecycle field was accepted: {body}");
                    };
                    assert_eq!(error.code, code, "{body}");
                    assert_eq!(error.error, message, "{body}");
                    assert!(!error.uncertain, "{body}");
                }
                assert_eq!(json!(actual_result), json!(reference_result), "{body}");
                assert_eq!(actual.gate.active()?, reference.gate.active()?, "{body}");
                assert_eq!(actual.gate.held()?, reference.gate.held()?, "{body}");
                assert_eq!(marker.exists(), reference_marker.exists(), "{body}");
                let engine = actual.engine.lock().await;
                assert_eq!(engine.posted, 0, "{body}");
                assert!(!engine.held_input(), "{body}");
                if matches!(*observed.borrow(), super::ConnectionPhase::Refusal) {
                    assert!(actual.gate.active()?, "{body}");
                    assert_eq!(actual.gate.held()?, stopped, "{body}");
                }
                phase.send_replace(super::ConnectionPhase::Refusal);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn array_shaped_lifecycle_requests_are_refused_without_effects() {
        let owner = "a".repeat(48);
        let cases = [
            r#"["hello"]"#.to_owned(),
            format!(r#"["shutdown",{}]"#, std::process::id()),
            r#"["hold",true]"#.to_owned(),
            r#"["hold",false]"#.to_owned(),
            format!(r#"["release","{owner}"]"#),
            format!(r#"["renew","{owner}"]"#),
        ];
        for owned in [false, true] {
            for stopped in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let marker = directory.path().join("stopped");
                let service = service(&marker);
                let grant =
                    owned.then(|| service.gate.acquire(&owner, super::Who::Person).unwrap());
                service.gate.hold(stopped).unwrap();
                if stopped {
                    std::fs::write(&marker, b"stop").unwrap();
                }
                let epoch = service.epoch().unwrap();
                let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
                for body in &cases {
                    let result = match deliver(format!(" \t{body}\n").into_bytes(), 1).await {
                        Ok(request) => service.dispatch(request, &phase).await,
                        Err(error) => Err(error),
                    };
                    let error = result.unwrap_err();
                    assert_eq!(error.code, Code::Input, "{body}");
                    assert_eq!(
                        error.error, "Native operation requires a string op",
                        "{body}"
                    );
                    assert!(!error.uncertain, "{body}");
                    assert!(
                        matches!(*observed.borrow(), super::ConnectionPhase::Refusal),
                        "{body}"
                    );
                    assert_eq!(service.gate.active().unwrap(), owned, "{body}");
                    if let Some(grant) = &grant {
                        assert!(grant.authorize().is_ok(), "{body}");
                    }
                    assert_eq!(service.gate.held().unwrap(), stopped, "{body}");
                    assert_eq!(service.epoch().unwrap(), epoch, "{body}");
                    assert_eq!(service.engine.lock().await.posted, 0, "{body}");
                    assert!(service.operations.check().is_ok(), "{body}");
                    if stopped {
                        assert_eq!(std::fs::read(&marker).unwrap(), b"stop", "{body}");
                    } else {
                        assert!(!marker.exists(), "{body}");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn lifecycle_fields_keep_their_operation_specific_refusals() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(&directory.path().join("stopped"));
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let grant = service
            .gate
            .acquire(&"a".repeat(48), super::Who::Agent)
            .unwrap();
        for fields in [
            "",
            r#", "owner":null"#,
            r#", "owner":true"#,
            r#", "owner":1"#,
            r#", "owner":[]"#,
            r#", "owner":{}"#,
            r#", "owner":"""#,
            r#", "owner":"not-an-owner""#,
        ] {
            for op in ["release", "renew"] {
                let request =
                    Request::decode(format!(r#"{{"op":"{op}"{fields}}}"#).as_bytes()).unwrap();
                let error = service.dispatch(request, &phase).await.unwrap_err();
                if op == "release" {
                    assert_eq!(error.code, Code::LeaseLost);
                    assert_eq!(error.error, "This token cannot release another controller");
                } else {
                    assert_eq!(json!(error), json!(service.check().unwrap_err()));
                }
                assert!(!error.uncertain);
                assert!(grant.authorize().is_ok());
                assert!(matches!(
                    *observed.borrow(),
                    super::ConnectionPhase::Refusal
                ));
            }
        }
        let request = Request::decode(br#"{"op":"shutdown","pid":0}"#).unwrap();
        let error = service.dispatch(request, &phase).await.unwrap_err();
        assert_eq!(error.code, Code::Identity);
        assert_eq!(
            error.error,
            "Shutdown names a different input service process"
        );
        let request = Request::Shutdown {
            pid: std::process::id().into(),
        };
        let error = service.dispatch(request, &phase).await.unwrap_err();
        assert_eq!(error.code, Code::Busy);
        assert!(grant.authorize().is_ok());
    }

    #[test]
    fn lifecycle_payloads_ignore_only_unrelated_extension_members() {
        assert!(matches!(
            Request::decode(br#"{"op":"hello","on":null,"on":true,"pid":0,"pid":1}"#).unwrap(),
            Request::Hello
        ));
        assert!(matches!(
            Request::decode(br#"{"pid":42,"on":null,"on":false,"op":"shutdown"}"#).unwrap(),
            Request::Shutdown { pid: 42 }
        ));
        assert!(matches!(
            Request::decode(br#"{"on":true,"pid":null,"pid":false,"op":"hold"}"#).unwrap(),
            Request::Hold { on: Ok(true) }
        ));
        for op in ["release", "renew"] {
            let body = format!(
                r#"{{"owner":"unvalidated-token","on":null,"on":false,"op":"{op}","other":1,"other":2}}"#
            );
            let (Request::Release { owner } | Request::Renew { owner }) =
                Request::decode(body.as_bytes()).unwrap()
            else {
                panic!("expected an owner");
            };
            assert_eq!(owner, "unvalidated-token", "the gate owns token identity");
        }
    }

    #[tokio::test]
    async fn stop_and_release_remain_available_when_ordinary_permission_checks_fail() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("stopped");
        let service = service(&marker);
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        assert!(
            service.check().is_err(),
            "the synthetic session is never authorized"
        );
        let owner = "a".repeat(48);
        let grant = service.gate.acquire(&owner, super::Who::Agent).unwrap();

        let request = Request::decode(br#"{"op":"hold","on":true}"#).unwrap();
        assert_eq!(
            service.dispatch(request, &phase).await.unwrap(),
            json!({"ok": true, "held": true, "kept": true})
        );
        assert!(marker.exists());
        assert!(service.gate.held().unwrap());
        assert!(grant.authorize().is_err());
        assert!(matches!(
            *observed.borrow(),
            super::ConnectionPhase::Mutation("hold")
        ));

        let request = Request::decode(br#"{"op":"hold","on":false}"#).unwrap();
        service.dispatch(request, &phase).await.unwrap();
        let grant = service.gate.acquire(&owner, super::Who::Agent).unwrap();
        let request =
            Request::decode(format!(r#"{{"op":"release","owner":"{owner}"}}"#).as_bytes()).unwrap();
        assert_eq!(
            service.dispatch(request, &phase).await.unwrap(),
            json!({"ok": true, "released": true})
        );
        assert!(grant.authorize().is_err());
        assert!(matches!(
            *observed.borrow(),
            super::ConnectionPhase::Mutation("release")
        ));

        let error = service
            .dispatch(Request::Renew { owner }, &phase)
            .await
            .unwrap_err();
        assert_eq!(error.code, service.check().unwrap_err().code);
        assert_ne!(
            error.code,
            Code::LeaseLost,
            "renew still checks the session first"
        );
        assert_eq!(service.engine.lock().await.posted, 0);
    }

    #[tokio::test]
    async fn shutdown_rechecks_ownership_after_draining() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(&directory.path().join("stopped"));
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let (resume, waiting) = tokio::sync::oneshot::channel::<()>();
        let work = service
            .operations
            .start(async move { waiting.await.unwrap() })
            .await
            .unwrap();
        let request = Request::decode(
            format!(r#"{{"op":"shutdown","pid":{}}}"#, std::process::id()).as_bytes(),
        )
        .unwrap();
        let dispatch = service.dispatch(request, &phase);
        tokio::pin!(dispatch);
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(dispatch.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        let grant = service
            .gate
            .acquire(&"a".repeat(48), super::Who::Agent)
            .unwrap();
        resume.send(()).unwrap();
        let error = dispatch.await.unwrap_err();
        work.await.unwrap();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.error, "The input service has an active controller");
        assert!(!error.uncertain);
        assert!(matches!(
            *observed.borrow(),
            super::ConnectionPhase::Refusal
        ));
        assert!(grant.authorize().is_ok());
        assert_eq!(service.engine.lock().await.posted, 0);
    }

    #[tokio::test]
    async fn release_cancels_ownership_before_waiting_for_the_engine() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(&directory.path().join("stopped"));
        let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
        let owner = "a".repeat(48);
        let grant = service.gate.acquire(&owner, super::Who::Agent).unwrap();
        let engine = service.engine.lock().await;
        let request = Request::Release { owner };
        let dispatch = service.dispatch(request, &phase);
        tokio::pin!(dispatch);
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(dispatch.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        assert!(grant.authorize().is_err());
        assert!(service.engine.try_lock().is_err());
        drop(engine);
        assert_eq!(
            dispatch.await.unwrap(),
            json!({"ok": true, "released": true})
        );
        service
            .gate
            .acquire(&"b".repeat(48), super::Who::Agent)
            .unwrap();
    }

    #[tokio::test]
    async fn a_failed_marker_removal_keeps_agents_stopped() {
        let directory = tempfile::tempdir().unwrap();
        let service = service(directory.path());
        let (phase, _) = super::watch::channel(super::ConnectionPhase::Refusal);
        let request = Request::decode(br#"{"op":"hold","on":true}"#).unwrap();
        assert_eq!(
            service.dispatch(request, &phase).await.unwrap(),
            json!({"ok": true, "held": true, "kept": false})
        );
        let request = Request::decode(br#"{"op":"hold","on":false}"#).unwrap();
        let error = service.dispatch(request, &phase).await.unwrap_err();
        assert_eq!(error.code, Code::Native);
        assert_eq!(
            error.error,
            "Agents could not be resumed: the helper could not clear the stop"
        );
        assert!(!error.uncertain);
        assert!(service.gate.held().unwrap());
        assert!(directory.path().is_dir());
        assert_eq!(service.engine.lock().await.posted, 0);
    }

    #[tokio::test]
    async fn lifecycle_replies_keep_their_exact_line_framing() {
        for (result, expected) in [
            (Ok(json!({"ok":true,"held":true,"kept":true})), b"{\"held\":true,\"kept\":true,\"ok\":true}\n".as_slice()),
            (Request::decode(br#"{"op":"hold","on":null}"#).and_then(|request| match request { Request::Hold { on } => on.map(|_| json!({})), _ => panic!("expected hold") }), b"{\"code\":\"input\",\"error\":\"Hold requires a boolean on\",\"ok\":false,\"uncertain\":false}\n".as_slice()),
        ] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            super::reply(&mut server, result).await.unwrap();
            drop(server);
            let mut bytes = Vec::new();
            super::AsyncReadExt::read_to_end(&mut client, &mut bytes).await.unwrap();
            assert_eq!(bytes, expected);
        }
    }

    #[tokio::test]
    async fn an_abandoned_read_does_not_degrade_the_service() {
        let operations = super::Operations::default();
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        for malformed in [false, true] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            client
                .write_all(if malformed {
                    b"{\n"
                } else {
                    b"{\"op\":\"hello\"}\n"
                })
                .await
                .unwrap();
            drop(client);
            let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
            let slot = slots.clone().acquire_owned().await.unwrap();
            let mut tasks = super::JoinSet::new();
            let task = tasks.spawn(async move {
                let _slot = slot;
                let result = match read_request(&mut server).await {
                    Ok(request) => {
                        assert!(matches!(request, Request::Hello));
                        phase.send_replace(super::ConnectionPhase::Read);
                        Ok(json!({"ok": true}))
                    }
                    Err(error) => Err(error),
                };
                super::reply(&mut server, result).await
            });
            operations
                .connections
                .lock()
                .unwrap()
                .insert(task.id(), (observed, false));
            let outcome = tasks.join_next_with_id().await.unwrap();
            assert!(matches!(outcome, Ok((_, Err(_)))));
            super::connection_outcome(outcome, &operations, false).unwrap();
            assert!(operations.connections.lock().unwrap().is_empty());
            assert_eq!(slots.available_permits(), 1);
            operations.check().unwrap();
        }
        operations.start(async { 42 }).await.unwrap().await.unwrap();
        operations.drain().await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_refusal_reply_does_not_degrade_the_service()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("stopped");
        let service = Arc::new(service(&marker));
        let (mut client, mut server) = UnixStream::pair()?;
        client.write_all(b"{\"op\":\"hold\",\"on\":null}\n").await?;
        drop(client);
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let mut tasks = super::JoinSet::new();
        let serving = service.clone();
        let task = tasks.spawn(async move {
            let request = read_request(&mut server).await?;
            let refusal = serving.dispatch(request, &phase).await;
            let Err(error) = &refusal else {
                panic!("expected a hold refusal");
            };
            assert_eq!(error.code, Code::Input);
            assert_eq!(error.error, "Hold requires a boolean on");
            assert!(!error.uncertain);
            assert!(matches!(*phase.borrow(), super::ConnectionPhase::Refusal));
            super::reply(&mut server, refusal).await
        });
        service
            .operations
            .connections
            .lock()
            .map_err(|_| "connection tracking failed")?
            .insert(task.id(), (observed, false));
        let outcome = tasks
            .join_next_with_id()
            .await
            .ok_or("connection missing")?;
        assert!(matches!(outcome, Ok((_, Err(_)))));
        super::connection_outcome(outcome, &service.operations, false)?;
        service.operations.check()?;
        assert!(!service.gate.held()?);
        assert!(!marker.exists());
        assert_eq!(service.engine.lock().await.posted, 0);
        Ok(())
    }

    #[tokio::test]
    async fn an_abandoned_ownership_refusal_keeps_the_current_controller_healthy() {
        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        for operation in ["release", "renew", "input"] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            client
                .write_all(
                    format!("{}\n", json!({"op": operation, "owner": "b".repeat(48)})).as_bytes(),
                )
                .await
                .unwrap();
            drop(client);
            let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
            let refused = gate.clone();
            let mut tasks = super::JoinSet::new();
            let task = tasks.spawn(async move {
                let _phase = phase;
                let request = read_request(&mut server).await?;
                let refusal = match request {
                    Request::Renew { owner } => refused.renew(&owner).unwrap_err(),
                    Request::Release { owner } => refused.cancel(Some(&owner)).unwrap_err(),
                    Request::Input { owner, .. } => refused.cancel(Some(&owner)).unwrap_err(),
                    _ => panic!("expected an ownership request"),
                };
                assert_eq!(refusal.code, Code::LeaseLost);
                assert!(!refusal.uncertain);
                super::reply(&mut server, Err(refusal)).await
            });
            operations
                .connections
                .lock()
                .unwrap()
                .insert(task.id(), (observed, false));
            let outcome = tasks.join_next_with_id().await.unwrap();
            assert!(matches!(outcome, Ok((_, Err(_)))));
            super::connection_outcome(outcome, &operations, false).unwrap();
            operations.check().unwrap();
            assert!(gate.active().unwrap());
            drop(grant.authorize().unwrap());
            gate.renew(&owner).unwrap();
        }
        operations.drain().await.unwrap();
    }

    #[tokio::test]
    async fn a_handler_panic_is_observed() {
        for stopping in [false, true] {
            let operations = super::Operations::default();
            let mut tasks = super::JoinSet::<Result<()>>::new();
            let (phase, observed) =
                super::watch::channel(super::ConnectionPhase::Mutation("input"));
            let task = tasks.spawn(async move {
                let _phase = phase;
                panic!("private owner and typed text");
            });
            operations
                .connections
                .lock()
                .unwrap()
                .insert(task.id(), (observed, false));
            let outcome = tasks.join_next_with_id().await.unwrap();
            assert!(outcome.as_ref().unwrap_err().is_panic());
            let error = super::connection_outcome(outcome, &operations, stopping).unwrap_err();
            assert_eq!(error.code, Code::Uncertain);
            assert!(error.uncertain);
            assert!(operations.check().unwrap_err().uncertain);
            assert!(operations.drain().await.unwrap_err().uncertain);
            assert!(operations.connections.lock().unwrap().is_empty());
            for diagnostic in [json!(error).to_string(), format!("{error:?}")] {
                assert!(!diagnostic.contains("private owner and typed text"));
                assert!(diagnostic.len() < 256);
            }
        }
    }

    #[tokio::test]
    async fn a_lost_mutation_reply_keeps_recovery_pending() {
        let operations = Arc::new(super::Operations::default());
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let engine = Stand::locked(&[]);
        let mut tasks = super::JoinSet::new();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let (effect, effected) = tokio::sync::oneshot::channel();
        let (deliver, delivery) = tokio::sync::oneshot::channel::<()>();
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);
        let performed = engine.clone();
        let granted = grant.clone();
        let working = operations.clone();
        let task = tasks.spawn(async move {
            let request = read_request(&mut server).await?;
            assert!(matches!(request, Request::Input { .. }));
            phase.send_replace(super::ConnectionPhase::Mutation("input"));
            let work = working
                .start(async move {
                    drop(granted.authorize().unwrap());
                    let mut engine = performed.lock().await;
                    engine.posted += 1;
                    engine.held = true;
                })
                .await?;
            work.await.unwrap();
            effect.send(()).unwrap();
            delivery.await.unwrap();
            super::reply(&mut server, Ok(json!({"ok": true, "performed": 1}))).await
        });
        operations
            .connections
            .lock()
            .unwrap()
            .insert(task.id(), (observed, false));
        assert_eq!(engine.lock().await.posted, 0);
        client.write_all(b"{\"op\":\"input\"}\n").await.unwrap();
        effected.await.unwrap();
        drop(client);
        deliver.send(()).unwrap();
        let error =
            super::connection_outcome(tasks.join_next_with_id().await.unwrap(), &operations, false)
                .unwrap_err();
        assert!(error.uncertain);
        assert!(gate.active().unwrap());
        assert!(grant.authorize().is_ok());
        assert!(operations.start(async {}).await.unwrap_err().uncertain);
        assert_eq!(engine.lock().await.posted, 1);
        gate.cancel(Some(&owner)).unwrap();
        assert!(
            super::drain_and_release(&operations, &engine, &gate)
                .await
                .unwrap_err()
                .uncertain
        );
        assert!(!engine.lock().await.held);
        assert!(operations.check().unwrap_err().uncertain);
        super::reset(&operations, &engine, &gate, Perform::release)
            .await
            .unwrap();
        operations.check().unwrap();
        assert_eq!(
            engine.lock().await.posted,
            1,
            "recovery never replays input"
        );
    }

    #[tokio::test]
    async fn reset_settles_completed_connections_without_hiding_later_failures() {
        for late in [false, true] {
            for panic in [false, true] {
                tokio::time::timeout(Duration::from_secs(5), async {
                    let operations = Arc::new(super::Operations::default());
                    let gate = Arc::new(Gate::default());
                    let old_grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
                    let engine = Stand::locked(&[]);
                    let (client, mut server) = UnixStream::pair().unwrap();
                    let (effect, effected) = tokio::sync::oneshot::channel();
                    let (deliver, delivery) = tokio::sync::oneshot::channel();
                    let mut deliver = Some(deliver);
                    let (phase, observed) =
                        super::watch::channel(super::ConnectionPhase::Mutation("input"));
                    let mut completed = observed.clone();
                    let working = operations.clone();
                    let performed = engine.clone();
                    let mut tasks = super::JoinSet::new();
                    let task = tasks.spawn(async move {
                        let _phase = phase;
                        working
                            .start(async move {
                                let mut engine = performed.lock().await;
                                engine.posted += 1;
                                engine.held = true;
                            })
                            .await?
                            .await
                            .unwrap();
                        effect.send(()).unwrap();
                        delivery.await.unwrap();
                        assert!(!panic, "private connection failure");
                        super::reply(&mut server, Ok(json!({"ok": true, "performed": 1}))).await
                    });
                    operations
                        .connections
                        .lock()
                        .unwrap()
                        .insert(task.id(), (observed, false));
                    effected.await.unwrap();
                    drop(client);
                    if !late {
                        deliver.take().unwrap().send(()).unwrap();
                        completed.changed().await.unwrap_err();
                    }
                    super::reset(&operations, &engine, &gate, Perform::release)
                        .await
                        .unwrap();
                    operations.check().unwrap();
                    assert!(old_grant.authorize().is_err());
                    assert!(!engine.lock().await.held);
                    if late {
                        deliver.take().unwrap().send(()).unwrap();
                        completed.changed().await.unwrap_err();
                    }
                    let outcome = tasks.join_next_with_id().await.unwrap();
                    if panic {
                        assert!(outcome.as_ref().unwrap_err().is_panic());
                    } else {
                        assert!(matches!(outcome, Ok((_, Err(_)))));
                    }
                    assert_eq!(
                        super::connection_outcome(outcome, &operations, false).is_err(),
                        late
                    );
                    assert_eq!(operations.check().is_err(), late);
                    assert!(operations.connections.lock().unwrap().is_empty());
                    if !late {
                        let grant = gate.acquire(&"b".repeat(48), super::Who::Agent).unwrap();
                        operations
                            .start(async move {
                                drop(grant.authorize().unwrap());
                            })
                            .await
                            .unwrap()
                            .await
                            .unwrap();
                        operations.drain().await.unwrap();
                    }
                    assert_eq!(
                        engine.lock().await.posted,
                        1,
                        "recovery never replays input"
                    );
                })
                .await
                .expect("connection recovery did not settle");
            }
        }
    }

    #[tokio::test]
    async fn shutdown_classifies_cancelled_handlers_and_joins_owned_work() {
        for lost_reply in [false, true] {
            let operations = super::Operations::default();
            let gate = Arc::new(Gate::default());
            let _grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
            let engine = Stand::locked(&[]);
            let held = engine.clone().lock_owned().await;
            let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
            let work = operations
                .start(async move {
                    let mut held = held;
                    barrier.await.unwrap();
                    held.held = true;
                })
                .await
                .unwrap();
            let mut tasks = super::JoinSet::new();
            let (phase, observed) =
                super::watch::channel(super::ConnectionPhase::Mutation("input"));
            let idle = tasks.spawn(async move {
                let _phase = phase;
                std::future::pending::<Result<()>>().await
            });
            operations
                .connections
                .lock()
                .unwrap()
                .insert(idle.id(), (observed, false));
            if lost_reply {
                let (finished, received) = tokio::sync::oneshot::channel();
                let (phase, observed) =
                    super::watch::channel(super::ConnectionPhase::Mutation("release"));
                let task = tasks.spawn(async move {
                    let _phase = phase;
                    finished.send(()).unwrap();
                    Err(Error::new(Code::Transport, "private transport cause"))
                });
                operations
                    .connections
                    .lock()
                    .unwrap()
                    .insert(task.id(), (observed, false));
                received.await.unwrap();
            }
            let cleanup = super::finish(&gate, &mut tasks, &operations, &engine);
            tokio::pin!(cleanup);
            std::future::poll_fn(|cx| {
                assert!(std::future::Future::poll(cleanup.as_mut(), cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            assert!(!gate.active().unwrap());
            assert!(gate.acquire(&"b".repeat(48), super::Who::Agent).is_err());
            assert!(engine.try_lock().is_err());
            resume.send(()).unwrap();
            let result = tokio::time::timeout(Duration::from_secs(5), cleanup)
                .await
                .expect("shutdown did not cancel handlers and drain work");
            assert_eq!(result.is_err(), lost_reply);
            assert_eq!(operations.check().is_err(), lost_reply);
            tokio::time::timeout(Duration::from_secs(5), work)
                .await
                .expect("registered work did not report its outcome")
                .unwrap();
            assert!(operations.tasks.lock().unwrap().is_empty());
            assert!(!engine.lock().await.held);
            assert_eq!(engine.lock().await.releases, 1);
        }
    }

    #[tokio::test]
    async fn stop_fields_use_the_last_boolean_and_invalid_values_leave_the_stop_unchanged()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let marker = dir.path().join("stopped");
        let gate = Gate::default();
        let (phase, observed) = super::watch::channel(super::ConnectionPhase::Refusal);

        for stopped in [true, false] {
            let reply = hold(&gate, &marker, stopped, &phase)?;
            assert_eq!(reply, json!({"ok": true, "held": stopped, "kept": true}));
            if stopped {
                std::fs::write(&marker, b"stop")?;
            }
            for fields in [
                r#", "on": true, "on": false"#,
                r#", "on": false, "on": true"#,
                r#", "on": true, "on": true"#,
                r#", "on": false, "on": false"#,
                r#", "on": true, "\u006fn": false"#,
                r#", "\u006f\u006e": false, "on": true"#,
                r#", "on": null, "on": true"#,
                r#", "on": false, "on": null"#,
                "",
                r#", "on": null"#,
                r#", "on": "false""#,
                r#", "on": 0"#,
                r#", "on": []"#,
                r#", "on": {}"#,
            ] {
                let body = format!("{{\"op\":\"hold\"{fields}}}\n");
                phase.send_replace(super::ConnectionPhase::Refusal);
                let result = match deliver(body.clone().into_bytes(), 64).await {
                    Ok(Request::Hold { on }) => on.and_then(|on| hold(&gate, &marker, on, &phase)),
                    Ok(_) => panic!("expected hold"),
                    Err(error) => Err(error),
                };
                let wire: Value = serde_json::from_str(&body)?;
                if let Some(on) = wire["on"].as_bool() {
                    assert_eq!(result?, json!({"ok":true,"held":on,"kept":true}), "{body}");
                    assert_eq!(gate.held()?, on, "{body}");
                    assert_eq!(marker.exists(), on, "{body}");
                    hold(&gate, &marker, stopped, &phase)?;
                    if stopped {
                        std::fs::write(&marker, b"stop")?;
                    }
                    continue;
                }
                let error = result.err().ok_or("expected refusal")?;
                assert_eq!(error.code, Code::Input, "{body}");
                assert_eq!(error.error, "Hold requires a boolean on");
                assert!(!error.uncertain, "{body}");
                assert!(matches!(
                    *observed.borrow(),
                    super::ConnectionPhase::Refusal
                ));
                assert_eq!(gate.held()?, stopped, "{body}");
                if stopped {
                    assert_eq!(std::fs::read(&marker)?, b"stop", "{body}");
                } else {
                    assert!(!marker.exists(), "{body}");
                }
            }
        }

        for on in [true, false] {
            let body = format!("{{\"op\":\"hold\",\"on\":{on},\"other\":1,\"other\":2}}\n");
            let Request::Hold { on } = deliver(body.into_bytes(), 64).await? else {
                panic!("expected hold");
            };
            let on = on?;
            let reply = hold(&gate, &marker, on, &phase)?;
            assert_eq!(reply, json!({"ok": true, "held": on, "kept": true}));
            assert_eq!(gate.held()?, on);
            assert_eq!(marker.exists(), on);
        }
        Ok(())
    }

    /// What a stand-in engine does at one action.
    #[derive(Clone, Copy, PartialEq)]
    enum Step {
        Post,
        Refuse,
        Unreadable,
        /// Post one event, then fail.
        Break,
        UnacknowledgedPress,
        DockRefusal,
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
        clock: Option<Instant>,
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
                    clock: None,
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
        fn now(&self) -> Instant {
            self.clock.unwrap_or_else(Instant::now)
        }

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
            if let Some(clock) = &mut self.clock {
                *clock += self.check_delay;
            } else if !self.check_delay.is_zero() {
                tokio::time::sleep(self.check_delay).await;
            }
            if prepare && self.sheet {
                return Err(Error::new(Code::FocusChanged, "A sheet blocks the window")
                    .with("reason", json!("sheet")));
            }
            self.asked.push(expect.clone());
            let index = self.logged("check");
            self.log.push(format!("check {index}"));
            if self.script.get(index) == Some(&Step::Refuse) {
                return Err(
                    Error::new(Code::FocusChanged, "Calculator has keyboard focus")
                        .with("reason", json!("app"))
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
            if self.script.get(index) == Some(&Step::UnacknowledgedPress) {
                return Err(
                    Error::new(Code::Uncertain, "The Dock did not acknowledge the press")
                        .uncertain(),
                );
            }
            if self.script.get(index) == Some(&Step::DockRefusal) {
                return Err(Error::new(
                    Code::Input,
                    "No pressable Dock item at that point; no click was sent",
                ));
            }
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

    #[tokio::test]
    async fn a_refused_or_unacknowledged_dock_click_reports_no_posted_events() {
        for (step, uncertain, code) in [
            (Step::DockRefusal, false, "input"),
            (Step::UnacknowledgedPress, true, "uncertain"),
        ] {
            let stand = Stand::locked(&[step]);
            let gate = Gate::default();
            let action =
                serde_json::from_value(json!({"type": "click", "x": 1138, "y": 940})).unwrap();
            let mut engine = stand.lock().await;
            let reply = tokio::time::timeout(
                Duration::from_secs(1),
                run(&mut *engine, &gate, vec![action], None),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(reply["ok"], false);
            assert_eq!(reply["code"], code);
            assert_eq!(reply["performed"], 0);
            assert_eq!(reply["partial"], false);
            assert_eq!(reply["uncertain"], uncertain);
            assert_eq!(engine.posted, 0);
            assert_eq!(engine.releases, 1);
        }
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
    async fn later_keys_keep_app_only_checks_while_pointer_and_text_keep_their_own_targets() {
        let expect = Expect::new(812)
            .unwrap()
            .in_window(42)
            .unwrap()
            .in_field("Search")
            .unwrap();
        let stand = ran_actions(
            vec![
                json!({"type":"key","key":"tab"}),
                json!({"type":"pointer","phase":"down","x":10,"y":20}),
                json!({"type":"pointer","phase":"move","x":30,"y":40}),
                json!({"type":"pointer","phase":"up","x":30,"y":40}),
                json!({"type":"wait","ms":0}),
                json!({"type":"key","key":"return"}),
                json!({"type":"type","text":"a"}),
            ],
            &expect,
        )
        .await;
        let engine = stand.lock().await;
        assert_eq!(engine.posted, 7);
        let full = (Some(42), Some("Search".to_owned()));
        assert_eq!(
            asked_for(&engine),
            [
                full.clone(),
                full.clone(),
                full.clone(),
                full.clone(),
                (None, None),
                full
            ]
        );
        assert!(engine.asked.iter().all(|asked| asked.pid() == 812));
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
    async fn deliver(line: Vec<u8>, chunk: usize) -> Result<Request> {
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
            assert!(matches!(reply, Request::Hello));
            let error = deliver(padded(MAX_REQUEST + 1), chunk).await.unwrap_err();
            assert_eq!(error.code, "protocol");
        }
    }

    #[tokio::test]
    async fn the_other_routes_still_ignore_stop_fields() {
        let request = deliver(
            b"{\"op\":\"hello\",\"on\":true,\"on\":false}\n".to_vec(),
            64,
        )
        .await
        .unwrap();
        assert!(matches!(request, Request::Hello));
        assert_eq!(
            deliver(b"[]\n".to_vec(), 64).await.unwrap_err().code,
            Code::Input
        );
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
                    owner: Some(owner),
                };
                let mut engine = engine.lock_owned().await;
                let outcome = operations
                    .start(async move { run(&mut *engine, &gate, keys(1), app().as_ref()).await })
                    .await
                    .unwrap();
                let reply = outcome.await.unwrap();
                cancel.owner = None;
                *observed.lock().unwrap() = Some(reply);
                Ok(())
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
                let engine = engine.lock().await;
                assert!(engine.releases >= 1, "cleanup acknowledged release first");
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
            requests.join_next().await.unwrap().unwrap().unwrap();
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
    async fn aborting_a_request_after_registration_keeps_its_work_until_cleanup() {
        use std::{future::Future, task::Context, task::Poll, task::Waker};

        let operations = super::Operations::default();
        let gate = Arc::new(Gate::default());
        let owner = "a".repeat(48);
        let grant = gate.acquire(&owner, super::Who::Agent).unwrap();
        let stand = Stand::locked(&[]);
        let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
        {
            let request = async {
                let _cancel = super::Cancellation {
                    gate: gate.clone(),
                    owner: Some(owner),
                };
                let engine = stand.clone().lock_owned().await;
                operations
                    .start(async move {
                        let _engine = engine;
                        barrier.await.unwrap();
                        grant.authorize().map(drop)
                    })
                    .await
                    .unwrap()
                    .await
            };
            tokio::pin!(request);
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(request.as_mut().poll(&mut context), Poll::Pending));
            assert_eq!(operations.tasks.lock().unwrap().len(), 1);
        }
        assert!(!gate.active().unwrap());
        assert!(stand.try_lock().is_err());
        {
            let cleanup = super::drain_and_release(&operations, &stand, &gate);
            tokio::pin!(cleanup);
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(cleanup.as_mut().poll(&mut context), Poll::Pending));
            assert_eq!(operations.tasks.lock().unwrap().len(), 1);
            resume.send(()).unwrap();
            cleanup.await.unwrap();
        }
        assert!(operations.tasks.lock().unwrap().is_empty());
        let engine = stand.lock().await;
        assert_eq!(engine.releases, 1);
        assert_eq!(engine.posted, 0);
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
        let stand = Stand::locked(&[]);
        let error = super::drain_and_release(&operations, &stand, &Gate::default())
            .await
            .unwrap_err();
        assert_eq!(error.error, "The input worker did not report its outcome");
        assert!(error.uncertain);
        assert_eq!(stand.lock().await.releases, 1);
        assert!(operations.check().is_err());
    }

    #[test]
    fn successful_cleanup_and_single_failures_keep_their_original_shapes() {
        use std::error::Error as _;

        super::combine_cleanup(Ok(()), Ok(())).unwrap();
        for primary in [false, true] {
            let error = Error::new(Code::Busy, "Original failure")
                .held_by(crate::error::Holder::Agent)
                .with("detail", json!({"count": 3}))
                .uncertain()
                .with_source(std::io::Error::from_raw_os_error(13));
            let expected = serde_json::to_vec(&error).unwrap();
            let error = if primary {
                super::combine_cleanup(Err(error), Ok(()))
            } else {
                super::combine_cleanup(Ok(()), Err(error))
            }
            .unwrap_err();
            assert_eq!(serde_json::to_vec(&error).unwrap(), expected);
            assert_eq!(error.to_string(), "Original failure");
            assert_eq!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(13)
            );
        }
    }

    #[test]
    fn primary_and_cleanup_failures_are_both_retained() {
        use std::error::Error as _;

        for primary_unknown in [false, true] {
            for cleanup_unknown in [false, true] {
                let mut primary = Error::new(Code::Busy, "Original failure")
                    .held_by(crate::error::Holder::Agent)
                    .with("count", json!(3))
                    .with_source(std::io::Error::from_raw_os_error(13));
                primary.uncertain = primary_unknown;
                let mut cleanup = Error::new(Code::Cleanup, "Original cleanup failure")
                    .with("private", json!("cleanup detail"))
                    .with_source(std::io::Error::from_raw_os_error(5));
                cleanup.uncertain = cleanup_unknown;
                let primary_bytes = serde_json::to_vec(&primary).unwrap();
                let cleanup_bytes = serde_json::to_vec(&cleanup).unwrap();
                let error = super::combine_cleanup(Err(primary), Err(cleanup)).unwrap_err();
                let pending = primary_unknown || cleanup_unknown;
                assert_eq!(
                    json!(error),
                    json!({
                        "ok": false,
                        "code": "busy",
                        "error": format!("Original failure; cleanup: cleanup (recovery pending: {pending})"),
                        "uncertain": pending,
                        "holder": "agent",
                        "count": 3,
                    })
                );
                let pair = error
                    .source()
                    .unwrap()
                    .downcast_ref::<super::CleanupFailures>()
                    .unwrap();
                assert_eq!(serde_json::to_vec(&pair.primary).unwrap(), primary_bytes);
                assert_eq!(serde_json::to_vec(&pair.cleanup).unwrap(), cleanup_bytes);
                assert!(std::ptr::eq(
                    pair.source().unwrap().downcast_ref::<Error>().unwrap(),
                    &pair.primary
                ));
                for (original, errno) in [(&pair.primary, 13), (&pair.cleanup, 5)] {
                    assert_eq!(
                        original
                            .source()
                            .unwrap()
                            .downcast_ref::<std::io::Error>()
                            .unwrap()
                            .raw_os_error(),
                        Some(errno)
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn a_cleanup_failure_cannot_turn_unknown_work_into_success() {
        use std::error::Error as _;

        let operations = super::Operations::default();
        let outcome = operations
            .start(async { panic!("worker failed") })
            .await
            .unwrap();
        assert!(outcome.await.is_err());
        let stand = Stand::locked(&[]);
        {
            let mut engine = stand.lock().await;
            engine.held = true;
            engine.release_fails = true;
        }
        let gate = Gate::default();
        let error = super::drain_and_release(&operations, &stand, &gate)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
        let pair = error
            .source()
            .unwrap()
            .downcast_ref::<super::CleanupFailures>()
            .unwrap();
        assert_eq!(
            pair.primary.error,
            "The input worker did not report its outcome"
        );
        assert_eq!(pair.cleanup.error, "Held input could not be released");
        assert!(stand.lock().await.held);
        assert!(operations.start(async {}).await.unwrap_err().uncertain);
        stand.lock().await.release_fails = false;
        assert!(
            super::drain_and_release(&operations, &stand, &gate)
                .await
                .unwrap_err()
                .uncertain
        );
        assert!(!stand.lock().await.held);
        assert!(operations.start(async {}).await.unwrap_err().uncertain);
        super::reset(&operations, &stand, &gate, Perform::release)
            .await
            .unwrap();
        operations.start(async {}).await.unwrap().await.unwrap();
    }

    #[tokio::test]
    async fn a_gate_failure_does_not_hide_a_drain_failure() {
        use std::error::Error as _;

        let operations = super::Operations::default();
        let outcome = operations
            .start(async { panic!("worker failed") })
            .await
            .unwrap();
        assert!(outcome.await.is_err());
        let gate = Arc::new(Gate::default());
        let grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = grant.authorize().unwrap();
                panic!("poison ownership");
            }))
            .is_err()
        );
        let stand = Stand::locked(&[]);
        stand.lock().await.held = true;
        let error = super::drain_and_release(&operations, &stand, &gate)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
        let pair = error
            .source()
            .unwrap()
            .downcast_ref::<super::CleanupFailures>()
            .unwrap();
        assert_eq!(pair.primary.code, Code::Uncertain);
        assert_eq!(pair.cleanup.code, Code::Native);
        assert_eq!(pair.cleanup.error, "Input ownership failed");
        let engine = stand.lock().await;
        assert_eq!(engine.releases, 0, "unknown ownership cannot allow release");
        assert!(engine.held);
        assert!(operations.check().unwrap_err().uncertain);
        drop(engine);
        let mut requests = tokio::task::JoinSet::new();
        let error = super::finish(&gate, &mut requests, &operations, &stand)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Native, "closing failed first");
        assert!(error.uncertain);
        let pair = error
            .source()
            .unwrap()
            .downcast_ref::<super::CleanupFailures>()
            .unwrap();
        assert_eq!(pair.primary.error, "Input ownership failed");
        assert_eq!(pair.cleanup.code, Code::Uncertain);
        assert!(pair.cleanup.source().is_some());
        assert_eq!(stand.lock().await.releases, 0);
    }

    #[test]
    fn cleanup_diagnostics_do_not_reveal_private_causes() {
        use std::error::Error as _;

        let secret = "private-owner-input-cause".repeat(1024);
        let primary = Error::new(Code::Native, "Input failed")
            .with_source(std::io::Error::other(secret.clone()));
        let cleanup = Error::new(Code::Cleanup, secret.clone())
            .with("private", json!(secret))
            .with_source(std::io::Error::other(secret.clone()))
            .uncertain();
        let error = super::combine_cleanup(Err(primary), Err(cleanup)).unwrap_err();
        let pair = error
            .source()
            .unwrap()
            .downcast_ref::<super::CleanupFailures>()
            .unwrap();
        for diagnostic in [
            serde_json::to_string(&error).unwrap(),
            error.to_string(),
            format!("{error:?}"),
            pair.to_string(),
            format!("{pair:?}"),
        ] {
            assert!(!diagnostic.contains("private-owner-input-cause"));
            assert!(diagnostic.len() < 256);
        }
        assert_eq!(pair.cleanup.error, secret);
        assert_eq!(pair.cleanup.details["private"], secret);
        assert_eq!(pair.primary.source().unwrap().to_string(), secret);
        assert_eq!(pair.cleanup.source().unwrap().to_string(), secret);
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
    async fn cancelling_reset_before_drain_or_rebind_keeps_recovery_pending() {
        use std::{future::Future, task::Context, task::Poll, task::Waker};

        for before_rebind in [false, true] {
            let operations = super::Operations::default();
            let gate = Arc::new(Gate::default());
            let _grant = gate.acquire(&"a".repeat(48), super::Who::Agent).unwrap();
            let stand = Stand::locked(&[]);
            let (resume, barrier) = tokio::sync::oneshot::channel::<()>();
            let mut resume = Some(resume);
            let running = operations.start(barrier).await.unwrap();
            let engine_guard = if before_rebind {
                resume.take().unwrap().send(()).unwrap();
                running.await.unwrap().unwrap();
                operations.drain().await.unwrap();
                Some(stand.lock().await)
            } else {
                // Leave the registered operation waiting at its drain barrier.
                None
            };
            {
                let recovery = super::reset(&operations, &stand, &gate, Perform::release);
                tokio::pin!(recovery);
                let mut context = Context::from_waker(Waker::noop());
                assert!(matches!(
                    recovery.as_mut().poll(&mut context),
                    Poll::Pending
                ));
                assert!(
                    !gate.active().unwrap(),
                    "cancellation did not need the engine"
                );
                assert!(operations.check().unwrap_err().uncertain);
            }
            assert!(operations.start(async {}).await.unwrap_err().uncertain);
            if !before_rebind {
                assert_eq!(operations.tasks.lock().unwrap().len(), 1);
            }
            drop(resume);
            drop(engine_guard);
            super::reset(&operations, &stand, &gate, Perform::release)
                .await
                .unwrap();
            assert_eq!(stand.lock().await.releases, 1);
            operations.check().unwrap();
            assert!(operations.tasks.lock().unwrap().is_empty());
        }
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
        engine.clock = Some(Instant::now());
        let gate = Gate::default();
        let actions = (0..3)
            .map(|_| Action::Type {
                text: "hello".into(),
            })
            .collect();
        run(&mut *engine, &gate, actions, app().as_ref())
            .await
            .unwrap();
        assert_eq!(
            engine.budgets,
            [
                super::focus::PREPARE_BUDGET,
                super::focus::PREPARE_BUDGET - Duration::from_millis(5),
                super::focus::PREPARE_BUDGET - Duration::from_millis(10),
            ]
        );
        assert_eq!(engine.preparations, [true, true, true]);
    }

    #[tokio::test]
    async fn failed_focus_checks_spend_budget_and_untargeted_actions_spend_none() {
        let stand = Stand::locked(&[Step::Refuse]);
        let mut engine = stand.lock().await;
        engine.clock = Some(Instant::now());
        engine.check_delay = Duration::from_millis(5);
        let mut budget = Duration::from_millis(2);
        let error = super::require_within(
            &mut *engine,
            app().as_ref().unwrap(),
            Need::Keys,
            &mut budget,
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::FocusChanged);
        assert_eq!(error.details["reason"], "app");
        assert_eq!(budget, Duration::ZERO);
        assert_eq!(engine.budgets, [Duration::from_millis(2)]);

        let before = engine.clock;
        let mut untouched = super::focus::PREPARE_BUDGET;
        assert_eq!(
            super::require_within(
                &mut *engine,
                app().as_ref().unwrap(),
                Need::Nothing,
                &mut untouched,
                false,
            )
            .await
            .unwrap(),
            None
        );
        assert_eq!(untouched, super::focus::PREPARE_BUDGET);
        assert_eq!(engine.clock, before);
        assert_eq!(engine.budgets, [Duration::from_millis(2)]);
    }
}
