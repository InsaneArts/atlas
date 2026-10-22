//! Ownership is independent of the event engine, so cancellation never queues
//! behind a drag, text insertion or wait. Posting holds this gate briefly.
use crate::error::{Error, Result};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(6);

/// Who asks for control: an agent, or the person through a viewer. A stop
/// refuses agents only, so the person can still take over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Agent,
    Person,
}

#[derive(Default)]
pub struct Gate(Mutex<State>);

#[derive(Default)]
pub struct State {
    owner: Option<String>,
    previous: Option<String>,
    expires: Option<Instant>,
    closed: bool,
    /// Whether the person holds control now, not an agent.
    person: bool,
    /// The person stopped agents: none may take control until they resume.
    held: bool,
    /// The owner whose control a cancel ended (the person's or the
    /// controller's doing, not expiry or the gate closing), and whether a
    /// stop ended it; told once, to that owner's action under way.
    cancelled: Option<(String, bool)>,
}

impl State {
    fn expire(&mut self) {
        if self.expires.is_some_and(|at| at <= Instant::now()) {
            self.owner = None;
        }
    }

    /// Refuse unless `owner` holds control. An action under way (`acting`)
    /// whose owner was cancelled is told so, once; anything else that finds
    /// control gone is told it was lost.
    fn require(&mut self, owner: &str, acting: bool) -> Result<()> {
        self.expire();

        if self.closed || self.owner.as_deref() != Some(owner) {
            let ended = self
                .cancelled
                .as_ref()
                .filter(|(cancelled, _)| cancelled == owner)
                .map(|(_, stopped)| *stopped);
            if acting
                && !self.closed
                && let Some(stopped) = ended
            {
                self.cancelled = None;
                // The person's stop says so, to the agent it cut off.
                return Err(if stopped {
                    Error::new("stopped", crate::error::STOPPED)
                } else {
                    Error::new("cancelled", "Native control was stopped")
                });
            }

            return Err(Error::new(
                "lease-lost",
                "Native control was released or expired",
            ));
        }

        Ok(())
    }
}

