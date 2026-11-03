//! The broker's private endpoint for one controller and helper: a folder
//! only the controller can reach, its lifetime lock, its three sockets and
//! its record. A reachable or unverifiable socket is never taken over, an
//! unsafe folder is refused rather than repaired, and only the sockets this
//! instance bound are ever removed.
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::{LockFile, open_lock},
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};

/// Where every broker's folder is made.
const BASE: &str = "/private/tmp";
/// The longest socket path a Unix address holds, in bytes.
const SOCKET_PATH_MAX: usize = 103;
/// How long a socket left behind is given to answer before it counts as stale.
const STALE: Duration = Duration::from_millis(250);

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
    let prefix: String = key[..8].iter().map(|byte| format!("{byte:02x}")).collect();
    base.join(format!("atlas-fp.{controller}.{prefix}"))
}

/// The sockets clients connect to.
pub(crate) struct Listeners {
    pub(crate) control: UnixListener,
    pub(crate) frame: UnixListener,
    pub(crate) management: UnixListener,
}

/// The bound endpoint. It holds the folder's lifetime lock, and removes on
/// drop each socket it bound that is still that same socket.
pub(crate) struct Endpoint {
    folder: PathBuf,
    bound: Vec<(PathBuf, u64)>,
    _lock: LockFile,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        for (socket, inode) in &self.bound {
            if fs::symlink_metadata(socket)
                .is_ok_and(|metadata| metadata.ino() == *inode && metadata.file_type().is_socket())
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

/// Make the folder when absent, then hold it to its rules: a real folder of
/// `controller`, mode 0700, with no ACL.
fn prepare(folder: &Path, controller: u32) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(folder) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Error::writing(folder, error)),
    }
    let metadata = fs::symlink_metadata(folder).map_err(context("Inspect", folder))?;
    if !metadata.is_dir() || metadata.uid() != controller {
        return Err(unsafe_folder(folder, "be a folder of this account"));
    }
    if metadata.mode() & 0o777 != 0o700 {
        return Err(unsafe_folder(folder, "have mode 0700"));
    }
    if atlas_core::acl::present(folder).map_err(context("Read the ACL of", folder))? {
        return Err(unsafe_folder(folder, "carry no ACL"));
    }
    Ok(())
}

/// Remove `socket` if it was left behind: this account's socket that no one
/// answers. Anything else there is refused.
async fn clear_stale(socket: &Path, controller: u32) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(socket) else {
        return Ok(());
    };
    if metadata.uid() != controller || !metadata.file_type().is_socket() {
        return Err(unsafe_folder(socket, "be a socket of this account"));
    }
    match tokio::time::timeout(STALE, UnixStream::connect(socket)).await {
        Ok(Err(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
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
    prepare(&folder, controller)?;
    let lock = open_lock(&folder.join("owner.lock"))?;
    FileExt::try_lock_exclusive(&lock.0)
        .map_err(|_| Error::new(Code::Busy, "Another frame service owns this helper"))?;

    // Made first, so a failure part way removes what was already bound.
    let mut endpoint = Endpoint {
        folder,
        bound: Vec::new(),
        _lock: lock,
    };
    let mut listeners = Vec::new();
    for name in ["c.sock", "f.sock", "m.sock"] {
        let socket = endpoint.folder.join(name);
        if socket.as_os_str().len() > SOCKET_PATH_MAX {
            return Err(Error::new(
                Code::Config,
                format!("The socket path {} is too long", socket.display()),
            ));
        }
        clear_stale(&socket, controller).await?;
        let listener = UnixListener::bind(&socket).map_err(context("Bind", &socket))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
            .map_err(context("Set the mode of", &socket))?;
        let inode = fs::symlink_metadata(&socket)
            .map_err(context("Inspect", &socket))?
            .ino();
        endpoint.bound.push((socket, inode));
        listeners.push(listener);
    }
    let mut listeners = listeners.into_iter();
    let (Some(control), Some(frame), Some(management)) =
        (listeners.next(), listeners.next(), listeners.next())
    else {
        return Err(Error::new(
            Code::State,
            "The frame service bound too few sockets",
        ));
    };
    Ok((
        Listeners {
            control,
            frame,
            management,
        },
        endpoint,
    ))
}

#[cfg(test)]
mod tests;
