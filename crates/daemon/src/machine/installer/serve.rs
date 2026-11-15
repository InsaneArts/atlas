//! The installer's socket: one request and one reply per connection, from
//! the genuine machine daemon alone, as the connection's own audit token
//! names it and launchd names its job.
use super::{Layout, Real, transaction::Installer};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    stage::Snapshot,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    os::{fd::AsFd, unix::fs::PermissionsExt},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// The longest request: a snapshot's manifest, and a little more.
const LONGEST: u64 = atlas_core::stage::MOST_MANIFEST as u64 + 64 * 1024;

/// How often a placed update's deadline is looked at.
const LOOKING: Duration = Duration::from_secs(30);

/// How long one connection has to say its request.
const ASKING: Duration = Duration::from_secs(60);

/// What a request asks.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Install {
        request: Token,
        transaction: Token,
        source: PathBuf,
        manifest: Snapshot,
    },
    Release {
        request: Token,
        transaction: Token,
    },
    Rollback {
        request: Token,
        transaction: Token,
    },
    Outcome {
        request: Token,
    },
    Transaction {
        transaction: Token,
    },
}

/// Serve as the guest's root installer until it fails.
///
/// # Errors
///
/// Refuses unless it runs as root; fails when its folders or socket cannot
/// be had.
pub fn serve_blocking() -> Result<()> {
    if !nix::unistd::geteuid().is_root() {
        return Err(Error::new(
            Code::Identity,
            "The installer runs as root alone.",
        ));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(serve(Layout::guest()))
}

async fn serve(layout: Layout) -> Result<()> {
    let installer = Arc::new(Installer::new(layout.clone(), Arc::new(Real))?);
    // Before anything is answered: what a crash left open is settled.
    let recovering = installer.clone();
    tokio::task::spawn_blocking(move || recovering.recover())
        .await
        .map_err(|_| Error::new(Code::State, "Recovery could not run."))??;
    let path = layout.socket();
    if std::fs::symlink_metadata(&path).is_ok_and(|seen| seen.file_type().is_socket_like()) {
        std::fs::remove_file(&path)?;
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    // Anyone may connect; who is answered is decided per connection.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))?;
    let ticking = installer.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(LOOKING);
        loop {
            every.tick().await;
            let looking = ticking.clone();
            let _ = tokio::task::spawn_blocking(move || looking.deadline()).await;
        }
    });
    loop {
        let (stream, _) = listener.accept().await?;
        let installer = installer.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(ASKING, answer(stream, installer)).await;
        });
    }
}

trait SocketLike {
    fn is_socket_like(&self) -> bool;
}

impl SocketLike for std::fs::FileType {
    fn is_socket_like(&self) -> bool {
        std::os::unix::fs::FileTypeExt::is_socket(self)
    }
}

/// Answer the one request on `stream`, when its peer is the genuine
/// machine daemon.
async fn answer(stream: tokio::net::UnixStream, installer: Arc<Installer>) -> Result<()> {
    let std_stream = stream.into_std()?;
    let (std_stream, peer) = tokio::task::spawn_blocking(move || {
        let requirement =
            atlas_core::app_bundle::signature::requirement_for(atlas_core::app_bundle::IDENTIFIER);
        let peer = atlas_native::peer::running(std_stream.as_fd(), &requirement);
        (std_stream, peer)
    })
    .await
    .map_err(|_| Error::new(Code::State, "The peer could not be looked at."))?;
    let Some(peer) = peer else {
        return Ok(());
    };
    // The process launchd names is the very one that connected: its PID
    // version the token's, before launchd is asked and after.
    let same = || {
        i32::try_from(peer.pid)
            .ok()
            .and_then(|pid| atlas_native::native::process_token::Token::of(pid).ok())
            .is_some_and(|now| now.version() == peer.pid_version)
    };
    if !same() {
        return Ok(());
    }
    crate::machine::launch::prove_job(peer.uid, peer.pid).await?;
    if !same() {
        return Ok(());
    }
    let stream = tokio::net::UnixStream::from_std(std_stream)?;
    let (reading, mut writing) = stream.into_split();
    let mut line = Vec::new();
    BufReader::new(reading.take(LONGEST))
        .read_until(b'\n', &mut line)
        .await?;
    let reply = match serde_json::from_slice::<Request>(&line) {
        Ok(request) => {
            let cdhash = peer.cdhash.clone();
            let uid = peer.uid;
            tokio::task::spawn_blocking(move || dispatch(&installer, request, uid, &cdhash))
                .await
                .unwrap_or_else(|_| json!({"ok": false, "error": "The request could not be done."}))
        }
        Err(_) => json!({"ok": false, "error": "That is not a request the installer serves."}),
    };
    let mut bytes = serde_json::to_vec(&reply)?;
    bytes.push(b'\n');
    writing.write_all(&bytes).await?;
    Ok(())
}

/// Do `request` for the machine job of account `uid`, running code `cdhash`.
fn dispatch(installer: &Installer, request: Request, uid: u32, cdhash: &[u8]) -> Value {
    let done = match request {
        Request::Install {
            request,
            transaction,
            source,
            manifest,
        } => installer.install(&request, &transaction, uid, &source, &manifest),
        Request::Release {
            request,
            transaction,
        } => installer.release(&request, &transaction, cdhash),
        Request::Rollback {
            request,
            transaction,
        } => installer.rollback(&request, &transaction),
        Request::Outcome { request } => installer.outcome(&request),
        Request::Transaction { transaction } => installer.transaction(&transaction),
    };
    done.unwrap_or_else(|error| json!(error))
}