impl Gate {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.0
            .lock()
            .map_err(|_| Error::new("native", "Input ownership failed"))
    }

    pub fn active(&self) -> Result<bool> {
        let mut state = self.lock()?;
        state.expire();
        Ok(state.owner.is_some())
    }

    /// Whether the person holds control now, through a viewer.
    pub fn person_in_control(&self) -> Result<bool> {
        let mut state = self.lock()?;
        state.expire();
        Ok(state.owner.is_some() && state.person)
    }

    pub fn acquire(self: &Arc<Self>, owner: &str, who: Who) -> Result<Grant> {
        let mut state = self.lock()?;
        state.expire();

        if state.held && who == Who::Agent && !state.closed {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }

        // An agent finding the person in control is told to wait for them.
        if !state.closed && state.owner.is_some() && state.person && who == Who::Agent {
            return Err(Error::new("busy", crate::error::PERSON_HAS_CONTROL));
        }

        if state.closed || state.owner.is_some() {
            return Err(Error::new(
                "busy",
                "The native input service is unavailable or already controlled",
            ));
        }

        // Owners are `control::token()`s: two hexadecimal digits a byte.
        if owner.len() != 2 * crate::control::TOKEN_BYTES
            || !owner.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::new("input", "Invalid native owner token"));
        }

        state.owner = Some(owner.to_owned());
        state.previous = state.owner.clone();
        state.expires = Some(Instant::now() + TTL);
        state.person = who == Who::Person;
        state.cancelled = None;

        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    /// The grant for an input request: the action under way, so it hears
    /// of a cancel or a stop that came since control was taken.
    pub fn grant(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        self.lock()?.require(owner, true)?;
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    pub fn renew(&self, owner: &str) -> Result<()> {
        let mut state = self.lock()?;
        state.require(owner, false)?;
        state.expires = Some(Instant::now() + TTL);
        Ok(())
    }

    /// End ownership on purpose: the person stopped, or the controller
    /// released it. What was under way then reads as cancelled.
    pub fn cancel(&self, owner: Option<&str>) -> Result<()> {
        let mut state = self.lock()?;
        if let Some(owner) = owner
            && state.previous.as_deref() != Some(owner)
        {
            return Err(Error::new(
                "lease-lost",
                "This token cannot release another controller",
            ));
        }

        if let Some(owner) = state.owner.take() {
            state.cancelled = Some((owner, false));
        }

        Ok(())
    }

    /// Stop agents (`on`) or let them go on. Stopping ends an agent's
    /// control as a cancel does, so what it had under way reads as
    /// cancelled; the person's own control is left alone.
    pub fn hold(&self, on: bool) -> Result<()> {
        let mut state = self.lock()?;
        state.held = on;

        if on
            && !state.person
            && let Some(owner) = state.owner.take()
        {
            state.cancelled = Some((owner, true));
        }

        Ok(())
    }

    /// Whether the person has stopped agents.
    pub fn held(&self) -> Result<bool> {
        Ok(self.lock()?.held)
    }

    /// End ownership for a cause that is nobody's doing (the session
    /// changed, an action failed). What was under way reads as lost.
    pub fn revoke(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.owner = None;
        state.cancelled = None;
        Ok(())
    }

    pub fn close(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.closed = true;
        state.owner = None;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Grant {
    gate: Arc<Gate>,
    owner: String,
}

impl Grant {
    pub fn authorize(&self) -> Result<MutexGuard<'_, State>> {
        let mut state = self.gate.lock()?;
        state.require(&self.owner, true)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Arc<Gate> {
        Arc::new(Gate::default())
    }

    fn owner(digit: char) -> String {
        digit.to_string().repeat(2 * crate::control::TOKEN_BYTES)
    }

    fn code<T>(result: Result<T>) -> &'static str {
        result.err().map_or("ok", |error| error.code)
    }

    #[test]
    fn a_second_controller_is_busy() {
        let gate = gate();
        let _grant = gate.acquire(&owner('a'), Who::Agent).unwrap();

        assert!(gate.active().unwrap());
        assert_eq!(code(gate.acquire(&owner('b'), Who::Agent)), "busy");
        assert_eq!(
            code(gate.acquire(&owner('a'), Who::Agent)),
            "busy",
            "even the owner"
        );
    }

    #[test]
    fn an_agent_finding_the_person_in_control_is_told_to_wait_for_them() {
        let gate = gate();
        let _person = gate.acquire(&owner('a'), Who::Person).unwrap();

        assert!(gate.person_in_control().unwrap());
        let said = |who| {
            gate.acquire(&owner('b'), who)
                .err()
                .map(|error| (error.code, error.error))
        };
        assert_eq!(
            said(Who::Agent),
            Some(("busy", crate::error::PERSON_HAS_CONTROL.to_owned()))
        );
        assert_ne!(
            said(Who::Person).map(|(_, error)| error),
            Some(crate::error::PERSON_HAS_CONTROL.to_owned()),
            "another viewer is not told to wait for the person"
        );
    }

    #[test]
    fn owners_are_control_tokens() {
        let gate = gate();
        assert_eq!(owner('a').len(), crate::control::token().len());

        for bad in [
            String::new(),
            "a".repeat(47),
            "a".repeat(49),
            "g".repeat(48),
        ] {
            assert_eq!(code(gate.acquire(&bad, Who::Agent)), "input", "{bad:?}");
        }

        assert!(!gate.active().unwrap());
        assert_eq!(
            code(gate.acquire(&crate::control::token(), Who::Agent)),
            "ok"
        );
    }

    #[test]
    fn ownership_expires_unless_renewed() {
        let gate = gate();
        let grant = gate.acquire(&owner('a'), Who::Agent).unwrap();

        gate.renew(&owner('a')).unwrap();
        assert_eq!(code(gate.renew(&owner('b'))), "lease-lost");

        gate.lock().unwrap().expires = Some(Instant::now());
        assert!(!gate.active().unwrap());
        assert_eq!(code(gate.renew(&owner('a'))), "lease-lost");
        assert_eq!(code(gate.grant(&owner('a'))), "lease-lost");
        assert_eq!(code(grant.authorize()), "lease-lost");
        assert_eq!(
            code(gate.acquire(&owner('b'), Who::Agent)),
            "ok",
            "free again"
        );
    }

    #[test]
    fn a_stale_token_cannot_end_a_newer_controller() {
        let gate = gate();
        let _first = gate.acquire(&owner('a'), Who::Agent).unwrap();

        gate.cancel(Some(&owner('a'))).unwrap();
        let _second = gate.acquire(&owner('b'), Who::Agent).unwrap();
        assert_eq!(code(gate.cancel(Some(&owner('a')))), "lease-lost");
        assert!(gate.active().unwrap());

        gate.grant(&owner('b')).unwrap();
        gate.cancel(Some(&owner('b'))).unwrap();
        assert!(!gate.active().unwrap());
    }

    #[test]
    fn nothing_is_granted_after_a_cancel() {
        let gate = gate();
        let _acquired = gate.acquire(&owner('a'), Who::Agent).unwrap();
        let grant = gate.grant(&owner('a')).unwrap();
        drop(grant.authorize().unwrap());
        gate.cancel(None).unwrap();

        assert_eq!(
            code(gate.renew(&owner('a'))),
            "lease-lost",
            "a keep-alive is not told"
        );
        assert_eq!(
            code(gate.grant(&owner('b'))),
            "lease-lost",
            "nor a stranger"
        );
        assert_eq!(
            code(grant.authorize()),
            "cancelled",
            "the action under way is"
        );
        assert_eq!(code(grant.authorize()), "lease-lost", "once");

        let _again = gate.acquire(&owner('a'), Who::Agent).unwrap();
        gate.revoke().unwrap();

        assert_eq!(
            code(gate.grant(&owner('a'))),
            "lease-lost",
            "nobody stopped it"
        );
    }

    #[test]
    fn a_stop_cancels_an_agents_action_and_refuses_agents_until_resumed() {
        let gate = gate();
        let _acquired = gate.acquire(&owner('a'), Who::Agent).unwrap();
        let grant = gate.grant(&owner('a')).unwrap();

        gate.hold(true).unwrap();
        assert!(gate.held().unwrap());
        assert!(!gate.active().unwrap());
        assert_eq!(
            code(grant.authorize()),
            "stopped",
            "what it was doing stops"
        );
        assert_eq!(code(gate.acquire(&owner('b'), Who::Agent)), "stopped");

        let person = gate.acquire(&owner('c'), Who::Person).unwrap();
        gate.hold(true).unwrap();
        assert_eq!(code(person.authorize()), "ok", "the person keeps control");
        gate.cancel(Some(&owner('c'))).unwrap();

        gate.hold(false).unwrap();
        assert!(!gate.held().unwrap());
        assert_eq!(code(gate.acquire(&owner('b'), Who::Agent)), "ok");
    }

    #[test]
    fn an_input_asked_for_after_a_stop_hears_why_control_ended() {
        let gate = gate();
        let _acquired = gate.acquire(&owner('a'), Who::Agent).unwrap();
        gate.hold(true).unwrap();
        assert_eq!(code(gate.grant(&owner('a'))), "stopped", "not merely lost");
        assert_eq!(code(gate.grant(&owner('a'))), "lease-lost", "told once");

        gate.hold(false).unwrap();
        let _again = gate.acquire(&owner('b'), Who::Agent).unwrap();
        gate.cancel(None).unwrap();
        gate.hold(true).unwrap();
        assert_eq!(
            code(gate.grant(&owner('b'))),
            "cancelled",
            "the cause is what ended it, not what holds now"
        );
    }

    #[test]
    fn a_closed_gate_stays_closed() {
        let gate = gate();
        let grant = gate.acquire(&owner('a'), Who::Agent).unwrap();

        gate.close().unwrap();
        assert!(!gate.active().unwrap());
        assert_eq!(code(grant.authorize()), "lease-lost");
        assert_eq!(code(gate.acquire(&owner('b'), Who::Agent)), "busy");

        gate.cancel(None).unwrap();
        assert_eq!(
            code(gate.acquire(&owner('b'), Who::Agent)),
            "busy",
            "for good"
        );
    }
}
