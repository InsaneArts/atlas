//! Restarting the helper and logging it out, each under a claim of its own
//! that stays until the outcome is known.
use super::{Client, service::require_accessibility};
use crate::{
    control::Ownership,
    error::{Code, Error, Result},
    session::{Session, has_capability, input_settled, require_hidden},
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

impl Client {
    pub async fn restart(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(true)?;
        let before = self.identity().await?;

        // Recovery goes through the input service; with none running and no
        // Accessibility to start one, it cannot finish, so nothing is marked.
        let current = self.status().await;
        if current["inputService"]["ok"] != true {
            require_accessibility(&current)?;
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
        let refused = match &outcome {
            Ok(reply) => reply["ok"] != true && reply["uncertain"] != true,
            Err(error) => !error.uncertain,
        };
        if refused || outcome.as_ref().is_ok_and(|reply| reply["ok"] == true) {
            self.controller.clear_if(claim).map_err(Error::uncertain)?;
        }

        outcome
    }

    /// The restart itself, under the claim [`Client::restart`] holds: input
    /// cleared first, then the helper restarted and confirmed new.
    async fn restart_claimed(&self, before: &Session) -> Result<Value> {
        // Input has its own process. Clear held state there before restarting
        // the capture/command helper; a new capture PID alone proves nothing
        // about an uncertain mouse or keyboard action.
        let initial = self.status().await;
        // A service whose file was replaced (an app update) cannot become
        // ready under the restarted helper; it is replaced there instead.
        let stale = !crate::session::input_build_current(&initial);
        let (input_pid, _) = self.ensure_input(&initial).await?;
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

        let reply = self
            .mutate(json!({"op": "restart"}), None, None, None)
            .await?;
        if reply["ok"] != true {
            return Ok(reply);
        }

        // The helper was told to restart: from here on, every failure leaves
        // the outcome unknown, so the claim stays until recovery.
        let recovery = async {
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let status = self.status().await;

                // Settle only with a new capture process and the same input
                // service whose cleanup was acknowledged, idle and empty.
                let Ok(session) = require_hidden(&status, self.config.helper_uid) else {
                    continue;
                };

                if session.epoch.capture_pid == before.epoch.capture_pid {
                    continue;
                }

                if input_settled(&status, self.config.helper_uid, input_pid) {
                    self.controller.clear().map_err(Error::uncertain)?;
                    return Ok(json!({"ok": true, "restarted": true, "session": status}));
                }

                // Its cleanup was confirmed above, so nothing is held: a stale
                // service that is not ready under the new helper is replaced.
                if stale
                    && status["inputService"]["pid"] == input_pid
                    && status["inputService"]["ready"] == false
                {
                    self.upgrade_input(&status)
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
        };

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

    /// Log the helper out (`atlas logout`) while it is in the background
    /// (`identity` refuses it on screen). Its apps quit as at any log-out.
    /// Like a restart, it holds its own claim until the outcome is known:
    /// cleared once the desktop is gone or macOS refused, kept pending when
    /// it may still happen (an app holding it up, an unconfirmed request).
    /// `force` ends the session without asking the apps, and may run with
    /// outcomes still unknown: it is the person's way out of one.
    pub async fn logout(&self, force: bool) -> Result<Value> {
        let ownership = self.controller.acquire(force)?;
        self.logout_held(&ownership, force).await
    }

    /// [`Client::logout`] under control already held (`_ownership`), for a
    /// caller that keeps it across more than the log-out, such as uninstall.
    pub(crate) async fn logout_held(&self, _ownership: &Ownership, force: bool) -> Result<Value> {
        let status = self.status().await;
        require_hidden(&status, self.config.helper_uid)?;
        let claim = self.controller.quarantine()?;

        // Background jobs are stopped first, so none outlives the login it
        // ran in unsupervised.
        let jobs = if has_capability(&status["daemon"], crate::daemon::protocol::JOBS) {
            Some(crate::jobs::stop_every(self, crate::daemon::protocol::StopReason::Stopped).await)
        } else {
            None
        };
        let with_jobs = |mut reply: Value| {
            if let Some(jobs) = &jobs {
                jobs.say(&mut reply);
            }
            reply
        };

        match self
            .mutate(json!({"op": "logout", "force": force}), None, None, None)
            .await
        {
            Ok(reply) if reply["ok"] == true => {}
            // Refused before anything happened: nothing is pending.
            Ok(reply) if reply["uncertain"] != true => {
                self.controller.clear_if(&claim)?;
                return Ok(reply);
            }
            Ok(reply) => return Ok(reply),
            Err(error) => {
                if !error.uncertain {
                    self.controller.clear_if(&claim)?;
                }
                return Err(error);
            }
        }

        if self.desktop_gone(if force { 15 } else { 30 }).await {
            self.controller.clear_if(&claim)?;
            return Ok(with_jobs(json!({"ok": true, "loggedOut": true})));
        }

        if force {
            return Err(Error::new(
                Code::Logout,
                "macOS took the request but the helper is still logged in",
            )
            .uncertain());
        }

        let open = self
            .apps()
            .await
            .map(|reply| crate::app_selection::left_open(&reply))
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
        let uid = self.config.helper_uid;
        let looked = move || async move {
            tokio::task::spawn_blocking(move || crate::session::logged_out(uid))
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
