//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
use super::{events::Engine, lease::Gate, session_info};
use crate::{
    actions::{self, Action},
    error::{Error, Result},
    session::{Display, InputEpoch},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    signal::unix::{SignalKind, signal},
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

// Twice the controller's 32 KiB action limit, leaving room for the owner,
// epoch and JSON re-encoding around any batch the controller accepts.
const MAX_REQUEST: usize = 64 * 1024;

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
}

/// The operations the input service answers, by their `op` string.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Hello,
    Shutdown,
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
                "capabilities": ["shutdown"],
                "accessibility": Engine::trusted(),
                "ready": self.check().is_ok(),
                "enforcedEpoch": self.epoch()?,
                "session": session_info()?,
                "controlActive": self.gate.active()?,
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

            if self.gate.active()? {
                return Err(Error::new(
                    "busy",
                    "The native input service already has a controller",
                ));
            }

            engine.check()?;
            engine.release()?;
            engine.grant = Some(self.gate.acquire(owner)?);

            return Ok(json!({ "ok": true }));
        }

        if operation != Operation::Input {
            return Err(Error::new("input", "Unknown native operation"));
        }

        let grant = self.gate.grant(owner)?;
        actions::validate(
            &request["actions"],
            &Display {
                width: epoch.width,
                height: epoch.height,
                scale: 1.0,
            },
        )?;

        let actions = Vec::<Action>::deserialize(&request["actions"])?;
        let mut engine = self.engine.lock().await;

        drop(grant.authorize()?);
        engine.grant = Some(grant);

        for (performed, action) in actions.into_iter().enumerate() {
            let before = engine.posted;

            if let Err(error) = engine.perform(action).await {
                let _ = self.gate.revoke();

                // An action that posted some of its events before failing
                // (a few typed characters, one click of two) had part of
                // its effect, and nobody can say which part.
                let partial = engine.posted > before;
                let cleanup = engine.release();

                let mut reply = json!(error);
                reply["performed"] = json!(performed);
                reply["partial"] = json!(partial);

                // A cancellation is the person's doing and tells them what
                // landed; any other break mid-action leaves that unknown.
                let unknown = partial && error.code != "cancelled";
                reply["uncertain"] = json!(cleanup.is_err() || unknown || error.uncertain);

                if let Err(error) = cleanup {
                    reply["cleanupError"] = json!(error.error);
                }

                return Ok(reply);
            }
        }

        Ok(json!({
            "ok": true,
            "performed": request["actions"].as_array().map(Vec::len),
        }))
    }
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

    let service = Arc::new(Service {
        build,
        executable,
        stopping: tokio::sync::Notify::new(),
        epoch: RwLock::new(engine.epoch.clone()),
        engine: Mutex::new(engine),
        gate: Arc::new(Gate::default()),
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

                let Ok(peer) = stream.peer_cred() else {
                    continue;
                };
                if peer.uid() != controller_uid && peer.uid() != uid {
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
    use super::{MAX_REQUEST, read_request};
    use tokio::io::AsyncWriteExt;

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
