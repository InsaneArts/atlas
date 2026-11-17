//! The controller's side of the helper: one [`Client`] per configuration
//! that talks to the helper daemon and the input service over their
//! sockets, proves the helper is the hidden session it expects before every
//! action, holds controller exclusion while acting, and keeps the
//! screenshots and activity log in the private output folder.
mod apps;
mod capture;
mod guest_input;
mod input;
mod install;
mod keeper;
pub mod live;
mod mutate;
mod picture;
mod record;
mod run;
pub mod service;
mod service_change;
pub(crate) mod session;
mod stage;
mod stop;
#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
mod windows;

pub use apps::{AppList, AppRow};
#[cfg(test)]
pub(crate) use capture::prune_captures;
pub(crate) use input::require_looks;
pub use keeper::Leftovers;
pub(crate) use mutate::{Mutation, unconfirmed};
pub use record::PersonInput;
pub(crate) use record::{NOT_RECORDED, logged_at};
pub(crate) use run::{Finished, Run};
pub use run::{RUN_TIMEOUT, RUN_TIMEOUT_MAX};
pub(crate) use windows::{Seen, WINDOW_LOOKUP};

use crate::{doing::Doing, host_session::HostSession, link::Link, task::Task};
use atlas_control::Controller;
use atlas_core::{
    config::{
        Config,
        desktops::{Admission, Chosen, Selected},
    },
    desktop::{Desktop, Kind},
    error::{Code, Error, Result},
    protocol::{SESSION_PIN, SESSION_PROOF, SessionPin},
    session::{Observation, Session},
    transport::Transport,
};
pub(crate) use atlas_core::{screen::frame::Size, time::timestamp};
use serde_json::{Value, json};

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// What identifies an installed build without hashing it: its device,
/// inode, size and modification time.
pub(crate) type BuildKey = (u64, u64, u64, i64, i64);
/// Identity (device, inode, size, mtime) of the installed executable and its hash.
type BuildCache = Arc<std::sync::Mutex<Option<(BuildKey, String)>>>;

#[cfg(test)]
type AskShown = Arc<dyn Fn(&str) + Send + Sync>;

/// A helper session a command has just verified, and whether its daemon
/// refuses a read meant for another session. Only [`Client::verify`] makes
/// one, so a read given it was checked in that session.
#[derive(Debug, Clone)]
pub(crate) struct Verified {
    session: Session,
    observation: Observation,
    /// A VM's session as its proof pinned it; none for Second Login.
    machine: Option<Pinned>,
}

/// A VM's session as one command proved it: its epoch, and the generation of
/// its keeper's session the proof was taken in, which every change the
/// command makes carries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pinned {
    pub(crate) epoch: atlas_core::session::MachineEpoch,
    pub(crate) generation: u64,
    /// Why its guest takes no input from agents in this session, with the
    /// code it is refused by; none while it does.
    pub(crate) input_held: Option<(Code, String)>,
}

impl Verified {
    /// The session as it was verified.
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    /// A VM's session as its proof pinned it; none for Second Login.
    pub(crate) fn machine(&self) -> Option<&Pinned> {
        self.machine.as_ref()
    }

    /// Whether the input service that answered this verification offers
    /// `capability`.
    pub(crate) fn input_offers(&self, capability: &str) -> bool {
        self.observation.native.has_capability(capability)
    }

