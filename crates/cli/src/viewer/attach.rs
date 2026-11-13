//! How the app reaches the service: over a private socket in the person's
//! Atlas folder, never through a file. Each side checks the other's running
//! code first; only then does the service give the app, on that connection
//! alone, the viewer's address, the secret it names itself by and this
//! service's incarnation, then the lines it passes on ("viewer"). The app is
//! its own process: it outlives a service that ends and attaches to the next
//! one, and the service goes on when the app quits.
use super::{Viewer, unpoisoned};
use crate::menubar::{Line, Program};
use atlas_core::error::{Code, Error, Result};
use std::{
    fs,
    os::fd::AsFd,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering::SeqCst},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{Mutex, mpsc::UnboundedReceiver},
};

/// Where the socket is, in a home.
const SOCKET: &str = "Library/Application Support/Atlas/attach.sock";

/// The listening socket, removed with this service when still its own.
pub(super) struct Attach {
    listener: UnixListener,
    path: PathBuf,
    place: (u64, u64),
}

impl Attach {
    /// Listen on the account's attach socket. One left by a service that
    /// ended is replaced; one a service still answers on refuses this start.
    ///
    /// # Errors
    ///
    /// Returns the failure to make the folder or bind, and a refusal while
    /// another service listens.
    pub(super) async fn bind() -> Result<Self> {
        Self::bind_at(&atlas_core::config::home()?.join(SOCKET)).await
    }

    async fn bind_at(path: &Path) -> Result<Self> {
        if let Some(folder) = path.parent() {
            atlas_core::files::private_folder(folder, Code::Menubar)?;
        }
        if fs::symlink_metadata(path).is_ok() {
            if UnixStream::connect(path).await.is_ok() {
                return Err(Error::new(
                    Code::Menubar,
                    "Atlas already runs in the background",
                ));
            }
            fs::remove_file(path).map_err(|error| Error::writing(path, error))?;
        }
        let listener = UnixListener::bind(path).map_err(|error| Error::writing(path, error))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| Error::writing(path, error))?;
        let place = identity(path)
            .ok_or_else(|| Error::new(Code::Menubar, "The attach socket went away"))?;
        Ok(Self {
            listener,
            path: path.to_owned(),
            place,
        })
    }

    /// Serve attachments until dropped: one app at a time, `app` the
    /// executable it must run, each line of `lines` passed to the app
    /// attached then, and kept for the next while none is.
    pub(super) async fn serve(
        self,
        viewer: Arc<Viewer>,
        app: PathBuf,
        lines: UnboundedReceiver<Line>,
    ) {
        let lines = Arc::new(Mutex::new(lines));
        loop {
            let Ok((stream, _)) = self.listener.accept().await else {
                continue;
            };
            let Some((stream, pid)) = admitted(stream, &app).await else {
                continue;
            };
            // One app at a time: another is turned away while one is.
            let Ok(mut held) = lines.clone().try_lock_owned() else {
                continue;
            };
            let viewer = viewer.clone();
            tokio::spawn(async move {
                let untaken = attached(&viewer, stream, pid, &mut held).await;
                // Detached, still holding the lines, so the next app's
                // attachment never meets this one's cleanup: no process is
                // shown the screen as the app's any more.
                *unpoisoned(&viewer.app_process) = None;
                viewer.attaching.attached.store(false, SeqCst);
                drop(held);
                // A line the app went away before taking up is kept for the
                // next one; while the window waits, whether or not its line
                // was given, an app is started for it.
                if let Some(line) = untaken {
                    let _ = viewer.send(line);
                }
                if viewer.attaching.wanted.load(SeqCst) {
                    let _ = viewer.attaching.ensure();
                }
            });
        }
    }
}

