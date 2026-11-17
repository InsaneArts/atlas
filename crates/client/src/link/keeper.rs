//! A VM desktop's session keeper, as an Atlas command reaches it on its
//! socket in the keepers' private folder ([`crate::vm::keeper::sockets`]).
//! The keeper is checked before any reply of it is trusted: the folder is a
//! private one of this account's, the socket this account's, and the process that created and
//! listens on it (which `LOCAL_PEERTOKEN` names on the connecting end) runs
//! code signed as Atlas, in this account, as the very process launchd runs
//! for the VM's keeper.
use super::{Link, Unreachable};
use crate::station::Station;
use atlas_core::{
    app_bundle::signature::{APP_IDENTIFIER, requirement_for},
    config::desktops::Selected,
    desktop::DesktopId,
    error::{Code, Error, Result},
    launchd::Job,
    single_flight::Shared,
};
use atlas_session::pairing::{Enrolment, StationRecords};
use serde_json::{Value, json};
use std::{
    fmt,
    io::{BufRead, BufReader, Write},
    os::{
        fd::AsFd,
        unix::{
            fs::{FileTypeExt, MetadataExt},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// How long the keeper has to answer its hello when a client is made.
const HELLO: Duration = Duration::from_secs(2);
/// The longest line a keeper's answer holds.
const LINE: u64 = 4 << 20;

/// The keeper of a VM, reached on its socket.
#[derive(Clone)]
pub struct Keeper {
    pub selected: Arc<Selected>,
    vm: DesktopId,
    socket: PathBuf,
    pub timeout: Duration,
    trust: Arc<dyn Trust>,
    /// Where the VM's pairing is kept, when a test says; else its station
    /// folder's.
    pairing: Option<Arc<dyn StationRecords>>,
}

impl fmt::Debug for Keeper {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Keeper")
            .field("vm", &self.vm)
            .field("socket", &self.socket)
            .finish_non_exhaustive()
    }
}

/// How a client judges the keeper at the other end, given apart so tests
/// stand in.
pub(crate) trait Trust: Send + Sync {
    /// Refuse, with why, a keeper not trusted: its folder and socket at
    /// `socket`, and the listener on `stream`.
    fn trusts(&self, vm: &DesktopId, socket: &Path, stream: &UnixStream) -> Result<()>;
}

/// The keeper as launchd runs it for this account.
pub(crate) struct Launchd;

impl Trust for Launchd {
    fn trusts(&self, vm: &DesktopId, socket: &Path, stream: &UnixStream) -> Result<()> {
        let uid = nix::unistd::getuid().as_raw();
        let folder = socket.parent().map(Seen::at).unwrap_or_default();
        let listener =
            atlas_native::peer::running(stream.as_fd(), &requirement_for(APP_IDENTIFIER))
                .map(|peer| (peer.pid, peer.uid));
        let label = crate::vm::keeper::label(vm);
        let launchd = match atlas_core::launchd::job(&format!("gui/{uid}"), &label)? {
            Job::Loaded(loaded) => loaded.pid,
            Job::Absent => None,
        };
        judge_keeper(&folder, &Seen::at(socket), listener, launchd, uid).map_err(|why| {
            Error::new(
                Code::Identity,
                format!("The VM's session keeper was not trusted: {why}."),
            )
        })
    }
}

/// What a path's own entry is, never followed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) there: bool,
    pub(crate) folder: bool,
    pub(crate) socket: bool,
    pub(crate) uid: u32,
    pub(crate) mode: u32,
}

impl Seen {
    fn at(path: &Path) -> Self {
        std::fs::symlink_metadata(path).map_or_else(
            |_| Self::default(),
            |metadata| {
                let kind = metadata.file_type();
                Self {
                    there: true,
                    folder: kind.is_dir(),
                    socket: kind.is_socket(),
                    uid: metadata.uid(),
                    mode: metadata.mode(),
                }
            },
        )
    }
}

/// Whether the keeper whose station folder is `folder`, socket `socket`
/// and listener `listener` (its PID and account, when its running code is
/// signed as Atlas) is the one launchd runs as `launchd` for the account
/// `uid`.
pub(crate) fn judge_keeper(
    folder: &Seen,
    socket: &Seen,
    listener: Option<(u32, u32)>,
    launchd: Option<u32>,
    uid: u32,
) -> std::result::Result<(), &'static str> {
    if !folder.folder || folder.uid != uid || folder.mode & 0o077 != 0 {
        return Err("its folder is not a private folder of this account's");
    }
    if !socket.socket || socket.uid != uid {
        return Err("its socket is not one of this account's");
    }
    let (pid, account) = listener.ok_or("it is not signed as Atlas")?;
    if account != uid {
        return Err("it runs as another account");
    }
    if launchd != Some(pid) {
        return Err("it is not the keeper launchd runs");
    }
    Ok(())
}

