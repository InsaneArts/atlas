//! Changes the person makes in the helper through the viewer: taking,
//! renewing and ending control under a lease, input sent under it, and
//! recovery and cleanup, which wait until no control is held.
use super::{LEASE_SECONDS, Viewer, frames::Observation, unpoisoned};
use atlas_client::client::Client;
use atlas_control::ActionPermit;
use atlas_core::{
    error::{Code, Error, Result},
    session::Epoch,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// What a refusal of a lease's request says, as the viewer and the MCP view
/// each decide the lease's fate from it.
pub(crate) enum LeaseRefusal {
    /// The request could not be read.
    Unread,
    Busy,
    /// The viewer holds no such lease.
    Lost,
    /// The menu bar did not answer.
    Unanswered,
    Failed,
}

impl From<Code> for LeaseRefusal {
    fn from(code: Code) -> Self {
        match code {
            Code::InvalidJson | Code::Input => Self::Unread,
            Code::Busy => Self::Busy,
            Code::LeaseLost => Self::Lost,
            Code::Menubar => Self::Unanswered,
            Code::Agents
            | Code::Apps
            | Code::BadRequest
            | Code::Blocked
            | Code::Cancelled
            | Code::Capture
            | Code::Cleanup
            | Code::Collect
            | Code::Config
            | Code::ConfigInvalid
            | Code::Credentials
            | Code::Daemon
            | Code::Elements
            | Code::Environment
            | Code::FocusChanged
            | Code::Identity
            | Code::Init
            | Code::Install
            | Code::Interrupted
            | Code::Io
            | Code::Job
            | Code::LoggedOut
            | Code::Logout
            | Code::LogoutHeld
            | Code::Native
            | Code::NoEnvironment
            | Code::NoWindow
            | Code::NotReady
            | Code::OnConsole
            | Code::Onboarding
            | Code::Permission
            | Code::Platform
            | Code::PortInUse
            | Code::Protocol
            | Code::Restart
            | Code::SessionChanged
            | Code::Setup
            | Code::SpawnFailed
            | Code::StaleFrame
            | Code::Startup
            | Code::State
            | Code::Stopped
            | Code::Switch
            | Code::Terminal
            | Code::Transport
            | Code::Uncertain
            | Code::Uninstall
            | Code::Update
            | Code::Usage
            | Code::WaitFailed
            | Code::Windows => Self::Failed,
        }
    }
}

impl Viewer {
    /// End `owner`'s control (any control, without one) in the helper it
    /// was taken on.
    pub(super) async fn release(&self, owner: Option<&str>) -> Result<()> {
        let (previous, client) = {
            let mut lease = self.lease.lock().await;
            (lease.begin_release(owner)?, self.holder())
        };

        let completion = match previous.as_deref() {
            Some(previous) => release_native(&client, previous, false).await,
            None => Ok(()),
        };

        // Native execution can end before its background recording does.
        // Shutdown also flushes when ownership has already ended.
        let _recording = self.input_recording.lock().await;
        let recorded = client.flush_person_input().await;

        if previous.is_some() {
            let mut lease = self.lease.lock().await;
            lease.end_release();
            if !lease.active() {
                *unpoisoned(&self.lease_client) = None;
            }
        }

        completion?;
        recorded
    }

    /// End whichever control is held in a task of its own, which finishes
    /// however long the helper takes; the caller may wait on it or not.
    /// Once shutdown began, its own release ends control in place of this.
    pub(super) fn release_in_background(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let viewer = self.clone();

        tokio::spawn(async move {
            let Some(_changing) = viewer.changing().await else {
                return;
            };

            let _ = viewer.release(None).await;
        })
    }

    /// End whichever control is held, waiting a moment: the helper may take
    /// longer, and control then ends by itself.
    pub(super) async fn release_shortly(self: &Arc<Self>) {
        let ending = self.release_in_background();
        let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;
    }

    /// Take control of `client`'s helper for a new lease, in `epoch`: the
    /// lease's token. Until the helper answers, the acquire counts as input
    /// in progress, so control stays held (the settings cannot move to
    /// another helper meanwhile, nor another controller come in). When the
    /// lease ended meanwhile, or the answer is unknown, control is ended
    /// again on that same helper.
    pub(super) async fn take_control(&self, client: &Arc<Client>, epoch: &Epoch) -> Result<String> {
        let token = {
            let mut lease = self.lease.lock().await;

            // Settings that moved to another helper meanwhile refuse it.
            if !Arc::ptr_eq(client, &self.client()) {
                return Err(Error::session_changed());
            }

            let token = lease.acquire(&client.controller)?;
            *unpoisoned(&self.lease_client) = Some(client.clone());
            *self.lease_epoch.lock().await = Some(epoch.clone());
            token
        };

        // Nothing was asked of the helper yet, so a lease ended this soon
        // just fails here.
        let _acquiring = ActionPermit::begin(self.lease.clone(), &token).await?;
        let acquired = client.acquire_input(&token, epoch).await;
        let unknown = acquired.as_ref().is_err_and(|error| error.uncertain);
        let mut lease = self.lease.lock().await;

        let error = match acquired {
            Ok(_) => match lease.require(&token) {
                Ok(()) => return Ok(token),
                Err(lost) => lost,
            },
            Err(error) if !error.uncertain => {
                // Refused for certain: nothing to undo, the lease just ends.
                if lease.require(&token).is_ok() {
                    lease.release();
                }

                return Err(error);
            }
            Err(error) => error,
        };

        // Control may be granted with no lease left to hold it, or no one
        // knows: end it on the helper it was asked of, and say when that
        // could not be confirmed.
        let ending = lease.begin_release(Some(&token));
        drop(lease);

        let cleanup = release_native(client, &token, unknown).await;
        if matches!(ending, Ok(Some(_))) {
            self.lease.lock().await.end_release();
        }

        Err(match cleanup {
            Ok(()) => error,
            Err(mut cleanup) => {
                cleanup.error = format!("{} {}", error.error, cleanup.error);
                cleanup
            }
        })
    }

    /// Explicit, person-initiated recovery, once no control is held.
    pub(super) async fn recover(self: &Arc<Self>) -> Result<Value> {
        self.require_no_control("Release control before recovering")
            .await?;
        self.client().recover_helper().await
    }

    /// The person's cleanup of the session `body` names, after they
    /// confirmed it: every proof an agent's cleanup needs, but not held
    /// back by the session looking alive.
    pub(super) async fn clean_up(self: &Arc<Self>, session: Option<&str>) -> Result<Value> {
        self.require_no_control("Release control before cleaning up")
            .await?;
        let session = cleanup_session(session)?;
        atlas_client::cleanup::for_person(
            &self.client(),
            &session,
            atlas_client::cleanup::Mode::Apply,
        )
        .await
    }

    /// Follow the settings, then refuse in `refusal`'s words while control
    /// is held.
    async fn require_no_control(self: &Arc<Self>, refusal: &str) -> Result<()> {
        self.follow_settings().await;
        if self.lease.lock().await.active() {
            return Err(Error::new(Code::Busy, refusal));
        }
        Ok(())
    }

    /// The menu bar's emergency stop: end whichever lease is held, with the
    /// same cleanup as any release. Nothing is resumed.
    pub(super) async fn revoke(&self) -> Result<Value> {
        let held = self.lease.lock().await.active();
        self.release(None).await?;
        Ok(json!({"ok": true, "released": held}))
    }

    /// Take control for a new lease, from the frame kept under `key`.
    pub(super) async fn acquire(&self, key: &str) -> Result<Value> {
        let frame = self.observed(key).await?;
        let client = self.client();
        let current = client.identity().await?;
        if current.epoch != frame.epoch {
            return Err(Error::session_changed());
        }

        if !current.input {
            return Err(Error::new(
                Code::Permission,
                "Accessibility permission is unavailable",
            ));
        }

        let token = self.take_control(&client, &frame.epoch).await?;
        Ok(json!({"ok": true, "lease": token, "ttlSeconds": LEASE_SECONDS}))
    }

    /// The frame kept under `key`, when `owner` holds control of the session
    /// it shows.
    async fn controlled_frame(&self, owner: &str, key: &str) -> Result<Arc<Observation>> {
        self.lease.lock().await.require(owner)?;
        let frame = self.observed(key).await?;
        if self.lease_epoch.lock().await.as_ref() != Some(&frame.epoch) {
            return Err(Error::session_changed());
        }

        Ok(frame)
    }

    /// Renew `owner`'s lease, in the helper control was taken on.
    pub(super) async fn renew(&self, owner: &str, key: &str) -> Result<Value> {
        self.controlled_frame(owner, key).await?;
        let client = {
            let mut lease = self.lease.lock().await;
            lease.renew(owner)?;
            self.holder()
        };

        client.native_control("renew", owner).await?;
        Ok(json!({"ok": true}))
    }

    /// Send `actions` for `owner` to the helper control was taken on, as
    /// seen in the frame kept under `key`.
    pub(super) async fn input(&self, owner: &str, key: &str, actions: &Value) -> Result<Value> {
        let frame = self.controlled_frame(owner, key).await?;
        let _recording = self
            .input_recording
            .try_lock()
            .map_err(|_| Error::new(Code::Busy, "Wait for the current input to finish"))?;
        let _permit = ActionPermit::begin(self.lease.clone(), owner).await?;
        let client = self.holder();
        let result = client
            .input(
                actions,
                Some((&frame.epoch, frame.at)),
                Some((&self.lease, owner)),
            )
            .await;

        self.frames.lock().await.input_revision += 1;
        result
    }
}

/// End `owner`'s native control in `client`'s helper. The outcome is
/// marked pending first and cleared only once the helper confirms, so an
/// unconfirmed release stays an unknown outcome for recovery.
///
/// The helper also settles it when it says for certain that `owner` holds
/// no control to end (lease-lost): it was never granted, or another
/// controller took over since. An acquire still under way is ended by the
/// one that asked for it once it is answered. After an acquire whose answer
/// is unknown (`unknown_acquire`), that acquire may still be granted later,
/// so only a confirmed release settles it.
pub(super) async fn release_native(
    client: &Client,
    owner: &str,
    unknown_acquire: bool,
) -> Result<()> {
    let pending = client.controller.quarantine();
    let result = client.native_control("release", owner).await;
    let generation = pending?;

    match result {
        Err(error) if unknown_acquire || error.code != Code::LeaseLost || error.uncertain => {
            let message = format!(
                "Control stopped accepting input, but native cleanup was not confirmed: {error}"
            );
            Err(Error::new(Code::Uncertain, message).uncertain())
        }
        _ => client.controller.clear_if(&generation),
    }
}

/// The session a cleanup route names, by its key.
pub(super) fn cleanup_session(given: Option<&str>) -> Result<atlas_core::host::Key> {
    given
        .and_then(atlas_core::host::Key::parse)
        .ok_or_else(|| Error::new(Code::Input, "Name a session by its key"))
}
