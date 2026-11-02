//! The Unix socket a helper-side service listens on, and reading one bounded
//! request line from it. Shared by the helper daemon and the input service.
use atlas_core::{
    context::context,
    error::{Code, Error, Result},
    files::{LockFile, LockOpen, open_lock_file},
};
use fs2::FileExt;
use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    net::{UnixListener, UnixStream},
};

/// A bound socket. The helper-owned directory and an exclusive lifetime flock
/// make removing a stale socket safe without taking over a running service;
/// the socket is removed on drop only if it is still the one bound here.
pub struct Endpoint {
    socket: PathBuf,
    inode: u64,
    _lock: LockFile,
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

/// Who may reach the socket's directory.
#[derive(Debug, Clone, Copy)]
pub enum Directory {
    /// Others may traverse it but never write: created with this mode.
    Shared(u32),
    /// The helper alone, plus one search-only grant for the controller —
    /// setup's private layout. A directory that lost the grant (or was
    /// recreated) gets it back; any other access is refused.
    Private { controller: u32 },
}

fn refuse(role: &str, rule: &str) -> Error {
    Error::new(
        Code::Identity,
        format!("The {role} socket directory must belong to the helper and {rule}"),
    )
}

/// Give the controller search access to `directory`, as setup does.
fn grant_search(directory: &Path, controller: u32) -> Result<()> {
    let name = atlas_core::account::account_with_uid(controller)?
        .ok_or_else(|| Error::new(Code::Identity, "The controller account does not exist"))?
        .name;
    let granted = std::process::Command::new("/bin/chmod")
        .args(["+a", &format!("user:{name} allow search")])
        .arg(directory)
        .status()
        .map_err(context("Grant search access to", directory))?;
    if !granted.success() {
        return Err(Error::new(
            Code::Identity,
            "The controller could not be given access to the socket directory",
        ));
    }

    Ok(())
}

/// Create the socket's directory if absent and check it against `policy`.
fn prepare(parent: &Path, uid: u32, role: &str, policy: Directory) -> Result<()> {
    let mode = match policy {
        Directory::Shared(mode) => mode,
        Directory::Private { .. } => 0o700,
    };

    match fs::create_dir(parent) {
        Ok(()) => fs::set_permissions(parent, fs::Permissions::from_mode(mode))
            .map_err(context("Set socket directory permissions", parent))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(context("Create socket directory", parent)(error));
        }
    }

    let metadata =
        fs::symlink_metadata(parent).map_err(context("Inspect socket directory", parent))?;
    if !metadata.is_dir() || metadata.uid() != uid {
        return Err(refuse(role, "be a directory"));
    }

    match policy {
        // An ACL may grant writes the mode bits do not show.
        Directory::Shared(_) => {
            if metadata.mode() & 0o022 != 0
                || atlas_core::acl::allows_write(parent)
                    .map_err(context("Read socket directory ACL", parent))?
            {
                return Err(refuse(role, "deny writes by others"));
            }
        }
        Directory::Private { controller } => {
            if metadata.mode() & 0o077 != 0 {
                return Err(refuse(role, "give no access by mode to others"));
            }

            let lines = atlas_core::acl::acl_lines(parent)?;
            match atlas_core::acl::acl_policy(&lines, Some(controller)) {
                Ok(true) => {}
                Ok(false) => grant_search(parent, controller)?,
                Err(entry) => return Err(refuse(role, &format!("not carry {entry}"))),
            }
        }
    }

    Ok(())
}

/// Bind `socket` for the helper `uid`, its directory held to `policy`.
/// `role` names the service in errors.
///
/// # Errors
///
/// Returns an error when the folder breaks `policy`, the service already runs
/// or the socket can't be bound.
pub async fn bind(
    socket: &Path,
    uid: u32,
    role: &str,
    policy: Directory,
) -> Result<(UnixListener, Endpoint)> {
    let parent = socket
        .parent()
        .ok_or_else(|| Error::new(Code::Config, "Socket needs a parent directory"))?;
    prepare(parent, uid, role, policy)?;

    let lock = open_lock_file(&parent.join("service.lock"), LockOpen::ReadWrite)
        .map_err(context("Open service lock in", parent))?;

    let lock = LockFile(lock);
    FileExt::try_lock_exclusive(&lock.0).map_err(|_| {
        Error::new(
            Code::Busy,
            format!("Another {role} already owns this directory"),
        )
    })?;

    if let Ok(metadata) = fs::symlink_metadata(socket) {
        if metadata.uid() != uid || !metadata.file_type().is_socket() {
            return Err(Error::new(
                Code::Identity,
                format!("The existing {role} endpoint is not a helper-owned socket"),
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
                    Code::Busy,
                    format!(
                        "The existing {role} endpoint is reachable or cannot be verified stale"
                    ),
                ));
            }
        }
    }

    let listener = UnixListener::bind(socket).map_err(context("Bind service socket", socket))?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o666))
        .map_err(context("Set socket permissions", socket))?;

    let endpoint = Endpoint {
        socket: socket.to_owned(),
        inode: fs::symlink_metadata(socket)
            .map_err(context("Inspect bound socket", socket))?
            .ino(),
        _lock: lock,
    };

    Ok((listener, endpoint))
}

/// Answer a connection there is no room to serve with `reply`, then close it.
/// The reply is written once without waiting (a new socket's buffer holds
/// it), so turning a client away never stalls the accept loop.
pub fn turn_away(stream: UnixStream, reply: &serde_json::Value) {
    use std::io::Write;

    let Ok(mut line) = serde_json::to_vec(reply) else {
        return;
    };
    line.push(b'\n');

    // The descriptor stays non-blocking once taken from the runtime.
    if let Ok(mut stream) = stream.into_std() {
        let _ = stream.write(&line);
    }
}

/// One newline-terminated request line of at most `limit` bytes, however the
/// peer fragments it, within three seconds. Only new bytes are scanned.
///
/// # Errors
///
/// Returns an error for a line that is too long, cut short or not finished
/// within three seconds.
pub async fn read_line<R: AsyncRead + Unpin>(stream: &mut R, limit: usize) -> Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut body = Vec::new();
        let mut buffer = [0; 4096];

        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                return Err(Error::new(Code::Protocol, "Incomplete request"));
            }

            let scanned = body.len();
            body.extend_from_slice(&buffer[..n]);

            let end = body[scanned..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|offset| scanned + offset);
            if end.unwrap_or(body.len()) > limit {
                return Err(Error::new(
                    Code::Protocol,
                    format!("Request exceeds {} KiB", limit / 1024),
                ));
            }

            if let Some(end) = end {
                body.truncate(end);
                return Ok(body);
            }
        }
    })
    .await
    .map_err(|_| Error::new(Code::Transport, "Request timed out"))?
}

#[cfg(test)]
mod tests;
