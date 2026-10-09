//! Peer-authenticated input service. Socket handlers can cancel ownership while
//! the serialized event engine awaits the next step of a gesture.
use super::{events::Engine, lease::Gate, session_info};
use crate::{
    actions::{self, Action},
    error::{Error, Result},
    session::{Display, InputEpoch},
};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

struct Service {
    engine: Mutex<Engine>,
    gate: Arc<Gate>,
    epoch: RwLock<InputEpoch>,
}
impl Service {
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
        let operation = request["op"].as_str().unwrap_or_default();
        if operation == "hello" {
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
                "accessibility": Engine::trusted(),
                "ready": self.check().is_ok(),
                "enforcedEpoch": self.epoch()?,
                "session": session_info()?,
                "controlActive": self.gate.active()?,
                "heldInput": held_input,
                "streaming": true,
            }));
        }
        if operation == "release" {
            self.gate.cancel(Some(owner))?;
            self.cleanup().await?;
            return Ok(json!({ "ok": true, "released": true }));
        }
        if operation == "reset" {
            let expected: InputEpoch = serde_json::from_value(request["epoch"].clone())?;
            let current = InputEpoch::from_proof(&session_info()?, std::process::id())?;
            if expected != current || !Engine::trusted() {
                return Err(Error::new(
                    "identity",
                    "Recovery requires the current hidden session and permission",
                ));
            }
            self.gate.cancel(None)?;
            let mut engine = self.engine.lock().await;
            engine.rebind(current.clone())?;
            *self
                .epoch
                .write()
                .map_err(|_| Error::new("native", "Input identity failed"))? = current;
            return Ok(json!({ "ok": true, "released": true }));
        }
        self.check()?;
        if operation == "renew" {
            self.gate.renew(owner)?;
            return Ok(json!({ "ok": true }));
        }
        let expected: InputEpoch = serde_json::from_value(request["epoch"].clone())?;
        let epoch = self.epoch()?;
        if expected != epoch {
            return Err(Error::new(
                "session-changed",
                "Input coordinates belong to a different session or display",
            ));
        }
        if operation == "acquire" {
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
        if operation != "input" {
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
        let actions: Vec<Action> = serde_json::from_value(request["actions"].clone())?;
        let mut engine = self.engine.lock().await;
        drop(grant.authorize()?);
        engine.grant = Some(grant);
        for (performed, action) in actions.into_iter().enumerate() {
            if let Err(error) = engine.perform(action).await {
                let _ = self.gate.cancel(Some(owner));
                let cleanup = engine.release();
                let mut reply = json!(error);
                reply["performed"] = json!(performed);
                reply["uncertain"] = json!(cleanup.is_err());
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

// The helper-owned directory and an exclusive lifetime flock make stale socket
// removal safe without taking over another running input service.
struct Endpoint {
    socket: PathBuf,
    inode: u64,
    _lock: File,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.socket)
            .is_ok_and(|m| m.ino() == self.inode && m.file_type().is_socket())
        {
            let _ = fs::remove_file(&self.socket);
        }
    }
}
async fn bind(socket: &Path, uid: u32) -> Result<(UnixListener, Endpoint)> {
    let parent = socket
        .parent()
        .ok_or_else(|| Error::new("config", "Socket needs a parent directory"))?;
    match fs::create_dir(parent) {
        Ok(()) => fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
        return Err(Error::new(
            "identity",
            "Input socket directory must belong to the helper and deny writes by others",
        ));
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join("service.lock"))?;
    FileExt::try_lock_exclusive(&lock)
        .map_err(|_| Error::new("busy", "An input service already owns this directory"))?;
    if let Ok(metadata) = fs::symlink_metadata(socket) {
        if metadata.uid() != uid || !metadata.file_type().is_socket() {
            return Err(Error::new(
                "identity",
                "Existing input endpoint is not a helper-owned socket",
            ));
        }
        match tokio::time::timeout(Duration::from_millis(250), UnixStream::connect(socket)).await {
            Ok(Err(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                fs::remove_file(socket)?
            }
            _ => {
                return Err(Error::new(
                    "busy",
                    "Existing input endpoint is reachable or cannot be verified stale",
                ));
            }
        }
    }
    let listener = UnixListener::bind(socket)?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o666))?;
    let endpoint = Endpoint {
        socket: socket.to_owned(),
        inode: fs::symlink_metadata(socket)?.ino(),
        _lock: lock,
    };
    Ok((listener, endpoint))
}

pub async fn serve(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<()> {
    crate::session::validate_target_uid(uid, controller_uid)?;
    if controller_uid == 0 {
        return Err(Error::new(
            "identity",
            "The input service must run in a distinct helper account",
        ));
    }
    let mut engine = Engine::new(uid)?;
    let (listener, _endpoint) = bind(&socket, uid).await?;
    engine.recover(socket.with_extension("held.json"))?;
    let service = Arc::new(Service {
        epoch: RwLock::new(engine.epoch.clone()),
        engine: Mutex::new(engine),
        gate: Arc::new(Gate::default()),
    });
    let mut timer = tokio::time::interval(Duration::from_millis(50));
    let slots = Arc::new(Semaphore::new(16));
    let mut tasks = JoinSet::new();
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = terminate.recv() => break,
            _ = timer.tick() => {
                if service.check().is_err() {
                    let _ = service.gate.cancel(None);
                }
                if !service.gate.active()?
                    && let Ok(mut engine) = service.engine.try_lock()
                {
                    let _ = engine.release();
                }
            }
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let peer = stream.peer_cred()?;
                if peer.uid() != controller_uid && peer.uid() != uid {
                    continue;
                }
                let Ok(slot) = slots.clone().try_acquire_owned() else {
                    continue;
                };
                let service = service.clone();
                tasks.spawn(async move {
                    let _slot = slot;
                    let _ = handle(stream, service).await;
                });
            }
        }
    }
    service.gate.close()?;
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    service.cleanup().await?;
    Ok(())
}
async fn handle(mut stream: UnixStream, service: Arc<Service>) -> Result<()> {
    let request = tokio::time::timeout(Duration::from_secs(3), async {
        let mut body = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                return Err(Error::new("protocol", "Incomplete request"));
            }
            body.extend_from_slice(&buffer[..n]);
            if body.len() > 32768 {
                return Err(Error::new("protocol", "Native request exceeds 32 KiB"));
            }
            if let Some(end) = body.iter().position(|b| *b == b'\n') {
                return Ok(serde_json::from_slice::<Value>(&body[..end])?);
            }
        }
    })
    .await
    .map_err(|_| Error::new("transport", "Native request timed out"))??;
    let reply = if request["op"] == "input" {
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
    let mut bytes = serde_json::to_vec(&reply.unwrap_or_else(|e| json!(e)))?;
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(1), stream.write_all(&bytes))
        .await
        .map_err(|_| Error::new("transport", "Native reply timed out"))??;
    Ok(())
}
