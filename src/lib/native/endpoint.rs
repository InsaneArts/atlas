//! The Unix socket a helper-side service listens on, and reading one bounded
//! request line from it. Shared by the helper daemon and the input service.
use crate::error::{Error, Result};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
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
pub(crate) struct Endpoint {
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

/// Who may reach the socket's directory.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Directory {
    /// Others may traverse it but never write: created with this mode.
    Shared(u32),
    /// The helper alone, plus one search-only grant for the controller —
    /// setup's private layout. A directory that lost the grant (or was
    /// recreated) gets it back; any other access is refused.
    Private { controller: u32 },
}

fn refuse(role: &str, rule: &str) -> Error {
    Error::new(
        "identity",
        format!("The {role} socket directory must belong to the helper and {rule}"),
    )
}

/// Give the controller search access to `directory`, as setup does.
fn grant_search(directory: &Path, controller: u32) -> Result<()> {
    let name = crate::setup::account_with_uid(controller)?
        .ok_or_else(|| Error::new("identity", "The controller account does not exist"))?
        .name;
    let granted = std::process::Command::new("/bin/chmod")
        .args(["+a", &format!("user:{name} allow search")])
        .arg(directory)
        .status()?;
    if !granted.success() {
        return Err(Error::new(
            "identity",
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
        Ok(()) => fs::set_permissions(parent, fs::Permissions::from_mode(mode))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }

    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.uid() != uid {
        return Err(refuse(role, "be a directory"));
    }

    match policy {
        // An ACL may grant writes the mode bits do not show.
        Directory::Shared(_) => {
            if metadata.mode() & 0o022 != 0 || crate::acl::allows_write(parent)? {
                return Err(refuse(role, "deny writes by others"));
            }
        }
        Directory::Private { controller } => {
            if metadata.mode() & 0o077 != 0 {
                return Err(refuse(role, "give no access by mode to others"));
            }

            let lines = crate::setup::helper::acl_lines(parent)?;
            match crate::setup::helper::acl_policy(&lines, Some(controller)) {
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
pub(crate) async fn bind(
    socket: &Path,
    uid: u32,
    role: &str,
    policy: Directory,
) -> Result<(UnixListener, Endpoint)> {
    let parent = socket
        .parent()
        .ok_or_else(|| Error::new("config", "Socket needs a parent directory"))?;
    prepare(parent, uid, role, policy)?;

    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join("service.lock"))?;

    FileExt::try_lock_exclusive(&lock).map_err(|_| {
        Error::new(
            "busy",
            format!("Another {role} already owns this directory"),
        )
    })?;

    if let Ok(metadata) = fs::symlink_metadata(socket) {
        if metadata.uid() != uid || !metadata.file_type().is_socket() {
            return Err(Error::new(
                "identity",
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
                    "busy",
                    format!(
                        "The existing {role} endpoint is reachable or cannot be verified stale"
                    ),
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

/// Answer a connection there is no room to serve with `reply`, then close it.
/// The reply is written once without waiting (a new socket's buffer holds
/// it), so turning a client away never stalls the accept loop.
pub(crate) fn turn_away(stream: UnixStream, reply: &serde_json::Value) {
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
pub(crate) async fn read_line<R: AsyncRead + Unpin>(
    stream: &mut R,
    limit: usize,
) -> Result<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut body = Vec::new();
        let mut buffer = [0; 4096];

        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                return Err(Error::new("protocol", "Incomplete request"));
            }

            let scanned = body.len();
            body.extend_from_slice(&buffer[..n]);

            let end = body[scanned..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|offset| scanned + offset);
            if end.unwrap_or(body.len()) > limit {
                return Err(Error::new(
                    "protocol",
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
    .map_err(|_| Error::new("transport", "Request timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn a_fragmented_line_is_reassembled_and_bounded() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let reader = tokio::spawn(async move { read_line(&mut server, 1024).await });

        client.write_all(b"{\"op\":").await.unwrap();
        client.write_all(b"\"hello\"}\nextra").await.unwrap();
        assert_eq!(reader.await.unwrap().unwrap(), b"{\"op\":\"hello\"}");

        let (mut client, mut server) = tokio::io::duplex(4096);
        client.write_all(&[b'x'; 2048]).await.unwrap();
        let error = read_line(&mut server, 1024).await.unwrap_err();
        assert_eq!(error.error, "Request exceeds 1 KiB");
    }

    #[tokio::test]
    async fn a_live_endpoint_is_never_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("run/s.sock");
        let uid = nix::unistd::getuid().as_raw();
        let (_listener, endpoint) = bind(&socket, uid, "test service", Directory::Shared(0o700))
            .await
            .unwrap();

        assert_eq!(
            fs::metadata(socket.parent().unwrap()).unwrap().mode() & 0o777,
            0o700
        );

        let second = bind(&socket, uid, "test service", Directory::Shared(0o700))
            .await
            .err()
            .unwrap();
        assert_eq!(second.code, "busy");

        drop(endpoint);
        assert!(!socket.exists(), "the endpoint removes its own socket");
    }

    /// A socket nothing listens on is replaced; anything that is not a
    /// socket is refused and left alone.
    #[tokio::test]
    async fn a_stale_socket_is_replaced_and_a_non_socket_refused() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("run/s.sock");
        let uid = nix::unistd::getuid().as_raw();

        fs::create_dir(socket.parent().unwrap()).unwrap();
        fs::set_permissions(socket.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        let stale = fs::symlink_metadata(&socket).unwrap().ino();
        let (_listener, endpoint) = bind(&socket, uid, "test service", Directory::Shared(0o700))
            .await
            .unwrap();

        assert_ne!(fs::symlink_metadata(&socket).unwrap().ino(), stale);
        UnixStream::connect(&socket).await.unwrap();

        drop((_listener, endpoint));
        fs::write(&socket, b"not a socket").unwrap();

        let refused = bind(&socket, uid, "test service", Directory::Shared(0o700))
            .await
            .err()
            .unwrap();
        assert_eq!(refused.code, "identity");
        assert_eq!(fs::read(&socket).unwrap(), b"not a socket");
    }

    #[tokio::test]
    async fn a_turned_away_client_reads_why() {
        use tokio::io::AsyncReadExt;

        let (mut client, server) = UnixStream::pair().unwrap();
        turn_away(server, &serde_json::json!({"ok": false, "code": "busy"}));

        let mut reply = String::new();
        client.read_to_string(&mut reply).await.unwrap();
        let line = reply.strip_suffix('\n').unwrap();
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value, serde_json::json!({"ok": false, "code": "busy"}));
    }

    #[tokio::test]
    async fn a_directory_whose_acl_grants_writes_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        fs::create_dir(&run).unwrap();
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();

        let user = std::env::var("USER").unwrap();
        let granted = std::process::Command::new("/bin/chmod")
            .args(["+a", &format!("user:{user} allow add_file")])
            .arg(&run)
            .status()
            .unwrap();
        assert!(granted.success());

        let uid = nix::unistd::getuid().as_raw();
        let refused = bind(
            &run.join("s.sock"),
            uid,
            "test service",
            Directory::Shared(0o700),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(refused.code, "identity");
    }

    /// A private directory is recreated with the controller's search grant,
    /// and one that is open by mode or carries other grants is refused.
    #[tokio::test]
    async fn a_private_directory_keeps_setups_policy() {
        let dir = tempfile::tempdir().unwrap();
        let uid = nix::unistd::getuid().as_raw();
        let private = Directory::Private { controller: uid };
        let run = dir.path().join("helper");
        let (listener, endpoint) = bind(&run.join("s.sock"), uid, "test", private)
            .await
            .unwrap();

        assert_eq!(fs::metadata(&run).unwrap().mode() & 0o777, 0o700);
        let lines = crate::setup::helper::acl_lines(&run).unwrap();
        assert_eq!(
            crate::setup::helper::acl_policy(&lines, Some(uid)),
            Ok(true)
        );

        drop((listener, endpoint));
        fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).unwrap();
        let open = bind(&run.join("s.sock"), uid, "test", private).await;
        assert_eq!(open.err().unwrap().code, "identity");

        fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();
        let user = std::env::var("USER").unwrap();
        std::process::Command::new("/bin/chmod")
            .args(["+a", &format!("user:{user} allow list")])
            .arg(&run)
            .status()
            .unwrap();

        let extra = bind(&run.join("s.sock"), uid, "test", private).await;
        assert_eq!(extra.err().unwrap().code, "identity");
    }
}
