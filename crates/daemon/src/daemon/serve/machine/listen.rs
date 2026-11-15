//! The guest's listener: each connection the host makes to its port, which
//! says first, in one byte, what it is for.
use super::{Accepting, Framing, Listener};
use atlas_session::{secure::Purpose, station::Raw};
use std::{io, os::fd::OwnedFd, time::Duration};
use tokio::{io::AsyncReadExt, io::unix::AsyncFd};

/// How long to wait before taking connections again when this process is
/// out of descriptors or memory, rather than spinning.
const SHORT: Duration = Duration::from_millis(100);

/// Connections made to a listening socket.
pub(super) struct Guest(AsyncFd<OwnedFd>);

impl Guest {
    /// Listen on vsock `port`, as a guest does for its host.
    ///
    /// # Errors
    ///
    /// Fails outside a VM, or with the port taken.
    pub(super) fn on(port: u32) -> io::Result<Self> {
        Self::of(atlas_core::vsock::listen_on(port)?)
    }

    /// Take connections from `listening`, a non-blocking listening socket
    /// of any stream family.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be watched.
    pub(super) fn of(listening: OwnedFd) -> io::Result<Self> {
        Ok(Self(AsyncFd::new(listening)?))
    }
}

impl Listener for Guest {
    fn accept(&self) -> Accepting<'_> {
        Box::pin(async {
            loop {
                let mut ready = self.0.readable().await?;
                let accepted = match ready
                    .try_io(|listening| atlas_core::vsock::accept(listening.get_ref()))
                {
                    Ok(accepted) => accepted,
                    Err(_would_block) => continue,
                };
                match accepted {
                    Ok(connection) => {
                        let stream = std::os::unix::net::UnixStream::from(connection);
                        let stream = tokio::net::UnixStream::from_std(stream)?;
                        return Ok(Box::pin(framed(stream)) as Framing);
                    }
                    // One connection that went before it was taken.
                    Err(error)
                        if matches!(
                            error.raw_os_error(),
                            Some(libc::ECONNABORTED | libc::EINTR)
                        ) => {}
                    // Out of room for now: it comes back once some is freed.
                    Err(error)
                        if matches!(
                            error.raw_os_error(),
                            Some(libc::EMFILE | libc::ENFILE | libc::ENOBUFS | libc::ENOMEM)
                        ) =>
                    {
                        tokio::time::sleep(SHORT).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        })
    }
}

/// What `stream` says it is for, then the stream.
async fn framed(mut stream: tokio::net::UnixStream) -> io::Result<(Purpose, Raw)> {
    let byte = stream.read_u8().await?;
    let purpose = Purpose::from_byte(byte)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "an unknown purpose"))?;
    Ok((purpose, Box::new(stream) as Raw))
}
