//! The controller's side of the helper: one [`Client`] per configuration
//! that talks to the helper daemon and the input service over their
//! sockets, proves the helper is the hidden session it expects before every
//! action, holds controller exclusion while acting, and keeps the
//! screenshots and activity log in the private output folder.
mod apps;
mod capture;
mod input;
mod install;
mod mutate;
mod record;
mod run;
mod service;
mod service_change;
mod session;
mod stop;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use capture::prune_captures;
pub use capture::{Frame, SCREENSHOT_SIDE, Size, png_dimensions};
pub use input::Check;
pub use mutate::validate_completion;
pub(crate) use record::PersonInput;
pub(crate) use run::Run;
pub use run::{RUN_TIMEOUT, RUN_TIMEOUT_MAX};

use crate::{
    config::Config,
    control::Controller,
    daemon::protocol::{SESSION_PIN, SessionPin},
    doing::Doing,
    error::{Code, Error, Result},
    host_session::HostSession,
    native::windows::HitWindow,
    session::{Session, has_capability, require_hidden},
    task::Task,
    transport::{Transport, run_output},
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Identity (device, inode, size, mtime) of the installed executable and its hash.
type BuildCache = Arc<std::sync::Mutex<Option<((u64, u64, u64, i64, i64), String)>>>;

/// A helper session a command has just verified, and whether its daemon
/// refuses a read meant for another session. Only [`Client::verify`] makes
/// one, so a read given it was checked in that session.
#[derive(Debug, Clone)]
pub struct Verified {
    session: Session,
    pinned: bool,
}

impl Verified {
    /// The session as it was verified.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// What a read carries to be served only in this session; none when the
    /// daemon cannot be told, and the session is verified again instead.
    pub(crate) fn pin(&self) -> Option<SessionPin> {
        self.pinned.then_some(SessionPin {
            security_session_id: self.session.epoch.security_session_id,
            display_id: self.session.epoch.display_id,
        })
    }
}

/// Who recorded mutations are attributed to in the activity log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Agent,
    Viewer,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Agent => "agent",
            Actor::Viewer => "viewer",
        }
    }
}

#[derive(Clone)]
pub struct Client {
    pub config: Config,
    pub transport: Transport,
    /// A local stand-in's socket peer, separate from its reported helper UID.
    #[cfg(test)]
    pub(crate) input_peer_uid: Option<u32>,
    pub controller: Controller,
    pub actor: Actor,
    /// What the command acting through this client does, for the log.
    pub doing: Option<Doing>,
    /// The host session the command acting through this client comes from,
    /// for activity attribution and to exclude its own recent activity
    /// when checking whether a service replacement would interrupt others.
    pub host_session: Option<HostSession>,
    /// Whether the person permits service replacement while sessions or
    /// background jobs are active. Controller exclusion still applies.
    pub allow_disrupt: bool,
    /// The task of that session the command belongs to, for its rows and
    /// the task's record; never for a decision.
    pub task: Option<Task>,
    /// Where the person's input through a viewer gathers before it is
    /// logged; none for every other client, whose rows are written at once.
    pub(crate) person_input: Option<record::PersonInput>,
    installed: BuildCache,
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl Client {
    pub(crate) fn remember_windows(&self, windows: &[HitWindow]) -> Result<()> {
        if let Some(session) = &self.host_session
            && let Ok(output) = self.output_dir()
        {
            crate::last_capture::remember_windows(output, session.key().as_str(), windows)?;
        }
        Ok(())
    }

    pub(crate) fn remembered_windows(&self) -> Option<Vec<HitWindow>> {
        crate::last_capture::remembered_windows(
            &self.config.output_dir,
            self.host_session.as_ref()?.key().as_str(),
        )
    }

    /// Construction only validates; reads such as status never write; doctor
    /// only probes that Atlas's own folders can be written, making them when
    /// missing.
    pub fn new(config: Config) -> Result<Self> {
        crate::session::validate_uid(config.helper_uid)?;
        Ok(Self {
            controller: Controller::new(config.helper_uid)?,
            transport: Transport {
                path: config.socket_path.clone(),
                timeout: Duration::from_secs(30),
                peer_uid: config.helper_uid,
            },
            #[cfg(test)]
            input_peer_uid: None,
            installed: BuildCache::default(),
            actor: Actor::Agent,
            doing: None,
            host_session: None,
            allow_disrupt: false,
            task: None,
            person_input: None,
            config,
        })
    }

    /// This client, recording what its command does in each activity row.
    pub fn doing(mut self, doing: Doing) -> Self {
        self.doing = Some(doing);
        self
    }

    /// This client, keeping no control's name in what it records.
    pub(crate) fn unlabelled(mut self) -> Self {
        self.doing = self.doing.map(Doing::unlabelled);
        self
    }

    /// The private output directory, created 0700 (with any missing parents)
    /// on first use. An existing directory must already be private; it is
    /// never re-permissioned. The umask can only remove mode bits.
    pub fn output_dir(&self) -> Result<&Path> {
        let path = &self.config.output_dir;
        if !path.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(|error| Error::writing(path, error))?;
        }

        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(Error::new(
                Code::Config,
                format!(
                    "Output directory {} must be a private (0700) directory you own",
                    path.display()
                ),
            ));
        }