impl Keeper {
    /// The keeper of `vm`, at `socket`, judged by `trust`.
    pub(crate) fn at(
        selected: Arc<Selected>,
        vm: DesktopId,
        socket: PathBuf,
        timeout: Duration,
        trust: Arc<dyn Trust>,
    ) -> Self {
        Self {
            selected,
            vm,
            socket,
            timeout,
            trust,
            pairing: None,
        }
    }

    /// This keeper, its VM's pairing kept in `records`.
    #[cfg(test)]
    pub(crate) fn with_pairing(mut self, records: Arc<dyn StationRecords>) -> Self {
        self.pairing = Some(records);
        self
    }

    /// What the VM's pairing fixed, read afresh from where it is kept. It
    /// waits on the disk, so it runs off any runtime.
    ///
    /// # Errors
    ///
    /// Refuses a VM not paired, or whose pairing was revoked; fails when
    /// its record cannot be read.
    pub(crate) fn paired(&self) -> Result<atlas_core::session::paired::Paired> {
        let kept = match &self.pairing {
            Some(records) => records.read()?,
            None => atlas_session::pairing_file::PairingFile::at(Station::own()?.path_of(&self.vm))
                .read()?,
        };
        match kept {
            Some(Enrolment::Paired(paired)) => Ok(paired),
            _ => Err(Error::new(
                Code::Identity,
                format!("{} is not paired.", self.selected.name),
            )),
        }
    }

    /// Ask the keeper `request` and read every line of its answer, within
    /// `deadline`. It waits on the socket and launchd, so it runs off any
    /// runtime.
    ///
    /// # Errors
    ///
    /// Fails when the keeper cannot be reached, is not trusted, or does not
    /// answer whole in time.
    pub(crate) fn exchange(&self, request: &Value, deadline: Duration) -> Result<Vec<Value>> {
        let until = Instant::now() + deadline;
        let unreached = |error: std::io::Error| {
            Error::new(
                Code::Transport,
                format!("{}'s session keeper did not answer.", self.selected.name),
            )
            .with_source(error)
        };
        let mut stream = UnixStream::connect(&self.socket).map_err(unreached)?;
        self.trust.trusts(&self.vm, &self.socket, &stream)?;
        // Set once, before anything is said: a socket whose keeper closed
        // takes no more options.
        stream
            .set_write_timeout(Some(deadline))
            .and_then(|()| stream.set_read_timeout(Some(deadline)))
            .map_err(unreached)?;
        let mut line = serde_json::to_vec(request)?;
        line.push(b'\n');
        stream.write_all(&line).map_err(unreached)?;
        let mut answer = Vec::new();
        let mut reader = BufReader::new(std::io::Read::take(&stream, LINE));
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(unreached(std::io::ErrorKind::TimedOut.into()));
            }
            let mut said = String::new();
            if reader.read_line(&mut said).map_err(unreached)? == 0 {
                return Ok(answer);
            }
            answer.push(serde_json::from_str(&said).map_err(|_| {
                Error::new(
                    Code::Protocol,
                    format!(
                        "{}'s session keeper said something not understood.",
                        self.selected.name
                    ),
                )
            })?);
        }
    }

    /// Ask the keeper `request`, within this link's deadline: every line of
    /// its answer.
    ///
    /// # Errors
    ///
    /// As [`Keeper::exchange`].
    pub async fn ask(&self, request: &Value) -> Result<Vec<Value>> {
        let (keeper, request) = (self.clone(), request.clone());
        tokio::task::spawn_blocking(move || keeper.exchange(&request, keeper.timeout))
            .await
            .map_err(|_| Error::new(Code::Transport, "Asking the session keeper stopped."))?
    }

    /// The keeper's one-line answer to `request`.
    ///
    /// # Errors
    ///
    /// As [`Keeper::ask`], and when it answers with no line.
    pub async fn one(&self, request: &Value) -> Result<Value> {
        self.ask(request).await?.pop().ok_or_else(|| {
            Error::new(
                Code::Transport,
                format!(
                    "{}'s session keeper closed without answering.",
                    self.selected.name
                ),
            )
        })
    }
}

/// Why a keeper's `hello` is not one this build speaks with; none when it
/// is.
fn spoken(hello: &Value, name: &str) -> Option<Error> {
    if hello["ok"] == true && hello["keeper"]["protocol"] == crate::vm::keeper::PROTOCOL {
        return None;
    }
    Some(
        Error::new(
            Code::Update,
            format!("{name}'s session keeper speaks another protocol than this Atlas."),
        )
        .with(
            "next",
            json!("Update Atlas so the keeper and this command are one build."),
        ),
    )
}

