//! The user desktop's services as this Mac sees them: its daemon's socket
//! and folder on this disk, and whether its input service has gone or keeps
//! a Stop saved.
use crate::client::Client;

/// Whether a process exists, whoever owns it (EPERM still means it exists).
pub(crate) fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };

    matches!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Ok(()) | Err(nix::errno::Errno::EPERM)
    )
}

/// [`daemon_listens`](crate::jobs::daemon_listens) for the second login
/// `login`: whether its socket is on this disk.
pub(crate) fn daemon_listens_in(client: &Client, _login: &atlas_core::user::User) -> bool {
    client.transport.path.exists()
}

impl Client {
    /// Whether the second login `login` has never been set up. Its daemon
    /// creates its directory before accepting jobs and keeps it across
    /// shutdowns. Only an absent directory establishes a first setup; a
    /// missing socket can leave background jobs running.
    pub(crate) fn first_setup_in(&self, _login: &atlas_core::user::User) -> bool {
        self.transport.path.parent().is_some_and(|directory| {
            std::fs::symlink_metadata(directory)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        })
    }

    /// Whether the input service `pid` of the second login `login` is gone:
    /// nothing listens on its socket and its process has ended, both as this
    /// Mac sees them.
    pub(crate) async fn input_exited_in(&self, _login: &atlas_core::user::User, pid: u32) -> bool {
        matches!(
            tokio::net::UnixStream::connect(&self.config.input_socket).await,
            Err(error) if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            )
        ) && !process_exists(pid)
    }

    /// Whether the input service of the second login `login` has a Stop
    /// saved: a file beside its socket on this disk.
    pub(crate) fn service_stop_saved_in(
        &self,
        _login: &atlas_core::user::User,
    ) -> std::io::Result<bool> {
        self.config
            .input_socket
            .with_extension("stopped")
            .try_exists()
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