        Ok(path)
    }

    pub async fn status(&self) -> Value {
        self.verified_status().await.unwrap_or_else(|e| json!(e))
    }

    async fn verified_status(&self) -> Result<Value> {
        let mut hello = self.transport.call(&json!({"op": "hello"})).await?;

        // Do not launch the probe into an unverified UID or the visible desktop.
        if hello["ok"] != true
            || hello["user"]["uid"].as_u64() != Some(self.config.helper_uid.into())
            || hello["session"]["onConsole"] != false
        {
            return Ok(hello);
        }

        hello["inputService"] = self
            .input_transport()
            .call(&json!({ "op": "hello" }))
            .await
            .unwrap_or_else(|error| json!(error));
        hello["installedBuild"] = json!(self.installed_build());
        hello["sessionProof"] = self
            .session_proof()
            .await
            .unwrap_or_else(|error| json!(error));

        Ok(hello)
    }

    async fn session_proof(&self) -> Result<Value> {
        let request = json!({
            "op": "run",
            "argv": [self.config.identity_probe, "session-info"],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true || reply["exitCode"] != 0 || reply["timedOut"] != false {
            return Err(Error::new(
                Code::Identity,
                format!(
                    "The helper cannot run the shared Atlas command at {}. Run atlas \
                     install.",
                    self.config.identity_probe.display()
                ),
            ));
        }

        let output = run_output(&reply, 4096)
            .map_err(|_| Error::new(Code::Identity, "Malformed or oversized identity output"))?;

        Ok(serde_json::from_slice(&output)?)
    }

    pub async fn identity(&self) -> Result<Session> {
        require_hidden(&self.status().await, self.config.helper_uid)
    }

    /// The helper session as the daemon reports it now, and whether that
    /// daemon refuses a read meant for another session: what reads in this
    /// session are checked by. It verifies the helper as
    /// [`Client::identity`] does.
    pub async fn verify(&self) -> Result<Verified> {
        let status = self.status().await;
        let session = require_hidden(&status, self.config.helper_uid)?;
        let pinned = has_capability(&status["daemon"], SESSION_PIN);
        Ok(Verified { session, pinned })
    }

    /// [`Client::verify`], refusing a helper daemon that does not offer
    /// `capability` with `daemon` and `older`, which says what it cannot do
    /// and how to bring it up to date. Nothing is sent to an older daemon.
    pub(crate) async fn verify_offering(
        &self,
        capability: &str,
        older: &'static str,
    ) -> Result<Verified> {
        let status = self.status().await;
        let session = require_hidden(&status, self.config.helper_uid)?;
        if !has_capability(&status["daemon"], capability) {
            return Err(Error::new(Code::Daemon, older));
        }
        let pinned = has_capability(&status["daemon"], SESSION_PIN);
        Ok(Verified { session, pinned })
    }

    /// Whether this shell may keep Atlas's own files: the controller lock
    /// and the screenshots. An agent's sandbox may forbid both while the
    /// helper itself is fine.
    pub fn writable(&self) -> Result<()> {
        self.controller.writable()?;
        crate::files::probe_write(self.output_dir()?)?;

        // Builds and environment plans are staged beside the shared Atlas
        // command for the helper to read; a missing folder is setup's
        // concern, not this check's.
        if let Some(shared) = self.config.identity_probe.parent()
            && shared.is_dir()
        {
            crate::files::probe_write(shared)?;
        }

        Ok(())
    }
}