/// How long a look at a VM's keeper is reused: reads that poll ask launchd
/// no more often than this, and never two at once.
const FRESH: Duration = Duration::from_secs(3);

/// What a look at a VM's keeper is for: the settings' name for it, the Second
/// Login named beside it, and the VM.
type Looked = (String, Option<String>, DesktopId);

/// The last look at each VM's keeper, shared by every client in the process:
/// one for each VM it was asked of, a handful at most.
static LOOKS: std::sync::Mutex<Vec<(Looked, &'static Shared<Link, Looked>)>> =
    std::sync::Mutex::new(Vec::new());

/// The shared look for `key`, made the first time it is asked for.
fn looks_of(key: &Looked) -> &'static Shared<Link, Looked> {
    let mut looks = LOOKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, shared)) = looks.iter().find(|(kept, _)| kept == key) {
        return shared;
    }
    let shared: &'static Shared<Link, Looked> = Box::leak(Box::new(Shared::new(FRESH)));
    looks.push((key.clone(), shared));
    shared
}

/// The link to `selected`'s VM `vm`: its keeper when it answers its hello
/// and the VM is paired, else unreachable with why. One look runs at a time,
/// off the caller, and is reused for [`FRESH`], so a status polled every
/// second never storms launchd; a caller that finds one running and nothing
/// fresh is told the keeper is not answering yet.
pub(crate) fn probe(selected: &Selected, vm: &DesktopId, timeout: Duration) -> Link {
    let key = (
        selected.name.clone(),
        selected.user_entry.clone(),
        vm.clone(),
    );
    shared_look(looks_of(&key), selected, vm, timeout, looked)
}

/// [`probe`] through `looks`, `look` making each new look.
fn shared_look(
    looks: &'static Shared<Link, Looked>,
    selected: &Selected,
    vm: &DesktopId,
    timeout: Duration,
    look: fn(&Selected, &DesktopId, Duration) -> Link,
) -> Link {
    let key = (
        selected.name.clone(),
        selected.user_entry.clone(),
        vm.clone(),
    );
    let (asked, named) = (selected.clone(), vm.clone());
    looks
        .get_blocking(key, HELLO + Duration::from_secs(1), move || {
            look(&asked, &named, timeout)
        })
        .map_or_else(
            || {
                Link::Unreachable(Unreachable {
                    selected: Arc::new(selected.clone()),
                    timeout,
                    why: Some(format!(
                        "{}'s session keeper is not answering yet",
                        selected.name
                    )),
                })
            },
            |link| link.with_timeout(timeout),
        )
}

/// Why the keeper of `name`, whose station folder is `folder`, does not
/// answer: the failure its last start kept there, when it kept one.
fn not_running(name: &str, folder: &Path) -> String {
    crate::vm::keeper::start_failure(folder).map_or_else(
        || format!("{name}'s session keeper is not running"),
        |failed| format!("{name}'s session keeper could not start: {failed}"),
    )
}

/// One look at `selected`'s VM `vm`'s keeper. It waits on the keeper and
/// launchd briefly.
fn looked(selected: &Selected, vm: &DesktopId, timeout: Duration) -> Link {
    let selected = Arc::new(selected.clone());
    let unreachable = |why: String| {
        Link::Unreachable(Unreachable {
            selected: selected.clone(),
            timeout,
            why: Some(why),
        })
    };
    let Ok(station) = Station::own() else {
        return unreachable(format!("{}'s session keeper is not running", selected.name));
    };
    let folder = station.path_of(vm);
    let not_running = not_running(&selected.name, &folder);
    let Ok(socket) = crate::vm::keeper::socket_of(vm) else {
        return unreachable(not_running);
    };
    if std::fs::symlink_metadata(&socket).is_err() {
        return unreachable(not_running);
    }
    let keeper = Keeper::at(
        selected.clone(),
        vm.clone(),
        socket,
        timeout,
        Arc::new(Launchd),
    );
    let hello = match keeper.exchange(&json!({"op": "hello"}), HELLO) {
        Ok(mut answer) => answer.pop().unwrap_or_default(),
        Err(error) => return unreachable(format!("{not_running}: {}", error.error)),
    };
    if let Some(refused) = spoken(&hello, &selected.name) {
        return unreachable(refused.error);
    }
    match atlas_session::pairing_file::PairingFile::at(folder).read() {
        Ok(Some(Enrolment::Paired(_))) => Link::Keeper(keeper),
        _ => unreachable(format!("{} is not paired yet", selected.name)),
    }
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;
