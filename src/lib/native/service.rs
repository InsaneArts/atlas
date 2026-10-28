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
    error::{Error, Holder, Result},
    session::{Display, InputEpoch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
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
/// target app, with the Dock covering where it takes clicks.
const CAPABILITIES: [&str; 4] = [
    crate::session::SHUTDOWN,
    crate::session::HOLD,
    crate::session::FOCUS_APP,
    crate::session::FOCUS_DOCK,
];

struct Service {
    engine: Mutex<Engine>,
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

    let Some((file, metadata)) = crate::digest::open_regular(&path)
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
            .map_err(|_| Error::new("native", "Input identity failed"))
    }

    fn check(&self) -> Result<()> {
        if InputEpoch::from_proof(&session_info()?, std::process::id())? != self.epoch()?
            || !Engine::trusted()
        {
            return Err(Error::new(
                "identity",
                "The input session or permission changed; release control",
            ));
        }

        Ok(())
    }

    async fn cleanup(&self) -> Result<()> {
        let mut engine = self.engine.lock().await;

        if !self.gate.active()? {
            engine.release()?;
        }

        Ok(())
    }

    async fn dispatch(&self, request: &Value) -> Result<Value> {
        let owner = request["owner"].as_str().unwrap_or_default();
        let operation = Operation::of(request);
        if operation == Operation::Hello {
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
                    "identity",
                    "Shutdown names a different input service process",
                ));
            }

            if self.gate.active()? {
                return Err(Error::new(
                    "busy",
                    "The input service has an active controller",
                ));
            }

            // Release held input first; only an acknowledged release closes the
            // gate for good. Holding the engine lock keeps acquire out meanwhile,
            // and one that took the lock first is seen once it is held.
            let mut engine = self.engine.lock().await;
            if self.gate.active()? {
                return Err(Error::new(
                    "busy",
                    "The input service has an active controller",
                ));
            }

            if let Err(error) = engine.release() {
                return Err(Error::new(
                    "uncertain",
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
                    "native",
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
                    "identity",
                    "Recovery requires the current hidden session and permission",
                ));
            }

            self.gate.cancel(None)?;
            let mut engine = self.engine.lock().await;
            let rebound = engine.rebind(current);

            // Enforce whatever epoch the engine now holds, even when its
            // release failed: the two never disagree, and the timer keeps
            // retrying that release against the display it was bound to.
            *self
                .epoch
                .write()
                .map_err(|_| Error::new("native", "Input identity failed"))? = engine.epoch.clone();

            rebound?;
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
                "session-changed",
                "Input coordinates belong to a different session or display",
            ));
        }

        if operation == Operation::Acquire {
            let mut engine = self.engine.lock().await;

            if let Some(holder) = self.gate.holder()? {
                // An agent finding the person in control is told to wait.
                let agent = request["person"] != true;
                let error = Error::new(
                    "busy",
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
            return Err(Error::new("input", "Unknown native operation"));
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

        let mut engine = self.engine.lock().await;

        drop(grant.authorize()?);
        engine.grant = Some(grant);

        if let Err(reply) = run(&mut *engine, &self.gate, actions, expect).await {
            return Ok(reply);
        }

        // The app the input landed in, for an agent's activity log (only
        // when asked, so a person's own control pays nothing): the owner of
        // the frontmost ordinary window, read once the engine is free.
        drop(engine);
        let app = if request["app"] == true {
            tokio::task::spawn_blocking(frontmost_app)
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        let mut reply = json!({
            "ok": true,
            "performed": request["actions"].as_array().map(Vec::len),
            "app": app,
        });
        // Echoed, so the controller knows each action was checked.
        if let Some(expect) = expect {
            reply["expect"] = json!(expect);
        }

        Ok(reply)
    }
}

/// What the input loop needs of the event engine; tests stand in for it.
trait Perform {
    /// Refuse unless `expect` holds for `need` now, before anything is
    /// posted, taking at most `budget` to tell.
    async fn require(&mut self, expect: Expect, need: Need, budget: Duration) -> Result<()>;
    /// Carry out one action, posting its events.
    async fn perform(&mut self, action: Action) -> Result<()>;
    /// Events posted so far.
    fn posted(&self) -> u64;
    /// Let go of every key and button still held.
    fn release(&mut self) -> Result<()>;
}

impl Perform for Engine {
    /// The reads block, so they run on a blocking thread; the engine stays
    /// locked by the caller meanwhile, so nothing posts in between.
    async fn require(&mut self, expect: Expect, need: Need, budget: Duration) -> Result<()> {
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let deadline = Instant::now() + budget;
        tokio::task::spawn_blocking(move || focus::check(expect, need, &display, deadline))
            .await
            .unwrap_or_else(|_| Err(focus::unreadable()))
    }

    async fn perform(&mut self, action: Action) -> Result<()> {
        Engine::perform(self, action).await
    }

    fn posted(&self) -> u64 {
        self.posted
    }

    fn release(&mut self) -> Result<()> {
        Engine::release(self)
    }
}

/// Carry out `actions` in order on `engine`, which the caller holds
/// locked, each checked against `expect` just before it starts. On the
/// first failure, revoke `gate`, release what is held and return the
/// failure reply: what was performed, whether the failing action had posted
/// anything, and whether the outcome is unknown.
async fn run(
    engine: &mut impl Perform,
    gate: &Gate,
    actions: Vec<Action>,
    expect: Option<Expect>,
) -> std::result::Result<(), Value> {
    let mut budget = focus::CHECK_BUDGET;

    for (performed, action) in actions.into_iter().enumerate() {
        let before = engine.posted();
        let checked = match expect {
            Some(expect) => require_within(engine, expect, Need::of(&action), &mut budget).await,
            None => Ok(()),
        };
        let step = match checked {
            Ok(()) => engine.perform(action).await,
            Err(error) => Err(error),
        };

        if let Err(error) = step {
            let _ = gate.revoke();
            return Err(failed(engine, error, performed, before));
        }
    }

    Ok(())
}

/// [`Perform::require`], spending what it takes of `budget`: the time one
/// request may spend checking in all.
async fn require_within(
    engine: &mut impl Perform,
    expect: Expect,
    need: Need,
    budget: &mut Duration,
) -> Result<()> {
    let started = Instant::now();
    let outcome = engine.require(expect, need, *budget).await;
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
    let unknown = partial && error.code != "cancelled";
    reply["uncertain"] = json!(cleanup.is_err() || unknown || error.uncertain);

    if let Err(error) = cleanup {
        reply["cleanupError"] = json!(error.error);
    }

    reply
}

/// The name of the app owning the frontmost ordinary window, if any.
fn frontmost_app() -> Option<String> {
    super::windows::windows()
        .ok()?
        .into_iter()
        .find(|window| window.layer == 0)
        .and_then(|window| window.owner)
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
            "identity",
            "The input service must run in a distinct helper account",
        ));
    }

    let mut engine = Engine::new(uid)?;
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
        engine: Mutex::new(engine),
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
                        "busy",
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

    let closed = service.gate.close();
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let cleanup = service.cleanup().await;

    outcome.and(closed).and(cleanup)
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
        .map_err(|_| Error::new("transport", "Native reply timed out"))??;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Action, CAPABILITIES, Duration, Error, Expect, Gate, MAX_REQUEST, Need, Perform, Result,
        admitted, read_request, run,
    };
    use serde_json::{Value, json};
    use std::sync::{Arc, Weak};
    use tokio::{io::AsyncWriteExt, net::UnixStream, sync::Mutex};

    /// What a stand-in engine does at one action.
    #[derive(Clone, Copy, PartialEq)]
    enum Step {
        Post,
        Refuse,
        /// Post one event, then fail.
        Break,
    }

    /// An engine that logs each check and post, and does at each action
    /// what its script says. It holds a handle to the lock it lives in, to
    /// see that the lock is held whenever it is asked anything.
    struct Stand {
        script: Vec<Step>,
        log: Vec<String>,
        posted: u64,
        releases: usize,
        lock: Weak<Mutex<Stand>>,
    }

    impl Stand {
        fn locked(script: &[Step]) -> Arc<Mutex<Self>> {
            Arc::new_cyclic(|lock| {
                Mutex::new(Self {
                    script: script.to_vec(),
                    log: Vec::new(),
                    posted: 0,
                    releases: 0,
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
        async fn require(&mut self, _: Expect, _: Need, _: Duration) -> Result<()> {
            self.assert_locked();
            let index = self.logged("check");
            self.log.push(format!("check {index}"));
            if self.script.get(index) == Some(&Step::Refuse) {
                return Err(Error::new("focus-changed", "Calculator has keyboard focus")
                    .with("reason", "app")
                    .with("found", json!({"app": "Calculator", "pid": 812})));
            }
            Ok(())
        }

        async fn perform(&mut self, _: Action) -> Result<()> {
            self.assert_locked();
            let index = self.logged("post");
            self.log.push(format!("post {index}"));
            self.posted += 1;
            match self.script.get(index) {
                Some(Step::Break) => Err(Error::new("identity", "The session changed")),
                _ => Ok(()),
            }
        }

        fn posted(&self) -> u64 {
            self.posted
        }

        fn release(&mut self) -> Result<()> {
            self.releases += 1;
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
    ) -> (std::result::Result<(), Value>, Arc<Mutex<Stand>>) {
        let stand = Stand::locked(script);
        let gate = Arc::new(Gate::default());
        let outcome = {
            let mut engine = stand.lock().await;
            run(&mut *engine, &gate, keys(count), expect).await
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
    async fn an_action_that_fails_partway_stays_uncertain() {
        let (outcome, _) = ran(&[Step::Break], 2, app()).await;
        let reply = outcome.unwrap_err();
        assert_eq!(reply["code"], "identity");
        assert_eq!(reply["performed"], 0);
        assert_eq!(reply["partial"], true);
        assert_eq!(reply["uncertain"], true);
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
}
