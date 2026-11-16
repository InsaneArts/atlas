//! The VM's `station` socket, which Atlas VM listens on and splices to the
//! guest, trusted before each dial. The socket must be one in the VM's own
//! folder, a private folder of this account's; once connected, the process
//! that created and listens on it (which `LOCAL_PEERTOKEN` names on the
//! connecting end) must be signed as Atlas VM and be the very runner
//! launchd runs for the VM. Anything else ends the attempt, with why.
use super::{System, Vms};
use atlas_core::{
    app_bundle::{VM_IDENTIFIER, requirement_for},
    desktop::DesktopId,
    error::{Code, Error, Result},
    launchd::Job,
};
use atlas_session::{
    secure::Purpose,
    station::{Dial, Dialing, Raw},
};
use std::{
    fmt,
    os::{
        fd::AsFd,
        unix::{
            fs::{FileTypeExt, MetadataExt},
            net::UnixStream,
        },
    },
    path::Path,
};
use tokio::io::AsyncWriteExt;

/// Why the VM's station socket was not trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Untrusted {
    /// The VM's folder is not there.
    NoFolder,
    /// The VM's folder is not a private (0700) folder of this account's.
    FolderNotPrivate,
    /// Nothing is at the station path.
    NoSocket,
    /// The station path leads out of the VM's folder.
    OutsideFolder,
    /// The station path is not a socket of this account's.
    NotASocket,
    /// The process listening is not signed as Atlas VM, or is another
    /// account's.
    NotAtlasVm,
    /// The process listening is not the runner launchd runs for the VM.
    NotLaunchds,
}

impl fmt::Display for Untrusted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoFolder => "the VM's folder is not there",
            Self::FolderNotPrivate => "the VM's folder is not a private folder of this account's",
            Self::NoSocket => "Atlas VM is not listening for the VM's station",
            Self::OutsideFolder => "the VM's station socket leads out of its folder",
            Self::NotASocket => "the VM's station path is not a socket of this account's",
            Self::NotAtlasVm => "the VM's station socket is not served by Atlas VM",
            Self::NotLaunchds => "the VM's station socket is not served by the runner launchd runs",
        })
    }
}

impl Untrusted {
    fn refusal(self, vm: &DesktopId) -> Error {
        Error::new(
            Code::Identity,
            format!("VM {} was not dialled: {self}.", vm.as_str()),
        )
    }
}

/// What a path is, as its own entry says, never followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Folder,
    Socket,
    Link,
    Other,
}

/// A path's own entry: what it is, whose, and its mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    kind: Kind,
    uid: u32,
    mode: u32,
}

impl Entry {
    /// The entry at `path`; none when nothing is there.
    fn at(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        let kind = metadata.file_type();
        Some(Self {
            kind: if kind.is_symlink() {
                Kind::Link
            } else if kind.is_dir() {
                Kind::Folder
            } else if kind.is_socket() {
                Kind::Socket
            } else {
                Kind::Other
            },
            uid: metadata.uid(),
            mode: metadata.mode(),
        })
    }
}

/// Whether the VM's `folder` and its `station` entry may be dialled by the
/// account `uid`.
fn judge_path(
    folder: Option<Entry>,
    station: Option<Entry>,
    uid: u32,
) -> std::result::Result<(), Untrusted> {
    let folder = folder.ok_or(Untrusted::NoFolder)?;
    if folder.kind != Kind::Folder || folder.uid != uid || folder.mode & 0o077 != 0 {
        return Err(Untrusted::FolderNotPrivate);
    }
    let station = station.ok_or(Untrusted::NoSocket)?;
    match station.kind {
        Kind::Link => Err(Untrusted::OutsideFolder),
        Kind::Socket if station.uid == uid => Ok(()),
        _ => Err(Untrusted::NotASocket),
    }
}

/// Whether the process listening, as its audit token names it when its
/// code satisfies Atlas VM's requirement (its PID and account), is the
/// runner launchd runs now for the VM, in the account `uid`.
fn judge_peer(
    listener: Option<(u32, u32)>,
    launchd: Option<u32>,
    uid: u32,
) -> std::result::Result<(), Untrusted> {
    let (pid, account) = listener.ok_or(Untrusted::NotAtlasVm)?;
    if account != uid {
        return Err(Untrusted::NotAtlasVm);
    }
    if launchd != Some(pid) {
        return Err(Untrusted::NotLaunchds);
    }
    Ok(())
}

/// What the trust in a VM's station socket is judged on before it is
/// dialled: its folder's and its socket's own entries, which file each is,
/// and the runner launchd runs. While these stay the same, so does a
/// refusal: nothing changed to dial again for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    folder: Option<Entry>,
    station: Option<Entry>,
    /// The device and inode of each, so a socket made again is a change.
    places: [Option<(u64, u64)>; 2],
    launchd: Option<u32>,
}

/// Why a dial of the VM's station socket did not give a connection.
#[derive(Debug)]
pub enum Refused {
    /// It was not trusted, judged on `facts`.
    Untrusted { why: Untrusted, facts: Facts },
    /// It could not be dialled, or launchd could not be asked.
    Failed(Error),
}

impl Refused {
    /// The refusal as Atlas reports it, for `vm`.
    #[must_use]
    pub fn into_error(self, vm: &DesktopId) -> Error {
        match self {
            Self::Untrusted { why, .. } => why.refusal(vm),
            Self::Failed(error) => error,
        }
    }
}

