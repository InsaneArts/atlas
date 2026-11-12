//! Restarting the helper and logging it out, each under a claim of its own
//! that stays until the outcome is known.
use super::{Client, Mutation, service::require_accessibility};
use atlas_control::Ownership;
use atlas_core::{
    desktop::{Kind, SecondLogin},
    error::{Code, Error, Result},
    session::{Observation, Session},
};
use serde_json::{Value, json};
use std::time::Duration;

/// Whether a login is seen gone within `limit`: `looked` (no loginwindow
/// and no Dock) true twice in a row, `pause` apart, and then `answers` (its
/// daemon says hello) false. The processes are looked at all the while: a
/// login seen again, even while its daemon still answers, starts the count
/// over. Only silence after two looks that found it gone, and a third look
/// after that silence (the daemon's answer can take a while, and the login
/// can come back meanwhile), ends the wait.
pub(super) async fn gone_within<L, A>(
    mut looked: impl FnMut() -> L,
    mut answers: impl FnMut() -> A,
    limit: Duration,
    pause: Duration,
) -> bool
where
    L: std::future::Future<Output = bool>,
    A: std::future::Future<Output = bool>,
{
    let gone = async {
        let mut absent = 0;
        loop {
            tokio::time::sleep(pause).await;
            if !looked().await {
                absent = 0;
                continue;
            }

            absent += 1;
            if absent >= 2 && !answers().await {
                if looked().await {
                    return;
                }
                absent = 0;
            }
        }
    };

    tokio::time::timeout(limit, gone).await.is_ok()
}

/// Whether `outcome` leaves nothing unknown: done, or refused for certain.
fn settled(outcome: &Result<Value>) -> bool {
    match outcome {
        Ok(reply) => reply["ok"] == true || reply["uncertain"] != true,
        Err(error) => !error.uncertain,
    }
}

impl Client {
    /// Recover the helper: the verified restart when an outcome is unknown,
    /// otherwise starting, resetting or upgrading input. It never takes
    /// control or resumes input.
    ///
    /// # Errors
    ///
    /// Returns the refusal or failure of the recovery it chose.
    pub async fn recover_helper(&self) -> Result<Value> {
        if self.controller.pending()?.is_empty() {
            self.start_input().await
        } else {
            self.restart().await
        }
    }

    /// Restart the verified helper while retaining exclusion and pending-work ownership.
    ///
    /// # Errors
    ///
    /// Returns admission, permission, service or post-restart identity failures. Unacknowledged
    /// cleanup remains pending; a timeout is not proof that restart completed.
    pub async fn restart(&self) -> Result<Value> {
        let _ownership = self.service_change(true).await?;
        let before = self.identity().await?;

        // Recovery goes through the input service; with none running and no
        // Accessibility to start one, it cannot finish, so nothing is marked.
        let observation = self.observed().await;
        if !observation.native.ok {
            require_accessibility(&observation)?;
        }

        // Held through verification, so the restart reads as in progress.
        // A refusal that changed nothing clears it, as for a log-out.
        let claim = self.controller.quarantine()?;
        let outcome = self.restart_claimed(&before).await;

        self.settle_restart(&claim, outcome)
    }

    /// Settle this restart after verified recovery or a definite refusal.
    /// Other live mutations retain their own claims.
    pub(super) fn settle_restart(&self, claim: &str, outcome: Result<Value>) -> Result<Value> {
        if settled(&outcome) {
            self.controller.clear_if(claim).map_err(Error::uncertain)?;
        }

        outcome
    }

    /// The restart itself, under the claim [`Client::restart`] holds: input
    /// cleared first, then the helper restarted and confirmed new.
    async fn restart_claimed(&self, before: &Session) -> Result<Value> {
        // Input has its own process. Clear held state there before restarting
        // the capture/command helper; a new capture PID alone proves nothing
        // about an uncertain mouse or keyboard action. A service whose file was
        // replaced (an app update) cannot become ready under the restarted
        // helper; it is replaced there instead.
        let observation = self.observed().await;
        let stale = !observation.input_build_current();
        let (input_pid, _) = self.ensure_input(&observation).await?;
        let session = self.identity().await?;
        if session.epoch.input_pid != input_pid {
            return Err(Error::new(
                Code::Uncertain,
                "The input service changed while recovering",
            )
            .uncertain());
        }

        let cleanup = self
            .native_request(json!({ "op": "reset", "epoch": session.epoch.input_epoch() }))
            .await?;
        if cleanup["released"] != true {
            return Err(
                Error::new(Code::Uncertain, "Native input cleanup was not confirmed").uncertain(),
            );
        }

        let reply = self.mutate(Mutation::Restart, None, None, None).await?;
        if reply["ok"] != true {
            return Ok(reply);
        }

        // The helper was told to restart: from here on, every failure leaves
        // the outcome unknown, so the claim stays until recovery.
        let recovery = self.confirm_restart(before, input_pid, stale);
        tokio::time::timeout(Duration::from_secs(20), recovery)
            .await
            .map_err(|_| {
                Error::new(
                    Code::Restart,
                    "Restart acknowledged, but no new hidden helper was confirmed",
                )
                .uncertain()
            })?
    }

