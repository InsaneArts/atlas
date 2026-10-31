//! Input to the helper through its input service: a batch validated in
//! the verified session and sent under a grant of input, checked against
//! the app it is for when asked, and the native requests behind it that
//! take, release and reset that grant.
use super::{Actor, Client, service::require_accessibility};
use crate::{
    actions::{self, Expect},
    control::{Beside, Ownership, SharedLease},
    error::{Code, Error, Holder, Result},
    session::{
        Epoch, FOCUS_APP, FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW, Session, has_capability,
        require_hidden,
    },
    transport::Transport,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The longest the input service's grant can outlast a command killed
/// while holding it: the grant's own life, and a second to spare.
const GRANT_OUTLIVES: Duration = crate::native::GRANT_TTL.saturating_add(Duration::from_secs(1));

/// How an agent's batch is checked against the app it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Sent only with the check: an input service that cannot check is
    /// refused before anything is sent.
    Required(Expect),
}

/// What the helper is asked to expect of an agent's batch, given what its
/// input service `native` says it can check. An input service that answered
/// without the whole check, the Dock's part included and the window's and
/// field's when the batch names them, is refused when the check is
/// required: it would post the input unchecked, or let a click on a Dock
/// icon through, and input cannot be taken back once it is sent. One that
/// did not answer is refused later, as unavailable.
pub(super) fn expectation(native: &Value, check: Option<Check>) -> Result<Option<Expect>> {
    let answered = native["ok"] == true;
    match check {
        None => Ok(None),
        Some(Check::Required(expect)) if !answered || checks_all(native, &expect) => {
            Ok(Some(expect))
        }
        Some(Check::Required(_)) => Err(Error::new(
            Code::Startup,
            "The helper's input service is from an older Atlas: run atlas start-input, then try again",
        )),
    }
}

/// Whether input service `native` checks all of `expect`: the app, with
/// the Dock, and the window and field when it names them.
fn checks_all(native: &Value, expect: &Expect) -> bool {
    has_capability(native, FOCUS_APP)
        && has_capability(native, FOCUS_DOCK)
        && (expect.window().is_none() || has_capability(native, FOCUS_WINDOW))
        && (expect.field().is_none() || has_capability(native, FOCUS_FIELD))
}

impl Client {
    pub(crate) fn input_transport(&self) -> Transport {
        let peer_uid = self.config.helper_uid;
        #[cfg(test)]
        let peer_uid = self.input_peer_uid.unwrap_or(peer_uid);
        Transport {
            path: self.config.input_socket.clone(),
            timeout: Duration::from_secs(5),
            peer_uid,
        }
    }

    pub(crate) async fn native_control(&self, operation: &str, owner: &str) -> Result<Value> {
        self.native_request(json!({ "op": operation, "owner": owner }))
            .await
    }

    pub(crate) async fn acquire_input(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
        // The person's viewer may take over while agents are stopped.
        self.pending_until_known(self.native_request(json!({
            "op": "acquire",
            "owner": owner,
            "epoch": epoch.input_epoch(),
            "person": self.actor == Actor::Viewer,
        })))
        .await
    }

