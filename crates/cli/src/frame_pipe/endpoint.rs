//! The broker's private endpoint for one controller and helper: a folder
//! only the controller can reach, its lifetime lock, its two sockets and
//! its record. A reachable or unverifiable socket is never taken over, an
//! unsafe folder is refused rather than repaired, and only the sockets this
//! instance bound are ever removed.
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::{LockFile, LockOpen, open_lock_file},
    name::hex,
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};

/// Where every broker's folder is made.
const BASE: &str = "/private/tmp";
/// The longest socket path a Unix address holds, in bytes.
const SOCKET_PATH_MAX: usize = 103;
/// How long a socket left behind is given to answer: a dead one refuses at
/// once, and one that neither answers nor refuses in time counts as alive.
const STALE: Duration = Duration::from_secs(1);

/// The broker's identity for a controller and helper, whatever paths or
/// sessions serve them.
pub(crate) fn owner_key(controller: u32, helper: u32) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"atlas-frame-owner-v1");
    hash.update([0]);
    hash.update(controller.to_be_bytes());
    hash.update(helper.to_be_bytes());
    hash.finalize().into()
}

/// The broker's folder under `base`.
fn folder(base: &Path, controller: u32, key: &[u8; 32]) -> PathBuf {
    base.join(format!("atlas-fp.{controller}.{}", hex(&key[..8])))
}

/// The sockets clients connect to.
pub(crate) struct Listeners {
    pub(crate) control: UnixListener,
    pub(crate) frame: UnixListener,
}

/// A file system object's device and inode.
type Identity = (u64, u64);

fn identity(path: &Path) -> Option<Identity> {
    fs::symlink_metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}

/// The bound endpoint. It holds the folder's lifetime lock, and removes on
/// drop each socket it bound that is still that same socket in that same
/// folder.
pub(crate) struct Endpoint {
    folder: PathBuf,
    place: Identity,
    bound: Vec<(PathBuf, Identity)>,
    _lock: LockFile,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        if identity(&self.folder) != Some(self.place) {
            return;
        }
        for (socket, owned) in &self.bound {
            if identity(socket) == Some(*owned)
                && fs::symlink_metadata(socket)
                    .is_ok_and(|metadata| metadata.file_type().is_socket())
            {
                // Best effort: a socket left behind is found stale next time.
                let _ = fs::remove_file(socket);
            }
        }
    }
}

impl Endpoint {
    /// Where the broker's record is kept.
    pub(crate) fn record(&self) -> PathBuf {
        self.folder.join("service.json")
    }

    /// Where clients find the socket named `name`.
    pub(crate) fn socket(&self, name: &str) -> PathBuf {
        self.folder.join(name)
    }
}

fn unsafe_folder(folder: &Path, rule: &str) -> Error {
    Error::new(
        Code::Identity,
        format!("{} must {rule}; Atlas does not repair it", folder.display()),
    )
}