    /// What a read carries to be served only in this session. A daemon that
    /// cannot be told is refused, so no read goes unpinned.
    pub(crate) fn pin(&self) -> Result<SessionPin> {
        // A VM's guest holds every read and change to the session its proof
        // names.
        if let Some(machine) = &self.machine {
            return Ok(SessionPin {
                security_session_id: machine.epoch.security_session_id,
                display_id: machine.epoch.display_id,
            });
        }
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
    /// The desktop this client acts on, as the settings chose it.
    pub selected: Selected,
    /// How requests reach the desktop.
    pub link: Link,
    /// A local stand-in's socket peer, separate from its reported helper UID.
    #[cfg(any(test, feature = "test-support"))]
    pub input_peer_uid: Option<u32>,
    /// A test's stand-in for the person's account list; none never reads
    /// theirs.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) listed: Option<crate::sign_ins::Listed>,
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
    /// A VM desktop's records folder beneath the output folder; none for
    /// Second Login, whose records are the output folder itself.
    own_records: Option<PathBuf>,
    /// Where a VM desktop's lifecycle is kept, which says whether it is
    /// still being added; none for Second Login.
    pub(crate) lifecycle: Option<crate::vm::lifecycle::Lifecycle>,
    /// What the person allowed this command once, for its next attempt only.
    granted: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    /// Whether this command's next input is only its check before typing:
    /// judged, nothing sent.
    checking: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The digest of the whole text this command types, which its check
    /// and each of its requests carry.
    typing: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// The picture of the screen a refusal to ask the person came with, for
    /// the person's question alone, and the screen's privacy when it was
    /// taken: it goes up only while that is unchanged.
    for_ask: std::sync::Arc<std::sync::Mutex<Option<(crate::before::Raw, atlas_control::Visible)>>>,
    /// Observe a published question while its preview is still shown, before waiting.
    #[cfg(test)]
    pub(crate) ask_shown: Option<AskShown>,
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
    pub fn new(selected: Selected) -> Result<Self> {
        let desktop = selected.desktop();
        let timeout = Duration::from_secs(30);
        // The command service: where it listens, and the account the
        // kernel must name at its end.
        let (link, own_records, lifecycle) = match &selected.desktop {
            Chosen::User(config) => {
                let login = config.user();
                atlas_core::session::validate_uid(login.uid)?;
                let link = Link::Unix(Transport {
                    path: config.socket_path.clone(),
                    timeout,
                    peer_uid: login.uid,
                });
                (link, None, None)
            }
            Chosen::Vm(config) => {
                let link = crate::link::probe(&selected, &config.id, timeout);
                let lifecycle = crate::vm::lifecycle::Lifecycle::own().ok();
                (link, Some(selected.desktop.records()), lifecycle)
            }
        };
        let client = Self {
            controller: Controller::new(&desktop)?,
            link,
            #[cfg(any(test, feature = "test-support"))]
            input_peer_uid: None,
            #[cfg(any(test, feature = "test-support"))]
            listed: None,
            installed: BuildCache::default(),
            actor: Actor::Agent,
            doing: None,
            host_session: None,
            allow_disrupt: false,
            task: None,
            person_input: None,
            granted: std::sync::Arc::default(),
            checking: std::sync::Arc::default(),
            typing: std::sync::Arc::default(),
            for_ask: std::sync::Arc::default(),
            #[cfg(test)]
            ask_shown: None,
            own_records,
            lifecycle,
            selected,
        };
        client.admission();
        Ok(client)
    }

    /// The desktop this client acts on, as its settings name it.
    pub fn desktop(&self) -> Desktop {
        self.selected.desktop()
    }

    /// The helper's settings, for what only a Second Login does.
    ///
    /// # Errors
    ///
    /// Refuses a VM desktop with the one wording for what this build cannot
    /// do on it.
    pub fn user_config(&self) -> Result<&Config> {
        self.selected
            .desktop
            .user()
            .ok_or_else(|| self.unreachable())
    }

    /// The helper's settings, for a test of a Second Login.
    ///
    /// # Panics
    ///
    /// On a VM desktop.
    #[cfg(any(test, feature = "test-support"))]
    pub fn helper_config(&self) -> &Config {
        self.selected
            .desktop
            .user()
            .expect("a test of a Second Login")
    }

    /// The helper's settings, for a test that changes them.
    ///
    /// # Panics
    ///
    /// On a VM desktop.
    #[cfg(any(test, feature = "test-support"))]
    pub fn helper_config_mut(&mut self) -> &mut Config {
        match &mut self.selected.desktop {
            Chosen::User(config) => config,
            Chosen::Vm(_) => panic!("a test of a Second Login"),
        }
    }

    /// Refuse a VM desktop before anything is read or sent, unless it is
    /// added and its keeper is reached: each command on it is then admitted
    /// on its own checks.
    ///
    /// # Errors
    ///
    /// Returns [`Client::unreachable`] for a VM desktop still being added,
    /// or whose keeper this client does not reach.
    pub fn reachable(&self) -> Result<()> {
        match self.desktop().kind() {
            Kind::User(_) => Ok(()),
            Kind::Vm(_)
                if self.admission() == Admission::Open && matches!(self.link, Link::Keeper(_)) =>
            {
                Ok(())
            }
            Kind::Vm(_) => Err(self.unreachable()),
        }
    }

    /// Refuse a VM desktop before anything is read or sent, for what this
    /// build does on a Second Login alone.
    ///
    /// # Errors
    ///
    /// Returns [`Client::unreachable`] for a VM desktop.
    pub fn second_login_only(&self) -> Result<()> {
        match self.desktop().kind() {
            Kind::User(_) => Ok(()),
            Kind::Vm(_) => Err(self.unreachable()),
        }
    }

    /// The one refusal of what this build cannot do on a VM desktop, named
    /// for the desktop this client acts on, its add looked at again first.
    pub fn unreachable(&self) -> Error {
        self.admission();
        self.selected.unreachable()
    }

    /// Where this VM's add stands, its lifecycle record looked at again
    /// when its file changed since: a look at the file, and a small read
    /// only then, one caller at a time. Every copy of the selection, the
    /// keeper's link's among them, sees what it finds. A record that cannot
    /// be read leaves it uncertain, never added.
    pub(crate) fn admission(&self) -> Admission {
        use std::os::unix::fs::MetadataExt;
        let Chosen::Vm(config) = &self.selected.desktop else {
            return Admission::Open;
        };
        let adding = &self.selected.adding;
        let Some(lifecycle) = &self.lifecycle else {
            adding.doubt();
            return adding.get();
        };
        let look = || match lifecycle.state_of(&config.id) {
            Ok(Some(crate::vm::lifecycle::record::State::Adding)) => Admission::Adding,
            Ok(_) => Admission::Open,
            Err(_) => Admission::Uncertain,
        };
        match std::fs::symlink_metadata(lifecycle.record_at(&config.id)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                adding.renew(None, || Admission::Open);
            }
            Err(_) => adding.doubt(),
            Ok(file) => adding.renew(
                Some((file.ino(), file.len(), file.ctime(), file.ctime_nsec())),
                look,
            ),
        }
        adding.get()
    }

