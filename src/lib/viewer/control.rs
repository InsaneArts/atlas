//! Changes the person makes in the helper through the viewer: taking,
//! renewing and ending control under a lease, input sent under it, and
//! recovery and cleanup, which wait until no control is held.
use super::{LEASE_SECONDS, Viewer, unpoisoned};
use crate::{
    client::Client,
    control::ActionPermit,
    error::{Code, Error, Result},
    session::Epoch,
};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    sync::Arc,
};

/// What a control request asks: to send input, or to take, renew or end
/// control, or the menu bar's emergency stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Input,
    Acquire,
    Renew,
    Release,
    Revoke,
}

impl Viewer {
    /// End `owner`'s control (any control, without one) in the helper it
    /// was taken on.
    pub(super) async fn release(&self, owner: Option<&str>) -> Result<()> {
        self.release_with(owner, &mut io::stderr()).await
    }

    pub(super) async fn release_with(
        &self,
        owner: Option<&str>,
        diagnostics: &mut impl Write,
    ) -> Result<()> {
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

        let outcome = match (completion, recorded) {
            (Err(mut error), Err(recording)) => {
                error.error.push_str(&format!(
                    "; recording released input also failed: {recording}"
                ));
                Err(error)
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        };
        if let Err(error) = &outcome {
            // Timer and shutdown callers have no reply channel. Never print lease tokens or input.
            let _ = writeln!(
                diagnostics,
                "End viewer input control failed ({}, uncertain={}); check pending recovery",
                error.code, error.uncertain
            );
        }
        outcome
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
                return Err(Error::new(
                    Code::SessionChanged,
                    "Wait for a fresh frame from the current helper",
                ));
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
            Err(cleanup) => Error {
                error: format!("{} {}", error.error, cleanup.error),
                ..cleanup
            },
        })
    }

    /// Explicit, person-initiated recovery: the verified restart when an
    /// outcome is unknown, otherwise starting, resetting or upgrading input.
    /// It never takes control or resumes input.
    pub(super) async fn recover(self: &Arc<Self>) -> Result<Value> {
        self.follow_settings().await;

        if self.lease.lock().await.active() {
            return Err(Error::new(Code::Busy, "Release control before recovering"));
        }

        let client = self.client();
        if !client.controller.pending()?.is_empty() {
            return client.restart().await;
        }

        client.start_input().await
    }

    /// The person's cleanup of the session `body` names, after they
    /// confirmed it: every proof an agent's cleanup needs, but not held
    /// back by the session looking alive.
    pub(super) async fn clean_up(self: &Arc<Self>, body: &Value) -> Result<Value> {
        self.follow_settings().await;
        if self.lease.lock().await.active() {
            return Err(Error::new(Code::Busy, "Release control before cleaning up"));
        }
        let session = cleanup_session(body["session"].as_str())?;
        crate::cleanup::for_person(&self.client(), &session, crate::cleanup::Mode::Apply).await
    }

    /// Take, renew or end control, or send input, as `asked`.
    pub(super) async fn control(&self, asked: Control, body: &Value) -> Result<Value> {
        let owner = body["lease"].as_str().unwrap_or_default();
        let frame = body["frame"].as_str().unwrap_or_default();

        match asked {
            // The menu bar's emergency stop: end whichever lease is held, with
            // the same cleanup as any release. Nothing is resumed.
            Control::Revoke => {
                let held = self.lease.lock().await.active();
                self.release(None).await?;
                Ok(json!({"ok": true, "released": held}))
            }
            Control::Release => {
                self.release(Some(owner)).await?;
                Ok(json!({"ok": true}))
            }
            Control::Acquire => {
                let frame = self.observed(frame).await?;
                let client = self.client();
                let current = client.identity().await?;
                if current.epoch != frame.epoch {
                    return Err(Error::new(
                        Code::SessionChanged,
                        "Wait for a fresh frame from the current helper",
                    ));
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
            Control::Renew | Control::Input => {
                self.lease.lock().await.require(owner)?;
                let frame = self.observed(frame).await?;
                if self.lease_epoch.lock().await.as_ref() != Some(&frame.epoch) {
                    return Err(Error::new(
                        Code::SessionChanged,
                        "The controlled session changed",
                    ));
                }

                // Renewing and input go to the helper control was taken on.
                if asked == Control::Renew {
                    let client = {
                        let mut lease = self.lease.lock().await;
                        lease.renew(owner)?;
                        self.holder()
                    };

                    client.native_control("renew", owner).await?;
                    return Ok(json!({"ok": true}));
                }

                let _recording = self
                    .input_recording
                    .try_lock()
                    .map_err(|_| Error::new(Code::Busy, "Wait for the current input to finish"))?;
                let _permit = ActionPermit::begin(self.lease.clone(), owner).await?;
                let client = self.holder();
                let result = client
                    .input(
                        &body["actions"],
                        Some((&frame.epoch, frame.at)),
                        Some((&self.lease, owner)),
                    )
                    .await;

                self.frames.lock().await.input_revision += 1;
                result
            }
        }
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

    let nothing_held =
        |error: &Error| !unknown_acquire && error.code == Code::LeaseLost && !error.uncertain;

    match (pending, result) {
        (Ok(generation), Ok(_)) => client.controller.clear_if(&generation),
        (Ok(generation), Err(error)) if nothing_held(&error) => {
            client.controller.clear_if(&generation)
        }
        (Err(error), _) => Err(error),
        (_, Err(error)) => Err(Error::new(
            Code::Uncertain,
            format!(
                "Control stopped accepting input, but native cleanup was not confirmed: {error}"
            ),
        )
        .uncertain()),
    }
}

/// The session a cleanup route names, by its key.
pub(super) fn cleanup_session(given: Option<&str>) -> Result<crate::host_session::Key> {
    given
        .and_then(crate::host_session::Key::parse)
        .ok_or_else(|| Error::new(Code::Input, "Name a session by its key"))
}