impl Drop for Attach {
    fn drop(&mut self) {
        if identity(&self.path) == Some(self.place) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// The app that attaches to this service: how to start it, and where it is.
#[derive(Default)]
pub(super) struct Attaching {
    /// The shipped app and how to start it; none for an app this service
    /// runs as its child.
    program: std::sync::Mutex<Option<Program>>,
    /// Whether an app is attached now.
    attached: AtomicBool,
    /// Whether an app this service started is still on its way: started,
    /// neither attached nor ended yet.
    starting: AtomicBool,
    /// Whether the person asked for the window and no app took it up yet.
    wanted: AtomicBool,
    /// The apps started in a row that ended without attaching.
    failed_starts: AtomicU8,
}

/// How many apps in a row may end without attaching before no more are
/// started for a window: one that cannot start is not started forever.
const STARTS: u8 = 3;

impl Attaching {
    /// Start `program` when an app is asked for.
    pub(super) fn program(&self, program: Program) {
        *unpoisoned(&self.program) = Some(program);
    }

    /// The person asked for the window: kept until an app takes it up.
    pub(super) fn want_window(&self) {
        self.wanted.store(true, SeqCst);
        self.failed_starts.store(0, SeqCst);
    }

    /// Start the app that attaches to this service when none is attached and
    /// none this service started is still on its way; nothing for an app this
    /// service runs as its child. One that ends without attaching while the
    /// window waits is started again, a few times.
    ///
    /// # Errors
    ///
    /// Returns the failure to start it.
    pub(super) fn ensure(self: &Arc<Self>) -> Result<()> {
        if self.attached.load(SeqCst) {
            return Ok(());
        }
        let program = unpoisoned(&self.program);
        let Some(program) = program.as_ref() else {
            return Ok(());
        };
        if self.starting.swap(true, SeqCst) {
            return Ok(());
        }
        let attaching = self.clone();
        program
            .start_attaching(move || attaching.ended())
            .inspect_err(|_| self.starting.store(false, SeqCst))
    }

    /// The app this service started ended.
    fn ended(self: &Arc<Self>) {
        self.starting.store(false, SeqCst);
        if self.attached.load(SeqCst) || !self.wanted.load(SeqCst) {
            return;
        }
        if self.failed_starts.fetch_add(1, SeqCst) + 1 < STARTS {
            let _ = self.ensure();
        }
    }

    /// An app attached.
    fn attach(&self) {
        self.attached.store(true, SeqCst);
        self.starting.store(false, SeqCst);
        self.failed_starts.store(0, SeqCst);
    }
}

/// The process at the other end of `stream`, when it is this account's and,
/// as the kernel names it now, runs `app` exactly as it is on disk, its
/// running code signed as Atlas.
///
/// The kernel names the socket's last user, so a process of this account
/// that connects, forks and then runs the app itself in place of its own
/// code passes while its fork keeps the socket. That takes running code of
/// one's own as the person, which an agent using Atlas's commands does not
/// have; a check made on every message would close it.
async fn admitted(stream: UnixStream, app: &Path) -> Option<(UnixStream, u32)> {
    let credentials = stream.peer_cred().ok()?;
    if credentials.uid() != nix::unistd::getuid().as_raw() {
        return None;
    }
    let stream = stream.into_std().ok()?;
    let app = app.to_owned();
    let (stream, pid) = tokio::task::spawn_blocking(move || {
        let requirement = atlas_core::app_bundle::signature::requirement_for(
            atlas_core::app_bundle::signature::APP_IDENTIFIER,
        );
        let pid = atlas_native::peer::verified(stream.as_fd(), &app, &requirement);
        (stream, pid)
    })
    .await
    .ok()?;
    Some((UnixStream::from_std(stream).ok()?, pid?))
}

/// Give the attached app at `pid` what it needs, then pass it each line,
/// one at a time, until either side closes: the line it went away before
/// taking up, if any. The app acknowledges each line it took up with a
/// byte; it says nothing else.
async fn attached(
    viewer: &Viewer,
    mut stream: UnixStream,
    pid: u32,
    lines: &mut UnboundedReceiver<Line>,
) -> Option<Line> {
    let process = i32::try_from(pid).ok().and_then(|pid| {
        Some(atlas_core::process::Instance::new(
            pid,
            atlas_core::process::start(pid).ok()?,
        ))
    });
    *unpoisoned(&viewer.app_process) = process;
    viewer.attaching.attach();
    let settings = viewer
        .settings
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let first = format!(
        "{}\nsecret {}\nincarnation {}\nprotocol {}\nsettings {settings}\n",
        viewer.url,
        viewer.app_secret,
        viewer.incarnation,
        super::PROTOCOL
    );
    if stream.write_all(first.as_bytes()).await.is_err() {
        return None;
    }
    let (mut reading, mut writing) = stream.into_split();
    let mut pending = None;
    let mut taken = [0_u8; 1];
    loop {
        tokio::select! {
            line = lines.recv(), if pending.is_none() => {
                let line = line?;
                if writing.write_all(line.text().as_bytes()).await.is_err() {
                    return Some(line);
                }
                pending = Some(line);
            }
            read = reading.read(&mut taken) => match read {
                Ok(1..) => {
                    if pending.take() == Some(Line::Viewer) {
                        viewer.attaching.wanted.store(false, SeqCst);
                    }
                }
                // It let go, before taking up the line it was given.
                Ok(0) | Err(_) => return pending,
            },
        }
    }
}

/// The device and inode at `path`, when there is something there.
fn identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

#[cfg(test)]
#[path = "attach_tests.rs"]
mod tests;
