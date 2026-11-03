//! The controller's side of the helper: one [`Client`] per configuration
//! that talks to the helper daemon and the input service over their
//! sockets, proves the helper is the hidden session it expects before every
//! action, holds controller exclusion while acting, and keeps the
//! screenshots and activity log in the private output folder.
mod apps;
mod capture;
mod input;
mod install;
pub mod live;
mod mutate;
mod record;
mod run;
pub mod service;
mod service_change;
mod session;
mod stop;
#[cfg(test)]
mod tests;
mod windows;

pub use apps::{AppList, AppRow};
#[cfg(test)]
pub(crate) use capture::prune_captures;
pub(crate) use mutate::{Mutation, unconfirmed};
pub(crate) use record::NOT_RECORDED;
pub use record::PersonInput;
pub(crate) use run::{Finished, Run};
pub use run::{RUN_TIMEOUT, RUN_TIMEOUT_MAX};
pub(crate) use windows::WINDOW_LOOKUP;

use crate::{doing::Doing, host_session::HostSession, task::Task};
use atlas_control::Controller;
use atlas_core::{
    config::Config,
    error::{Code, Error, Result},
    protocol::{SESSION_PIN, SESSION_PROOF, SessionPin},
    session::{Observation, Session},
    transport::{Transport, run_output},
};
pub(crate) use atlas_core::{screen::frame::Size, time::timestamp};
use serde_json::{Value, json};

use std::{path::Path, sync::Arc, time::Duration};

/// Identity (device, inode, size, mtime) of the installed executable and its hash.
type BuildCache = Arc<std::sync::Mutex<Option<((u64, u64, u64, i64, i64), String)>>>;

/// A helper session a command has just verified, and whether its daemon
/// refuses a read meant for another session. Only [`Client::verify`] makes
/// one, so a read given it was checked in that session.
#[derive(Debug, Clone)]
pub(crate) struct Verified {
    session: Session,
    observation: Observation,
}

impl Verified {
    /// The session as it was verified.
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    /// What a read carries to be served only in this session. A daemon that
    /// cannot be told is refused, so no read goes unpinned.
    pub(crate) fn pin(&self) -> Result<SessionPin> {
        if !self.observation.has_capability(SESSION_PIN) {
            return Err(Error::new(
                Code::Daemon,
                "The helper is running an older Atlas that cannot hold a read to its session; run \
                 atlas restart, then try again",
            ));
        }
        Ok(SessionPin {
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
    pub(crate) fn as_str(self) -> &'static str {
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
    #[cfg(any(test, feature = "test-support"))]
    pub input_peer_uid: Option<u32>,
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
    pub person_input: Option<record::PersonInput>,
    installed: BuildCache,
}

impl Client {
    /// Construction only validates; reads such as status never write; doctor
    /// only probes that Atlas's own folders can be written, making them when
    /// missing.
    ///
    /// # Errors
    ///
    /// Rejects an invalid or non-distinct helper UID and failures creating the local controller
    /// identity. Construction does not verify a running helper session.
    pub fn new(config: Config) -> Result<Self> {
        atlas_core::session::validate_uid(config.helper_uid)?;
        Ok(Self {
            controller: Controller::new(config.helper_uid)?,
            transport: Transport {
                path: config.socket_path.clone(),
                timeout: Duration::from_secs(30),
                peer_uid: config.helper_uid,
            },
            #[cfg(any(test, feature = "test-support"))]
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
    #[cfg(any(test, feature = "test-support"))]
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
    ///
    /// # Errors
    ///
    /// Rejects an existing non-private, foreign-owned or non-directory destination, and returns
    /// directory-creation or inspection failures.
    pub fn output_dir(&self) -> Result<&Path> {
        atlas_core::files::private_folder(&self.config.output_dir, Code::Config)?;
        Ok(&self.config.output_dir)
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

        // Independent reads after the daemon's UID and off-console gate, joined
        // before returning: every identity still needs a new input report and
        // a new challenged proof, whose epochs require_hidden compares.
        // Neither result authorizes anything on its own.
        let input = self.input_transport();
        let request = json!({ "op": "hello" });
        let (input, proof) =
            tokio::join!(input.call(&request), self.session_proof(&hello["daemon"]));
        hello["inputService"] = input.unwrap_or_else(|error| json!(error));
        hello["installedBuild"] = json!(self.installed_build());
        hello["sessionProof"] = proof.unwrap_or_else(|error| json!(error));

        Ok(hello)
    }

    /// A fresh proof of the helper's session from a process of its own: the
    /// daemon's persistent proof worker when it has one, else a new run of
    /// the shared Atlas command.
    async fn session_proof(&self, daemon: &Value) -> Result<Value> {
        if atlas_core::session::has_capability(daemon, SESSION_PROOF) {
            let challenge = atlas_core::files::token();
            let reply = self
                .transport
                .call(&json!({"op": "session-proof", "challenge": challenge}))
                .await?;
            return Self::challenged_proof(&reply, &challenge);
        }
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

    /// The proof in `reply`, only when it answers `challenge`.
    fn challenged_proof(reply: &Value, challenge: &str) -> Result<Value> {
        if reply["ok"] != true
            || reply["challenge"].as_str() != Some(challenge)
            || !reply["proof"].is_object()
        {
            return Err(Error::new(
                Code::Identity,
                "The independent session proof did not match its challenge",
            ));
        }
        Ok(reply["proof"].clone())
    }

    /// What the helper's status says now.
    async fn observed(&self) -> Observation {
        Observation::decode(&self.status().await)
    }

    /// # Errors
    ///
    /// Returns the failure to reach the helper or to prove it is the configured
    /// account.
    pub async fn identity(&self) -> Result<Session> {
        self.verify().await.map(|verified| verified.session)
    }

    /// The helper session as the daemon reports it now, and whether that
    /// daemon refuses a read meant for another session: what reads in this
    /// session are checked by. It verifies the helper as
    /// [`Client::identity`] does.
    pub(crate) async fn verify(&self) -> Result<Verified> {
        let observation = self.observed().await;
        let session = observation.require_hidden(self.config.helper_uid)?;
        Ok(Verified {
            session,
            observation,
        })
    }

    /// [`Client::verify`], refusing a helper daemon that does not offer
    /// `capability` with `daemon` and `older`, which says what it cannot do
    /// and how to bring it up to date. Nothing is sent to an older daemon.
    pub(crate) async fn verify_offering(
        &self,
        capability: &str,
        older: &'static str,
    ) -> Result<Verified> {
        let verified = self.verify().await?;
        if !verified.observation.has_capability(capability) {
            return Err(Error::new(Code::Daemon, older));
        }
        Ok(verified)
    }

    /// Whether this shell may keep Atlas's own files: the controller lock
    /// and the screenshots. An agent's sandbox may forbid both while the
    /// helper itself is fine.
    ///
    /// # Errors
    ///
    /// Returns failures creating or checking Atlas-owned private output and control storage.
    /// Existing unsafe storage is refused rather than silently made trustworthy.
    pub fn writable(&self) -> Result<()> {
        self.controller.writable()?;
        atlas_core::files::probe_write(self.output_dir()?)?;

        // Builds and environment plans are staged beside the shared Atlas
        // command for the helper to read; a missing folder is setup's
        // concern, not this check's.
        if let Some(shared) = self.config.identity_probe.parent()
            && shared.is_dir()
        {
            atlas_core::files::probe_write(shared)?;
        }

        Ok(())
    }
}