/// The facts `vm`'s station socket is judged on now. It waits on launchd,
/// so it runs off any runtime.
///
/// # Errors
///
/// Fails when this account or launchd cannot be read.
pub fn facts(vm: &DesktopId) -> Result<Facts> {
    facts_with(&Vms::own()?, &super::Real, vm)
}

fn facts_with(vms: &Vms, system: &impl System, vm: &DesktopId) -> Result<Facts> {
    let folder = vms.folder(vm);
    let station = folder.join("station");
    let launchd = match system.job(&vms.domain(), &Vms::label(vm))? {
        Job::Loaded(loaded) => loaded.pid,
        Job::Absent => None,
    };
    Ok(Facts {
        folder: Entry::at(&folder),
        station: Entry::at(&station),
        places: [place(&folder), place(&station)],
        launchd,
    })
}

/// The device and inode at `path`, never followed.
fn place(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

/// A connection to `vm`'s station socket, trusted: checked before it is
/// dialled and after. It waits on launchd, so it runs off any runtime.
///
/// # Errors
///
/// Refuses, with why and the facts it was judged on, a socket that is not
/// trusted; fails when the socket cannot be dialled or launchd cannot be
/// asked.
pub fn trusted(vm: &DesktopId) -> std::result::Result<UnixStream, Refused> {
    trusted_with(
        &Vms::own().map_err(Refused::Failed)?,
        &super::Real,
        signed_vm,
        vm,
    )
}

/// Who listens on a connected socket, judged: its PID and account, when its
/// running code is Atlas VM's.
type Listening = fn(&UnixStream) -> Option<(u32, u32)>;

/// The listener on `stream`, when its running code is signed as Atlas VM.
fn signed_vm(stream: &UnixStream) -> Option<(u32, u32)> {
    atlas_native::peer::running(stream.as_fd(), &requirement_for(VM_IDENTIFIER))
        .map(|peer| (peer.pid, peer.uid))
}

fn trusted_with(
    vms: &Vms,
    system: &impl System,
    listening: Listening,
    vm: &DesktopId,
) -> std::result::Result<UnixStream, Refused> {
    let facts = facts_with(vms, system, vm).map_err(Refused::Failed)?;
    if let Err(why) = judge_path(facts.folder, facts.station, vms.uid) {
        return Err(Refused::Untrusted { why, facts });
    }
    let stream = UnixStream::connect(vms.folder(vm).join("station")).map_err(|error| {
        Refused::Failed(
            Error::new(
                Code::Transport,
                format!("Could not reach VM {}.", vm.as_str()),
            )
            .with_source(error),
        )
    })?;
    if let Err(why) = judge_peer(listening(&stream), facts.launchd, vms.uid) {
        return Err(Refused::Untrusted { why, facts });
    }
    Ok(stream)
}

/// The way to a VM's guest for one purpose: each dial a new trusted
/// connection to its station socket, opened with the purpose's byte.
#[derive(Clone, Debug)]
pub struct StationSocket {
    pub vm: DesktopId,
    pub purpose: Purpose,
}

impl Dial for StationSocket {
    fn dial(&self) -> Dialing<'_> {
        Box::pin(async move {
            dial(&self.vm, self.purpose)
                .await
                .map_err(|refused| std::io::Error::other(refused.into_error(&self.vm)))
        })
    }
}

/// A new trusted connection to `vm`'s guest for `purpose`, opened with the
/// purpose's byte.
///
/// # Errors
///
/// As [`trusted`]; fails when the byte cannot be said.
pub(super) async fn dial(vm: &DesktopId, purpose: Purpose) -> std::result::Result<Raw, Refused> {
    let vms = Vms::own().map_err(Refused::Failed)?;
    dial_with(
        vms,
        std::sync::Arc::new(super::Real),
        signed_vm,
        vm,
        purpose,
    )
    .await
}

/// [`dial`] on `vms` and `system`, its listener judged by `listening`.
pub(super) async fn dial_with<S: System + Send + Sync + 'static>(
    vms: Vms,
    system: std::sync::Arc<S>,
    listening: Listening,
    vm: &DesktopId,
    purpose: Purpose,
) -> std::result::Result<Raw, Refused> {
    let failed = |error: std::io::Error| {
        Refused::Failed(
            Error::new(
                Code::Transport,
                format!("Could not reach VM {}.", vm.as_str()),
            )
            .with_source(error),
        )
    };
    let named = vm.clone();
    let stream =
        tokio::task::spawn_blocking(move || trusted_with(&vms, &*system, listening, &named))
            .await
            .map_err(|error| failed(std::io::Error::other(error)))??;
    stream.set_nonblocking(true).map_err(failed)?;
    let mut stream = tokio::net::UnixStream::from_std(stream).map_err(failed)?;
    stream.write_all(&[purpose.byte()]).await.map_err(failed)?;
    Ok(Box::new(stream) as Raw)
}

#[cfg(test)]
impl Facts {
    /// Facts that differ from another number's.
    pub(super) const fn numbered(launchd: u32) -> Self {
        Self {
            folder: None,
            station: None,
            places: [None, None],
            launchd: Some(launchd),
        }
    }
}

#[cfg(test)]
#[path = "station_tests.rs"]
mod tests;