    /// [`Client::acquire_input`], trying again while a grant another command
    /// left behind runs out: a definite busy from another agent is retried
    /// about ten times a second for what is left of the controller's wait,
    /// and no longer than a grant can outlive its holder. A busy from the
    /// person, and every other answer, is returned at once, and a saved
    /// Stop or a signal's stop ends the retries.
    async fn acquire_input_waiting(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
        let mut until = None;
        loop {
            match self.acquire_input(owner, epoch).await {
                Err(error)
                    if error.code == Code::Busy
                        && error.holder == Some(Holder::Agent)
                        && !error.uncertain =>
                {
                    let until = *until.get_or_insert_with(|| {
                        Instant::now() + self.controller.remaining().min(GRANT_OUTLIVES)
                    });
                    let left = until.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(error);
                    }
                    tokio::time::sleep(crate::control::poll(left)).await;
                    // The service may not have heard of a Stop the person's
                    // side has saved.
                    if self.stop_marked() {
                        return Err(Error::stopped());
                    }
                    self.controller.interrupt().check()?;
                }
                outcome => return outcome,
            }
        }
    }

    pub(super) async fn native_request(&self, request: Value) -> Result<Value> {
        let result = self.native_call(&request).await?;
        if result["ok"] != true {
            let error = Error::new(
                match result["code"].as_str() {
                    Some("busy") => Code::Busy,
                    Some("lease-lost") => Code::LeaseLost,
                    Some("cancelled") => Code::Cancelled,
                    Some("stopped") => Code::Stopped,
                    Some("identity") => Code::Identity,
                    Some("session-changed") => Code::SessionChanged,
                    Some("permission") => Code::Permission,
                    Some("focus-changed") => Code::FocusChanged,
                    _ => Code::Native,
                },
                result["error"]
                    .as_str()
                    .unwrap_or("Native control request failed"),
            );

            // Who holds control, when the service says.
            let error = match result["holder"].as_str().and_then(Holder::parse) {
                Some(holder) => error.held_by(holder),
                None => error,
            };

            // The service reports when a failure may already have changed state.
            return Err(if result["uncertain"] == true {
                error.uncertain()
            } else {
                error
            });
        }

        Ok(result)
    }

    /// Send `request` to the input service and read its answer. It is
    /// never dropped half sent; an answer still missing when a signal's stop
    /// cuts the step under way leaves its outcome unknown, so the claim the
    /// caller holds for it stays pending.
    async fn native_call(&self, request: &Value) -> Result<Value> {
        let interrupt = self.controller.interrupt();
        let connection = self.input_transport().connect().await?;
        let sent = {
            let _sending = interrupt.sending();
            connection.dispatch(request).await?
        };

        tokio::select! {
            biased;
            reply = sent.finish() => reply,
            signal = interrupt.cut() => Err(interrupt.cut_short(signal)),
        }
    }

    pub async fn input(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
    ) -> Result<Value> {
        let _ownership = if lease.is_none() {
            Some(self.controller.acquire_beside_commands()?)
        } else {
            None
        };

        self.input_inner(actions, expected, lease, None).await
    }

    /// Send one validated batch under controller ownership the caller already
    /// holds, refusing unless the helper still has exactly `epoch`. With a
    /// `check`, the helper sends each action only while it would reach that
    /// app.
    pub(crate) async fn input_owned(
        &self,
        _ownership: &Ownership,
        actions: &Value,
        epoch: &Epoch,
        check: Option<Check>,
    ) -> Result<Value> {
        self.input_inner(actions, Some((epoch, Instant::now())), None, check)
            .await
    }

    /// The input service, still running in this same session, was pinned
    /// to an earlier display (switching accounts can renumber it): pin it to
    /// the current verified display, as start-input does. When the display
    /// changed size it is re-pinned too, but the action is refused so the
    /// caller looks at the screen again before acting on it.
    pub(super) async fn repin(&self, session: Session) -> Result<Session> {
        let unavailable = || {
            Error::new(
                Code::Startup,
                "The native input service is unavailable; run atlas start-input",
            )
        };
        let status = self.status().await;
        require_accessibility(&status)?;
        if !crate::session::pinned_to_session(&status, &session) {
            return Err(unavailable());
        }

        let epoch = &session.epoch;
        let pinned = &status["inputService"]["enforcedEpoch"];
        let same_size = pinned["width"].as_f64() == Some(epoch.display.width)
            && pinned["height"].as_f64() == Some(epoch.display.height);

        self.pending_until_known(
            self.native_request(json!({"op": "reset", "epoch": epoch.input_epoch()})),
        )
        .await?;
        let current = self.identity().await?;
        if !current.input {
            return Err(unavailable());
        }

        if !same_size {
            return Err(Error::new(
                Code::SessionChanged,
                "The helper's display changed size. Take a new screenshot, then try again.",
            ));
        }

        Ok(current)
    }

    async fn input_inner(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Check>,
    ) -> Result<Value> {
        // An agent's input holds control taken beside commands; the person's
        // holds the lease, which no command can start under.
        self.controller.require_known_beside(Beside::Commands)?;

        // A viewer's fresh observation is already tied to its acquired lease.
        // The native process validates that exact epoch again before every
        // event. The person's own input is never checked.
        if let (Some((epoch, _)), Some(_)) = (expected, lease) {
            debug_assert!(check.is_none(), "the person's input is never checked");
            let session = Session {
                epoch: epoch.clone(),
                capture: true,
                input: true,
            };
            actions::validate(actions, &session.epoch.display)?;
            return self
                .input_in_session(actions, expected, lease, None, &session)
                .await;
        }

        let status = self.status().await;
        self.input_with_status(&status, actions, expected, lease, check)
            .await
    }

    /// [`Client::input_inner`] once `status` is read. The epoch and whether
    /// the input service checks an agent's app come from this one status,
    /// so both belong to the very process the input goes to.
    pub(super) async fn input_with_status(
        &self,
        status: &Value,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Check>,
    ) -> Result<Value> {
        let session = require_hidden(status, self.config.helper_uid)?;
        let expect = expectation(&status["inputService"], check)?;

        if expected.is_some_and(|(epoch, _)| *epoch != session.epoch) {
            return Err(Error::new(
                Code::SessionChanged,
                "The helper changed since this frame",
            ));
        }

        let input_pid = session.epoch.input_pid;
        let session = if session.input || lease.is_some() {
            session
        } else {
            self.repin(session).await?
        };

        // The service whose capability was read is the one that checks.
        if expect.is_some() && session.epoch.input_pid != input_pid {
            return Err(Error::new(
                Code::SessionChanged,
                "The helper's input service changed meanwhile",
            ));
        }

        actions::validate(actions, &session.epoch.display)?;

        self.input_in_session(actions, expected, lease, expect, &session)
            .await
    }

    /// Carry out validated input in the verified session, retaining the
    /// standalone action's claim until native cleanup is acknowledged.
    pub(super) async fn input_in_session(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        expect: Option<Expect>,
        session: &Session,
    ) -> Result<Value> {
        self.controller.interrupt().check()?;
        let standalone = crate::files::token();
        let owner = lease.map(|(_, token)| token).unwrap_or(&standalone);
        if lease.is_none()
            && let Err(error) = self.acquire_input_waiting(owner, &session.epoch).await
        {
            if error.uncertain {
                // The service may now hold input for `owner`: release it, or
                // leave the outcome pending for recovery. The unacknowledged
                // acquire stays pending even if this cleanup succeeds.
                let _ = self
                    .pending_until_known(self.native_control("release", owner))
                    .await;
            }

            return Err(error);
        }

        let generation = lease
            .is_none()
            .then(|| self.controller.quarantine())
            .transpose()?;
        let mut request = json!({
            "op": "input",
            "actions": actions,
            "owner": owner,
            "epoch": session.epoch.input_epoch(),
            // An agent's command names the app it acted in.
            "app": self.doing.is_some(),
        });
        if let Some(expect) = expect {
            request["expect"] = json!(expect);
        }
        let result = self
            .mutate(request, lease, expected.map(|(_, at)| at), None)
            .await;

        if lease.is_none() {
            let cleanup = self.native_control("release", owner).await;
            let unknown = result
                .as_ref()
                .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
                || cleanup.as_ref().is_err_and(|error| error.uncertain);
            if cleanup.is_ok()
                && !unknown
                && let Some(generation) = &generation
            {
                self.controller.clear_if(generation)?;
            }
            if let Err(cleanup) = cleanup {
                // A release the stop cut short says so, as its input may.
                if cleanup.code == Code::Interrupted {
                    return Err(cleanup);
                }
                let error = match result {
                    Ok(_) => Error::new(
                        Code::Uncertain,
                        "Input finished but native release was not confirmed",
                    ),
                    Err(error) => Error::new(
                        Code::Uncertain,
                        format!("{} Native release was not confirmed either.", error.error),
                    ),
                };
                return Err(error.uncertain());
            }
        }

        result
    }

    /// Carry out a change to the input service as a pending mutation: it
    /// stays pending when its outcome is unknown, and clears once it is
    /// known, done or refused.
    pub(super) async fn pending_until_known<T>(
        &self,
        change: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let generation = self.controller.quarantine()?;
        let outcome = change.await;
        if outcome
            .as_ref()
            .map_or_else(|error| !error.uncertain, |_| true)
        {
            self.controller.clear_if(&generation)?;
        }

        outcome
    }
}