/// Make the folder when absent, then hold it to its rules through one open
/// handle: a real folder of `controller`, mode 0700, with no ACL. Returns
/// its identity, which every later removal checks.
fn prepare(folder: &Path, controller: u32) -> Result<Identity> {
    match fs::DirBuilder::new().mode(0o700).create(folder) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Error::writing(folder, error)),
    }
    let handle = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(folder)
        .map_err(|_| unsafe_folder(folder, "be a folder of this account"))?;
    let metadata = handle.metadata().map_err(context("Inspect", folder))?;
    if !metadata.is_dir() || metadata.uid() != controller {
        return Err(unsafe_folder(folder, "be a folder of this account"));
    }
    if metadata.mode() & 0o777 != 0o700 {
        return Err(unsafe_folder(folder, "have mode 0700"));
    }
    if atlas_core::acl::present_on(&handle).map_err(context("Read the ACL of", folder))? {
        return Err(unsafe_folder(folder, "carry no ACL"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

/// The folder's lifetime lock file, refused rather than repaired unless it is
/// a plain file of `controller` with mode 0600 and no ACL.
fn owner_lock(path: &Path, controller: u32) -> Result<LockFile> {
    let file =
        open_lock_file(path, LockOpen::ReadWrite).map_err(|error| Error::writing(path, error))?;
    let metadata = file.metadata().map_err(context("Inspect", path))?;
    if !metadata.is_file() || metadata.uid() != controller || metadata.mode() & 0o777 != 0o600 {
        return Err(unsafe_folder(path, "be a 0600 file of this account"));
    }
    if atlas_core::acl::present_on(&file).map_err(context("Read the ACL of", path))? {
        return Err(unsafe_folder(path, "carry no ACL"));
    }
    Ok(LockFile(file))
}

/// Record the full owner in the held lock file, or refuse a folder another
/// controller and helper own: only part of the key names the folder.
fn claim(lock: &LockFile, folder: &Path, key: &[u8; 32], helper: u32) -> Result<()> {
    use std::io::{Read, Seek, Write};
    let owner = format!("{} {helper}\n", hex(key));
    let mut file = &lock.0;
    let mut found = String::new();
    file.take(256)
        .read_to_string(&mut found)
        .map_err(context("Read the owner of", folder))?;
    if found.is_empty() {
        file.seek(std::io::SeekFrom::Start(0))
            .and_then(|_| file.write_all(owner.as_bytes()))
            .and_then(|()| file.sync_all())
            .map_err(context("Record the owner of", folder))?;
        return Ok(());
    }
    if found != owner {
        return Err(unsafe_folder(
            folder,
            "belong to this controller and helper",
        ));
    }
    Ok(())
}

/// Remove `socket` if it was left behind: this account's socket that no one
/// answers, still in the folder `place` names. Anything else there is
/// refused.
async fn clear_stale(socket: &Path, controller: u32, place: Identity) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(socket) else {
        return Ok(());
    };
    if metadata.uid() != controller
        || !metadata.file_type().is_socket()
        || metadata.mode() & 0o777 != 0o600
    {
        return Err(unsafe_folder(socket, "be a 0600 socket of this account"));
    }
    if atlas_core::acl::present(socket).map_err(context("Read the ACL of", socket))? {
        return Err(unsafe_folder(socket, "carry no ACL"));
    }
    match tokio::time::timeout(STALE, UnixStream::connect(socket)).await {
        Ok(Err(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            // Only the very socket probed is removed, from the very folder
            // checked, never one put there since.
            let folder = socket.parent().unwrap_or(socket);
            if identity(socket) != Some((metadata.dev(), metadata.ino()))
                || identity(folder) != Some(place)
            {
                return Err(Error::new(
                    Code::Busy,
                    "Another frame service is starting for this helper",
                ));
            }
            fs::remove_file(socket).map_err(context("Remove the stale socket", socket))
        }
        _ => Err(Error::new(
            Code::Busy,
            "Another frame service answers for this helper",
        )),
    }
}

/// Bind the broker's endpoint for `controller` and helper `helper`.
///
/// # Errors
///
/// Refuses an unsafe folder or socket, a service that already runs, or a
/// socket path too long for a Unix address; returns failures to bind.
pub(crate) async fn bind(controller: u32, helper: u32) -> Result<(Listeners, Endpoint)> {
    bind_in(Path::new(BASE), controller, helper).await
}

async fn bind_in(base: &Path, controller: u32, helper: u32) -> Result<(Listeners, Endpoint)> {
    let folder = folder(base, controller, &owner_key(controller, helper));
    let place = prepare(&folder, controller)?;
    let lock = owner_lock(&folder.join("owner.lock"), controller)?;
    FileExt::try_lock_exclusive(&lock.0)
        .map_err(|_| Error::new(Code::Busy, "Another frame service owns this helper"))?;
    claim(&lock, &folder, &owner_key(controller, helper), helper)?;

    // Made first, so a failure part way removes what was already bound.
    let mut endpoint = Endpoint {
        folder,
        place,
        bound: Vec::new(),
        _lock: lock,
    };
    let mut listeners = Vec::new();
    for name in ["c.sock", "f.sock"] {
        let socket = endpoint.folder.join(name);
        if socket.as_os_str().len() > SOCKET_PATH_MAX {
            return Err(Error::new(
                Code::Config,
                format!("The socket path {} is too long", socket.display()),
            ));
        }
        clear_stale(&socket, controller, place).await?;
        let listener = UnixListener::bind(&socket).map_err(context("Bind", &socket))?;
        // Owned for cleanup from the moment it exists.
        // Not known to be this socket, so not removed: the next start finds
        // it and clears it only if no one answers there.
        let owned = identity(&socket)
            .ok_or_else(|| Error::new(Code::Identity, "The bound socket could not be inspected"))?;
        endpoint.bound.push((socket.clone(), owned));
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
            .map_err(context("Set the mode of", &socket))?;
        // Only this account can change the folder; still, publish only the
        // very socket bound, private, in the very folder checked.
        let private = fs::symlink_metadata(&socket).is_ok_and(|metadata| {
            metadata.file_type().is_socket()
                && metadata.uid() == controller
                && metadata.mode() & 0o777 == 0o600
                && (metadata.dev(), metadata.ino()) == owned
        });
        if !private || identity(&endpoint.folder) != Some(place) {
            return Err(unsafe_folder(&socket, "stay the private socket bound"));
        }
        listeners.push(listener);
    }
    let mut listeners = listeners.into_iter();
    let (Some(control), Some(frame)) = (listeners.next(), listeners.next()) else {
        return Err(Error::new(
            Code::State,
            "The frame service bound too few sockets",
        ));
    };
    Ok((Listeners { control, frame }, endpoint))
}

#[cfg(test)]
mod tests;