    /// What the person allowed this command once, for its next attempt.
    pub(crate) fn granted(&self) -> Vec<Value> {
        self.granted
            .lock()
            .map(|granted| granted.clone())
            .unwrap_or_default()
    }

    /// Keep `grant` for this command's next attempt.
    pub(crate) fn grant(&self, grant: Value) {
        if let Ok(mut granted) = self.granted.lock() {
            granted.push(grant);
        }
    }

    /// Whether this command's next input is only its check.
    pub(crate) fn checking(&self) -> bool {
        self.checking.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Make this command's next inputs only its check, or real again.
    pub(crate) fn set_checking(&self, on: bool) {
        self.checking.store(on, std::sync::atomic::Ordering::SeqCst);
    }

    /// The digest of the whole text this command types, while it types.
    pub(crate) fn typing(&self) -> Option<String> {
        self.typing.lock().ok().and_then(|typing| typing.clone())
    }

    /// Say this command types the text `digest` names, or no longer types.
    pub(crate) fn set_typing(&self, digest: Option<String>) {
        if let Ok(mut typing) = self.typing.lock() {
            *typing = digest;
        }
    }

    /// Keep `raw`, taken while the screen's privacy was `admitted`, for the
    /// question the refusal just come back asks.
    pub(crate) fn keep_for_ask(&self, raw: crate::before::Raw, admitted: atlas_control::Visible) {
        if let Ok(mut kept) = self.for_ask.lock() {
            *kept = Some((raw, admitted));
        }
    }

    /// The picture kept for the question asked now, taken: none kept
    /// stays.
    pub(crate) fn take_for_ask(&self) -> Option<(crate::before::Raw, atlas_control::Visible)> {
        self.for_ask.lock().ok()?.take()
    }

    /// Whether a picture is kept for the question asked now.
    pub(crate) fn kept_for_ask(&self) -> bool {
        self.for_ask.lock().is_ok_and(|kept| kept.is_some())
    }

    /// Spend what was allowed once: an attempt used it.
    pub(crate) fn clear_granted(&self) {
        if let Ok(mut granted) = self.granted.lock() {
            granted.clear();
        }
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

    /// This desktop's private records folder (the output directory itself
    /// for Second Login, a folder of its own beneath it for a VM), created
    /// 0700 (with any missing parents) on first use. An existing directory must already be private; it is
    /// never re-permissioned. The umask can only remove mode bits.
    ///
    /// # Errors
    ///
    /// Rejects an existing non-private, foreign-owned or non-directory destination, and returns
    /// directory-creation or inspection failures.
    pub fn output_dir(&self) -> Result<&Path> {
        let records = self.records();
        atlas_core::files::private_folder(records, Code::Config)?;
        Ok(records)
    }

    /// [`Client::output_dir`], neither made nor checked.
    pub fn records(&self) -> &Path {
        self.own_records
            .as_deref()
            .unwrap_or_else(|| self.selected.desktop.output_dir())
    }

    /// What the activity keeps.
    pub(crate) fn activity(&self) -> &atlas_core::config::Activity {
        self.selected.desktop.activity()
    }

    /// What the desktop says of itself now, or why it could not be asked.
    pub async fn status(&self) -> Value {
        self.verified_status().await.unwrap_or_else(|e| json!(e))
    }

    async fn verified_status(&self) -> Result<Value> {
        if let Kind::Vm(_) = self.desktop().kind() {
            return self.vm_status().await;
        }
        self.reachable()?;
        let mut hello = self.link.call(&json!({"op": "hello"})).await?;

        // Do not launch the probe into an unverified UID or the visible desktop.
        let answered_as_configured = match self.desktop().kind() {
            Kind::User(login) => hello["user"]["uid"].as_u64() == Some(login.uid.into()),
            Kind::Vm(_) => return Err(self.unreachable()),
        };
        if hello["ok"] != true || !answered_as_configured || hello["session"]["onConsole"] != false
        {
            return Ok(hello);
        }

        // Independent reads after the daemon's UID and off-console gate, joined
        // before returning: every identity still needs a new input report and
        // a new challenged proof, whose epochs require_hidden compares.
        // Neither result authorizes anything on its own.
        let input = self.input_transport()?;
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
                .link
                .call(&json!({"op": "session-proof", "challenge": challenge}))
                .await?;
            return Self::challenged_proof(&reply, &challenge);
        }
        match self.desktop().kind() {
            Kind::User(login) => crate::user::session::session_proof_by_command(self, login).await,
            Kind::Vm(_) => Err(self.unreachable()),
        }
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
        // What this build does on a VM verifies it through its own path,
        // `verify_vm`: everything else is refused here, before anything is
        // asked of it, in the one wording.
        self.second_login_only()?;
        let observation = self.observed().await;
        let session = self.desktop().prove(&observation)?;
        Ok(Verified {
            session,
            observation,
            machine: None,
        })
    }

    /// The desktop proved for a command this build makes on either kind:
    /// Second Login through [`Client::verify`], a VM afresh through its
    /// keeper, its proof's generation kept for the changes that follow.
    pub(crate) async fn verify_either(&self) -> Result<Verified> {
        if matches!(self.desktop().kind(), Kind::Vm(_)) {
            self.verify_vm().await
        } else {
            self.verify().await
        }
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

        match self.desktop().kind() {
            Kind::User(login) => crate::user::install::staging_writable_in(self, login)?,
            Kind::Vm(_) => return Err(self.unreachable()),
        }

        Ok(())
    }
}
