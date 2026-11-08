//! Input to the helper through its input service: a batch validated in
//! the verified session and sent under a grant of input, checked against
//! the app it is for when asked, the native requests behind it that take,
//! release and reset that grant, and the looks at an app's elements it
//! keeps for referenced clicks.
use super::{Actor, Client, Mutation, Verified, service::require_accessibility};
use atlas_control::{Beside, Ownership, SharedLease};
use atlas_core::{
    actions::{self, Expect},
    error::{Code, Error, Holder, Result},
    session::{
        ELEMENT_ACTIONS, Epoch, FOCUS_APP, FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW, LOOKS,
        NativeObservation, Observation, Session, TYPED_FIELDS,
    },
    transport::Transport,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The longest the input service's grant can outlast a command killed
/// while holding it: the grant's own life, and a second to spare.
const GRANT_OUTLIVES: Duration =
    atlas_core::session::GRANT_TTL.saturating_add(Duration::from_secs(1));

/// The longest a look may take, as long as the probe was given: the scan's
/// own four seconds, each request within them, and the reply.
const LOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// Refuse unless the input service that answered `verified` reads and
/// keeps looks; nothing is asked of one that does not.
pub(crate) fn require_looks(verified: &Verified) -> Result<()> {
    if verified.input_offers(LOOKS) {
        return Ok(());
    }
    Err(Error::new(
        Code::Startup,
        "The helper's input service cannot read elements: run atlas start-input, then try again",
    ))
}

/// What the helper is asked to expect of an agent's batch, given what its
/// input service `native` says it can check. An input service that answered
/// without the whole check, the Dock's part included and the window's and
/// field's when the batch names them, is refused when the check is
/// required: it would post the input unchecked, or let a click on a Dock
/// icon through, and input cannot be taken back once it is sent. One that
/// did not answer is refused later, as unavailable.
pub(super) fn expectation(
    native: &NativeObservation,
    check: Option<Expect>,
) -> Result<Option<Expect>> {
    match check {
        None => Ok(None),
        Some(expect) if !native.ok || checks_all(native, &expect) => Ok(Some(expect)),
        Some(_) => Err(Error::new(
            Code::Startup,
            "The helper's input service is from an older Atlas: run atlas start-input, then try again",
        )),
    }
}

/// Whether input service `native` checks all of `expect`: the app, with
/// the Dock, and the window and field when it names them.
fn checks_all(native: &NativeObservation, expect: &Expect) -> bool {
    native.has_capability(FOCUS_APP)
        && native.has_capability(FOCUS_DOCK)
        && (expect.window().is_none() || native.has_capability(FOCUS_WINDOW))
        && (expect.field().is_none() || native.has_capability(FOCUS_FIELD))
}

/// The input service's refusal in `result`: its code when it is one callers
/// act on, who holds control when it says, and whether the failure may
/// already have changed state.
fn native_refusal(result: &Value) -> Error {
    let error = Error::refused(
        result["code"]
            .as_str()
            .and_then(Code::parse)
            .filter(|code| code.native())
            .unwrap_or(Code::Native),
        result,
        "Native control request failed",
    );
    match result["holder"].as_str().and_then(Holder::parse) {
        Some(holder) => error.held_by(holder),
        None => error,
    }
}

impl Client {
    pub(crate) fn input_transport(&self) -> Transport {
        let peer_uid = self.config.helper_uid;
        #[cfg(any(test, feature = "test-support"))]
        let peer_uid = self.input_peer_uid.unwrap_or(peer_uid);
        Transport {
            path: self.config.input_socket.clone(),
            timeout: Duration::from_secs(5),
            peer_uid,
        }
    }

    /// One look at the elements of the app `expect` names, read and kept
    /// by the input service of the session `verified` describes, which the
    /// caller verified just before. Its reply names the look. A read sends
    /// nothing, so its failure is never uncertain.
    pub(crate) async fn look_at(&self, verified: &Verified, expect: &Expect) -> Result<Value> {
        require_looks(verified)?;
        let request = json!({
            "op": "elements",
            "epoch": verified.session().epoch.input_epoch(),
            "expect": expect,
        });
        let mut transport = self.input_transport();
        transport.timeout = LOOK_TIMEOUT;
        let connection = transport.connect().await.map_err(Error::certain)?;
        // Once asked, a read with no answer in time is the one the probe
        // gave none for.
        let reply = async { connection.dispatch(&request).await?.finish().await }
            .await
            .map_err(|cause| {
                Error::new(Code::Elements, "The accessibility probe returned no result")
                    .with_source(cause)
            })?;
        if reply["ok"] != true {
            // As the probe's failures read: a refusal of the session keeps
            // its code, and the read's own failure is the elements'.
            let code = match Code::of_reply(&reply, Code::Elements) {
                code @ (Code::SessionChanged | Code::Identity | Code::Busy) => code,
                _ => Code::Elements,
            };
            return Err(Error::refused(code, &reply, "The accessibility probe failed").certain());
        }
        Ok(reply)
    }

    /// # Errors
    ///
    /// Returns the failure to verify the helper, or the input service's
    /// refusal.
    pub async fn native_control(&self, operation: &str, owner: &str) -> Result<Value> {
        self.native_request(json!({ "op": operation, "owner": owner }))
            .await
    }

    /// # Errors
    ///
    /// Returns the failure to verify the helper for `epoch`, or the input
    /// service's refusal.
    pub async fn acquire_input(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
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
                    tokio::time::sleep(atlas_control::poll(left)).await;
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
        let interrupt = self.controller.interrupt();
        let connection = self.input_transport().connect().await?;
        let sent = {
            // A signal never drops a half-sent request. Missing replies retain
            // the caller's pending claim through the shared answer path.
            let _sending = interrupt.sending();
            connection.dispatch(&request).await?
        };
        let result = self.answer(sent).await?;
        if result["ok"] != true {
            return Err(native_refusal(&result));
        }

        if result["uncertain"] == true {
            return Err(Error::new(
                Code::Uncertain,
                "Input service reported an uncertain outcome",
            )
            .uncertain());
        }

        Ok(result)
    }

    /// # Errors
    ///
    /// Returns the refusal of the batch or the failure to deliver it; the error
    /// says when its outcome is uncertain.
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

        self.input_inner(actions::Batch::decode(actions), expected, lease, None)
            .await
    }

    /// Keep decoding failures behind controller and session guards while carrying
    /// the decoded batch through the input decision. Ownership outlives cleanup.
    pub(crate) async fn input_owned(
        &self,
        _ownership: &Ownership,
        actions: Result<actions::Batch<'_>>,
        expected: Option<(&Epoch, Instant)>,
        check: Option<Expect>,
    ) -> Result<Value> {
        self.input_inner(actions, expected, None, check).await
    }

    /// The input service, still running in this same session, was pinned
    /// to an earlier display (switching accounts can renumber it): pin it to
    /// the current verified display, as start-input does. When the display
    /// changed size it is re-pinned too, but the action is refused so the
    /// caller looks at the screen again before acting on it.
    pub(super) async fn repin(&self, session: Session) -> Result<Session> {
        let status = self.status().await;
        let observation = Observation::decode(&status);
        let unavailable = || {
            let cause = status["inputService"]["error"]
                .as_str()
                .map_or_else(String::new, |cause| format!(" ({cause})"));
            Error::new(
                Code::Startup,
                format!("The native input service is unavailable{cause}; run atlas start-input"),
            )
        };
        require_accessibility(&observation)?;
        if !observation.native.pinned_to_session(&session) {
            return Err(unavailable());
        }

        let epoch = &session.epoch;
        let same_size = observation.native.repinnable(&session);

        self.pending_until_known(
            self.native_request(json!({"op": "reset", "epoch": epoch.input_epoch()})),
        )
        .await?;
        let current = self.identity().await?;
        if !current.input {
            return Err(unavailable());
        }

        if !same_size {
            return Err(Error::session_changed());
        }

        Ok(current)
    }

    async fn input_inner(
        &self,
        actions: Result<actions::Batch<'_>>,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Expect>,
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
            let actions = actions?;
            actions.check(&session.epoch.display)?;
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
        actions: Result<actions::Batch<'_>>,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Expect>,
    ) -> Result<Value> {
        let observation = Observation::decode(status);
        let session = observation.require_hidden(self.config.helper_uid)?;
        // An input service older than this one may call a password field a
        // plain one: no text typed through it is kept.
        if !observation.native.has_capability(TYPED_FIELDS)
            && let Some(doing) = &self.doing
        {
            doing.before().typed_into(None);
        }
        let expect = expectation(&observation.native, check)?;

        if expected.is_some_and(|(epoch, _)| *epoch != session.epoch) {
            return Err(Error::session_changed());
        }

        let input_pid = session.epoch.input_pid;
        let session = if session.input || lease.is_some() {
            session
        } else {
            self.repin(session).await?
        };

        // The service whose capability was read is the one that checks, and
        // the one that acts on elements.
        let replaced = session.epoch.input_pid != input_pid;
        if expect.is_some() && replaced {
            return Err(Error::session_changed());
        }

        let actions = actions?;
        if actions
            .actions()
            .iter()
            .any(|action| action.element_action().is_some())
        {
            if !observation.native.has_capability(ELEMENT_ACTIONS) {
                return Err(Error::new(
                    Code::Startup,
                    "The helper's input service cannot act on elements: run atlas start-input, \
                     then try again",
                ));
            }
            if replaced {
                return Err(Error::session_changed());
            }
        }
        actions.check(&session.epoch.display)?;

        self.input_in_session(actions, expected, lease, expect, &session)
            .await
    }

    /// Carry out validated input in the verified session, retaining the
    /// standalone action's claim until native cleanup is acknowledged.
    pub(super) async fn input_in_session(
        &self,
        actions: actions::Batch<'_>,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        expect: Option<Expect>,
        session: &Session,
    ) -> Result<Value> {
        self.controller.interrupt().check()?;
        // Asked for now, so its capture runs beside the grant and the claim.
        let picture = self.picture_before(&session.epoch.input_epoch(), session.epoch.capture_pid);
        let standalone = atlas_core::files::token();
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
        // An agent's input is judged by the person's accounts while they
        // list any; the person's own never is.
        let asking = lease
            .is_none()
            .then(|| crate::asking::policy(self))
            .flatten();
        let mutation = Mutation::Input {
            actions,
            owner,
            epoch: session.epoch.input_epoch(),
            picture,
            app: self.doing.is_some(),
            expect: expect.as_ref(),
            asking,
        };
        let result = self
            .mutate(mutation, lease, expected.map(|(_, at)| at), None)
            .await;

        match &generation {
            Some(generation) => self.release_input(owner, generation, result).await,
            None => result,
        }
    }

    /// End a standalone action: release its grant, and settle `generation`
    /// only once both the action's `result` and the release are known. The
    /// claim stays held until this returns.
    async fn release_input(
        &self,
        owner: &str,
        generation: &str,
        result: Result<Value>,
    ) -> Result<Value> {
        let cleanup = self.native_control("release", owner).await;
        let unknown = result
            .as_ref()
            .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true);
        let Err(cleanup) = cleanup else {
            if !unknown {
                self.controller.clear_if(generation)?;
            }
            return result;
        };

        // A release the stop cut short says so, as its input may.
        if cleanup.code == Code::Interrupted {
            return Err(cleanup);
        }
        let message = match result {
            Ok(_) => format!(
                "Input finished but native release was not confirmed: {}",
                cleanup.error
            ),
            Err(error) => format!(
                "{} Native release was not confirmed either: {}",
                error.error, cleanup.error
            ),
        };
        Err(Error::new(Code::Uncertain, message).uncertain())
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
        if !outcome.as_ref().is_err_and(|error| error.uncertain) {
            self.controller.clear_if(&generation)?;
        }

        outcome
    }
}
