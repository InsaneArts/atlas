//! The user desktop's services as this Mac sees them: its daemon's socket
//! and folder on this disk, and its input service: launched and waited for,
//! gone, or keeping a Stop saved.
use crate::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    session::Observation,
    transport::run_output,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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

    /// Launch the shared Atlas command, installed on this disk, as the input
    /// service of the second login `login`, for this account: its process
    /// ID, before it is known to be ready.
    async fn launch_input_in(&self, login: &atlas_core::user::User) -> Result<u32> {
        // Arguments stay separate from shell syntax, including configured paths.
        let script = concat!(
            "nohup \"$1\" input-service --uid \"$2\" --controller-uid \"$3\" --socket \"$4\" ",
            ">\"$HOME/Library/Logs/Atlas-input.log\" 2>&1 </dev/null & echo $!",
        );
        let controller_uid = nix::unistd::getuid().as_raw();
        let request = json!({
            "op": "run",
            "argv": [
                "/bin/sh", "-c", script, "sh",
                self.config.identity_probe,
                login.uid.to_string(),
                controller_uid.to_string(),
                self.config.input_socket,
            ],
            "timeoutMs": 3000,
        });
        launched_pid(&self.transport.call(&request).await?)
    }

    /// [`Client::spawn_input`] in the second login `login`: launched, then
    /// waited for until it is ready.
    pub(crate) async fn spawn_input_in(&self, login: &atlas_core::user::User) -> Result<u32> {
        let child = self.launch_input_in(login).await?;
        let deadline = Instant::now() + Duration::from_secs(6);

        loop {
            let state = self.status().await;
            if self
                .desktop()
                .prove(&Observation::decode(&state))
                .is_ok_and(|session| session.input && session.epoch.input_pid == child)
            {
                return Ok(child);
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    Code::Startup,
                    "Input service did not become ready. Check ~/Library/Logs/Atlas-input.log \
                     in the helper account and confirm that identity_probe points to the current Rust binary.",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// The process a launch printed, the only one that may satisfy readiness.
/// Any other answer may still have started something.
fn launched_pid(reply: &Value) -> Result<u32> {
    let failed = || {
        Error::new(
            Code::Startup,
            "The helper could not launch the Rust input service",
        )
        .uncertain()
    };
    if reply["ok"] != true || reply["exitCode"] != 0 {
        return Err(failed());
    }

    let output = run_output(reply, 32).map_err(|cause| failed().with_source(cause))?;
    let text = String::from_utf8(output).map_err(|cause| failed().with_source(cause))?;
    let pid = text
        .trim()
        .parse::<u32>()
        .map_err(|cause| failed().with_source(cause))?;
    if pid > 1 { Ok(pid) } else { Err(failed()) }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