    /// Wait for a new capture process and the same input service whose
    /// cleanup was acknowledged, idle and empty, then settle. A `stale`
    /// service that is not ready under the new helper is replaced there.
    async fn confirm_restart(
        &self,
        before: &Session,
        input_pid: u32,
        stale: bool,
    ) -> Result<Value> {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let status = self.status().await;
            let observation = Observation::decode(&status);
            let Ok(session) = self.desktop().prove(&observation) else {
                continue;
            };

            if session.epoch.capture_pid == before.epoch.capture_pid {
                continue;
            }

            if observation.settled_in(&session, input_pid) {
                self.controller.clear().map_err(Error::uncertain)?;
                return Ok(json!({"ok": true, "restarted": true, "session": status}));
            }

            // Its cleanup was confirmed, so nothing is held.
            if stale
                && observation.native.pid == Some(input_pid)
                && observation.native.ready == Some(false)
            {
                self.upgrade_input_for(&status, true)
                    .await
                    .map_err(Error::uncertain)?;
                self.controller.clear().map_err(Error::uncertain)?;
                let status = self.status().await;

                return Ok(json!({
                    "ok": true,
                    "restarted": true,
                    "inputReplaced": true,
                    "session": status,
                }));
            }
        }
    }

    /// Log the helper out (`atlas logout`) while it is in the background
    /// (`identity` refuses it on screen). Its apps quit as at any log-out.
    /// Like a restart, it holds its own claim until the outcome is known:
    /// cleared once the desktop is gone or macOS refused, kept pending when
    /// it may still happen (an app holding it up, an unconfirmed request).
    /// `force` ends the session without asking the apps, and may run with
    /// outcomes still unknown: it is the person's way out of one.
    ///
    /// # Errors
    ///
    /// Returns ownership, identity, job-stop or logout failures. Unknown logout or cleanup
    /// outcomes stay pending and must not be retried blindly.
    pub async fn logout(&self, force: bool) -> Result<Value> {
        let ownership = self.controller.acquire(force)?;
        self.logout_held(&ownership, force).await
    }

    /// [`Client::logout`] under control already held (`_ownership`), for a
    /// caller that keeps it across more than the log-out, such as uninstall.
    ///
    /// # Errors
    ///
    /// Returns a refusal to log out, or an error when the helper has not logged
    /// out yet.
    pub async fn logout_held(&self, _ownership: &Ownership, force: bool) -> Result<Value> {
        let observation = self.observed().await;
        self.desktop().prove(&observation)?;
        let claim = self.controller.quarantine()?;

        // Background jobs are stopped first, so none outlives the login it
        // ran in unsupervised.
        let jobs = if observation.has_capability(atlas_core::protocol::JOBS) {
            Some(crate::jobs::stop_every(self, atlas_core::protocol::StopReason::Stopped).await)
        } else {
            None
        };

        let requested = self
            .mutate(Mutation::Logout { force }, None, None, None)
            .await;
        if !requested.as_ref().is_ok_and(|reply| reply["ok"] == true) {
            // Refused before anything happened: nothing is pending.
            if settled(&requested) {
                self.controller.clear_if(&claim)?;
            }
            return requested;
        }

        if self.desktop_gone(if force { 15 } else { 30 }).await {
            self.controller.clear_if(&claim)?;
            let mut reply = json!({"ok": true, "loggedOut": true});
            if let Some(jobs) = &jobs {
                jobs.say(&mut reply);
            }
            return Ok(reply);
        }

        if force {
            return Err(Error::new(
                Code::Logout,
                "macOS took the request but the helper is still logged in",
            )
            .uncertain());
        }

        let open = self
            .running_apps()
            .await
            .map(|reply| crate::app_selection::left_open(&reply, &self.desktop()))
            .unwrap_or_default();
        let still = if open.is_empty() {
            String::new()
        } else {
            format!(" Still open: {}.", open.join(", "))
        };

        Err(Error::new(
            Code::LogoutHeld,
            format!("The helper has not logged out yet; an app may be asking to save.{still}"),
        )
        .uncertain())
    }

    /// Whether the helper's login goes away within `seconds`: no
    /// loginwindow and no Dock of it, seen twice in a row half a second
    /// apart (a Dock alone can restart in a login that lives on), and its
    /// daemon no longer answering. A look that fails counts as present.
    async fn desktop_gone(&self, seconds: u64) -> bool {
        match self.desktop().kind() {
            Kind::SecondLogin(login) => self.second_login_gone(login, seconds).await,
        }
    }

    /// [`Client::desktop_gone`] for the second login `login`, whose
    /// processes this Mac can see.
    async fn second_login_gone(&self, login: &SecondLogin, seconds: u64) -> bool {
        let uid = login.uid;
        let looked = move || async move {
            tokio::task::spawn_blocking(move || atlas_core::session::logged_out(uid))
                .await
                .ok()
                .flatten()
                == Some(true)
        };
        let answers = || async { self.transport.call(&json!({"op": "hello"})).await.is_ok() };

        gone_within(
            looked,
            answers,
            Duration::from_secs(seconds),
            Duration::from_millis(500),
        )
        .await
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
