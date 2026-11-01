//! Ownership is independent of the event engine, so cancellation never queues
//! behind a drag, text insertion or wait. Posting holds this gate briefly.
use crate::error::{Code, Error, Holder, Result};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// Who asks for control: an agent, or the person through a viewer. A stop
/// refuses agents only, so the person can still take over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Who {
    Agent,
    Person,
}

#[derive(Default)]
pub(super) struct Gate(Mutex<State>);

#[derive(Default)]
pub(super) struct State {
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
    fn expire_at(&mut self, now: Instant) {
        if self.expires.is_some_and(|at| at <= now) {
            self.owner = None;
        }
    }

    /// Refuse unless `owner` holds control. An action under way (`acting`)
    /// whose owner was cancelled is told so, once; anything else that finds
    /// control gone is told it was lost.
    fn require_at(&mut self, owner: &str, acting: bool, now: Instant) -> Result<()> {
        self.expire_at(now);

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
                    Error::stopped()
                } else {
                    Error::new(Code::Cancelled, "Native control was stopped")
                });
            }

            return Err(Error::new(
                Code::LeaseLost,
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
            .map_err(|_| Error::new(Code::Native, "Input ownership failed"))
    }

    pub(super) fn active(&self) -> Result<bool> {
        let state = self.lock()?;
        self.active_at(state, Instant::now())
    }

    fn active_at(&self, mut state: MutexGuard<'_, State>, now: Instant) -> Result<bool> {
        state.expire_at(now);
        Ok(state.owner.is_some())
    }

    /// Who holds control now, if anyone: the person through a viewer, or
    /// an agent. Read under one lock, so the two never disagree.
    pub(super) fn holder(&self) -> Result<Option<Holder>> {
        let state = self.lock()?;
        self.holder_at(state, Instant::now())
    }

    fn holder_at(&self, mut state: MutexGuard<'_, State>, now: Instant) -> Result<Option<Holder>> {
        state.expire_at(now);
        Ok(match (state.owner.is_some(), state.person) {
            (false, _) => None,
            (true, true) => Some(Holder::Person),
            (true, false) => Some(Holder::Agent),
        })
    }

    pub(super) fn acquire(self: &Arc<Self>, owner: &str, who: Who) -> Result<Grant> {
        let state = self.lock()?;
        self.acquire_at(state, owner, who, Instant::now(), Instant::now)
    }

    fn acquire_at(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        who: Who,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<Grant> {
        state.expire_at(now);

        if state.held && who == Who::Agent && !state.closed {
            return Err(Error::stopped());
        }

        // An agent finding the person in control is told to wait for them.
        if !state.closed && state.owner.is_some() && state.person && who == Who::Agent {
            return Err(
                Error::new(Code::Busy, crate::error::PERSON_HAS_CONTROL).held_by(Holder::Person)
            );
        }

        if state.closed || state.owner.is_some() {
            let error = Error::new(
                Code::Busy,
                "The native input service is unavailable or already controlled",
            );
            return Err(match (state.closed, state.person) {
                (true, _) => error,
                (false, true) => error.held_by(Holder::Person),
                (false, false) => error.held_by(Holder::Agent),
            });
        }

        // Owners are `files::token()`s: two hexadecimal digits a byte.
        if crate::files::Token::parse(owner).is_none() {
            return Err(Error::new(Code::Input, "Invalid native owner token"));
        }

        state.owner = Some(owner.to_owned());
        state.previous = state.owner.clone();
        state.expires = Some(deadline_now() + super::GRANT_TTL);
        state.person = who == Who::Person;
        state.cancelled = None;

        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    /// The grant for an input request: the action under way, so it hears
    /// of a cancel or a stop that came since control was taken.
    pub(super) fn grant(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        let state = self.lock()?;
        self.grant_at(state, owner, Instant::now())
    }

    fn grant_at(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        now: Instant,
    ) -> Result<Grant> {
        state.require_at(owner, true, now)?;
        drop(state);
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    pub(super) fn renew(&self, owner: &str) -> Result<()> {
        let state = self.lock()?;
        self.renew_at(state, owner, Instant::now(), Instant::now)
    }

    fn renew_at(
        &self,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<()> {
        state.require_at(owner, false, now)?;
        state.expires = Some(deadline_now() + super::GRANT_TTL);
        Ok(())
    }

    /// End ownership on purpose: the person stopped, or the controller
    /// released it. What was under way then reads as cancelled.
    pub(super) fn cancel(&self, owner: Option<&str>) -> Result<()> {
        let mut state = self.lock()?;
        if let Some(owner) = owner
            && state.previous.as_deref() != Some(owner)
        {
            return Err(Error::new(
                Code::LeaseLost,
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
    pub(super) fn hold(&self, on: bool) -> Result<()> {
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
    pub(super) fn held(&self) -> Result<bool> {
        Ok(self.lock()?.held)
    }

    /// End ownership for a cause that is nobody's doing (the session
    /// changed, an action failed). What was under way reads as lost.
    pub(super) fn revoke(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.owner = None;
        state.cancelled = None;
        Ok(())
    }

    pub(super) fn close(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.closed = true;
        state.owner = None;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Grant {
    gate: Arc<Gate>,
    owner: String,
}

impl Grant {
    pub(super) fn authorize(&self) -> Result<MutexGuard<'_, State>> {
        let state = self.gate.lock()?;
        self.authorize_at(state, Instant::now())
    }

    fn authorize_at<'a>(
        &self,
        mut state: MutexGuard<'a, State>,
        now: Instant,
    ) -> Result<MutexGuard<'a, State>> {
        state.require_at(&self.owner, true, now)?;
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
        digit.to_string().repeat(2 * crate::files::TOKEN_BYTES)
    }

    fn code<T>(result: Result<T>) -> &'static str {
        result.err().map_or("ok", |error| error.code.as_str())
    }

    #[test]
    fn a_second_controller_is_busy() {
        let now = Instant::now();
        let gate = gate();
        assert_eq!(gate.holder_at(gate.lock().unwrap(), now).unwrap(), None);
        let _grant = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();

        assert!(gate.active_at(gate.lock().unwrap(), now).unwrap());
        assert_eq!(
            gate.holder_at(gate.lock().unwrap(), now).unwrap(),
            Some(Holder::Agent)
        );
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
            "busy"
        );
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)),
            "busy",
            "even the owner"
        );
    }

    #[test]
    fn an_agent_finding_the_person_in_control_is_told_to_wait_for_them() {
        let now = Instant::now();
        let gate = gate();
        let _person = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Person, now, || now)
            .unwrap();

        assert_eq!(
            gate.holder_at(gate.lock().unwrap(), now).unwrap(),
            Some(Holder::Person)
        );
        let said = |who| {
            gate.acquire_at(gate.lock().unwrap(), &owner('b'), who, now, || now)
                .err()
                .map(|error| (error.code, error.error))
        };
        assert_eq!(
            said(Who::Agent),
            Some((Code::Busy, crate::error::PERSON_HAS_CONTROL.to_owned()))
        );
        let holder = |gate: &Arc<Gate>, who| {
            gate.acquire_at(gate.lock().unwrap(), &owner('b'), who, now, || now)
                .err()?
                .holder
        };
        assert_eq!(holder(&gate, Who::Agent), Some(Holder::Person));
        assert_eq!(holder(&gate, Who::Person), Some(Holder::Person));

        let other = self::gate();
        let _agent = other
            .acquire_at(other.lock().unwrap(), &owner('c'), Who::Agent, now, || now)
            .unwrap();
        assert_eq!(
            holder(&other, Who::Agent),
            Some(Holder::Agent),
            "another agent holds it"
        );
        other.close().unwrap();
        assert_eq!(
            holder(&other, Who::Agent),
            None,
            "a closed service names no one"
        );
        assert_ne!(
            said(Who::Person).map(|(_, error)| error),
            Some(crate::error::PERSON_HAS_CONTROL.to_owned()),
            "another viewer is not told to wait for the person"
        );
    }

    #[test]
    fn owners_are_control_tokens() {
        let now = Instant::now();
        let gate = gate();
        assert_eq!(owner('a').len(), crate::files::token().len());

        for bad in [
            String::new(),
            "a".repeat(47),
            "a".repeat(49),
            "g".repeat(48),
        ] {
            assert_eq!(
                code(gate.acquire_at(gate.lock().unwrap(), &bad, Who::Agent, now, || now)),
                "input",
                "{bad:?}"
            );
        }

        assert!(!gate.active_at(gate.lock().unwrap(), now).unwrap());
        assert_eq!(
            code(gate.acquire_at(
                gate.lock().unwrap(),
                &crate::files::token(),
                Who::Agent,
                now,
                || now
            )),
            "ok"
        );
    }

    #[test]
    fn acquisition_starts_the_ttl_when_the_deadline_is_set() {
        use std::time::Duration;

        let checked = Instant::now();
        let assigned = checked + Duration::from_secs(7);
        for who in [Who::Agent, Who::Person] {
            let gate = gate();
            let grant = gate
                .acquire_at(gate.lock().unwrap(), &owner('a'), who, checked, || assigned)
                .unwrap();
            assert_eq!(
                gate.lock().unwrap().expires,
                Some(assigned + super::super::GRANT_TTL)
            );
            drop(grant.authorize_at(gate.lock().unwrap(), assigned).unwrap());
        }
    }

    #[test]
    fn renewal_starts_the_ttl_after_authorization() {
        use std::time::Duration;

        let start = Instant::now();
        let gate = gate();
        let grant = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                start
            })
            .unwrap();
        let checked = start + Duration::from_secs(1);
        let assigned = checked + Duration::from_secs(7);
        gate.renew_at(gate.lock().unwrap(), &owner('a'), checked, || assigned)
            .unwrap();
        assert_eq!(
            gate.lock().unwrap().expires,
            Some(assigned + super::super::GRANT_TTL)
        );
        drop(grant.authorize_at(gate.lock().unwrap(), assigned).unwrap());
    }

    #[test]
    fn refused_acquisitions_and_renewals_do_not_sample_deadline_time() {
        let now = Instant::now();
        let gate = gate();
        assert_eq!(
            code(
                gate.acquire_at(gate.lock().unwrap(), "invalid", Who::Agent, now, || {
                    panic!("refused acquisition sampled deadline time")
                })
            ),
            "input"
        );
        gate.acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        assert_eq!(
            code(
                gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || {
                    panic!("busy acquisition sampled deadline time")
                })
            ),
            "busy"
        );
        for (owner, checked) in [
            (owner('b'), now),
            (owner('a'), now + super::super::GRANT_TTL),
        ] {
            assert_eq!(
                code(gate.renew_at(gate.lock().unwrap(), &owner, checked, || {
                    panic!("refused renewal sampled deadline time")
                })),
                "lease-lost"
            );
        }
        assert_eq!(
            gate.lock().unwrap().expires,
            Some(now + super::super::GRANT_TTL)
        );
    }

    #[test]
    fn ownership_is_valid_before_expiry_and_lost_at_and_after_expiry() {
        use std::time::Duration;

        let start = Instant::now();
        let expires = start + Duration::from_secs(6);
        let acquired = || {
            let gate = gate();
            let grant = gate
                .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                    start
                })
                .unwrap();
            assert_eq!(gate.lock().unwrap().expires, Some(expires));
            (gate, grant)
        };

        for (now, valid) in [
            (expires - Duration::from_nanos(1), true),
            (expires, false),
            (expires + Duration::from_nanos(1), false),
        ] {
            let (gate, _) = acquired();
            assert_eq!(gate.active_at(gate.lock().unwrap(), now).unwrap(), valid);

            let (gate, _) = acquired();
            assert_eq!(
                gate.holder_at(gate.lock().unwrap(), now).unwrap(),
                valid.then_some(Holder::Agent)
            );

            let expected = if valid { "ok" } else { "lease-lost" };
            let (gate, _) = acquired();
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('a'), now)),
                expected
            );

            let (_, grant) = acquired();
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
                expected
            );

            let (gate, _) = acquired();
            assert_eq!(
                code(gate.renew_at(gate.lock().unwrap(), &owner('a'), now, || now)),
                expected
            );
            assert_eq!(
                gate.lock().unwrap().expires,
                Some(if valid {
                    now + Duration::from_secs(6)
                } else {
                    expires
                })
            );

            let (gate, _) = acquired();
            assert_eq!(
                code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
                if valid { "busy" } else { "ok" }
            );
        }
    }

    #[test]
    fn renewal_moves_only_the_current_owners_deadline() {
        use std::time::Duration;

        let start = Instant::now();
        let gate = gate();
        let first = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                start
            })
            .unwrap();
        let renewed = start + Duration::from_secs(2);
        gate.renew_at(gate.lock().unwrap(), &owner('a'), renewed, || renewed)
            .unwrap();
        let deadline = renewed + Duration::from_secs(6);
        assert_eq!(gate.lock().unwrap().expires, Some(deadline));
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('b'), renewed, || renewed)),
            "lease-lost"
        );
        assert_eq!(gate.lock().unwrap().expires, Some(deadline));
        drop(
            first
                .authorize_at(first.gate.lock().unwrap(), start + Duration::from_secs(6))
                .unwrap(),
        );
        assert_eq!(
            gate.lock().unwrap().expires,
            Some(deadline),
            "authorization does not renew"
        );

        gate.cancel(Some(&owner('a'))).unwrap();
        let acquired = renewed + Duration::from_secs(5);
        let _second = gate
            .acquire_at(
                gate.lock().unwrap(),
                &owner('b'),
                Who::Agent,
                acquired,
                || acquired,
            )
            .unwrap();
        let deadline = acquired + Duration::from_secs(6);
        assert_eq!(gate.lock().unwrap().expires, Some(deadline));
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('a'), acquired, || acquired)),
            "lease-lost"
        );
        assert_eq!(gate.lock().unwrap().expires, Some(deadline));
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('b'), deadline, || deadline)),
            "lease-lost"
        );
        assert_eq!(
            gate.lock().unwrap().expires,
            Some(deadline),
            "a lost lease cannot renew"
        );
    }

    #[test]
    fn cancellation_is_reported_once_to_an_action_but_not_a_keepalive() {
        let start = Instant::now();
        let expires = start + super::super::GRANT_TTL;
        for stopped in [false, true] {
            let gate = gate();
            let grant = gate
                .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                    start
                })
                .unwrap();
            if stopped {
                gate.hold(true).unwrap();
            } else {
                gate.cancel(Some(&owner('a'))).unwrap();
            }

            assert_eq!(
                code(gate.renew_at(gate.lock().unwrap(), &owner('a'), expires, || expires)),
                "lease-lost"
            );
            assert_eq!(gate.lock().unwrap().expires, Some(expires));
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('b'), expires)),
                "lease-lost"
            );
            assert_eq!(gate.lock().unwrap().cancelled, Some((owner('a'), stopped)));
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                if stopped { "stopped" } else { "cancelled" }
            );
            assert_eq!(gate.lock().unwrap().cancelled, None);
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                "lease-lost"
            );
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('a'), expires)),
                "lease-lost"
            );
            assert_eq!(gate.lock().unwrap().expires, Some(expires));
        }
    }

    #[test]
    fn observing_expiry_before_cancel_or_stop_leaves_no_cancellation_reason() {
        let start = Instant::now();
        let expires = start + super::super::GRANT_TTL;
        for expired in [false, true] {
            for stopped in [false, true] {
                let gate = gate();
                let grant = gate
                    .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                        start
                    })
                    .unwrap();
                if expired {
                    assert!(!gate.active_at(gate.lock().unwrap(), expires).unwrap());
                }
                if stopped {
                    gate.hold(true).unwrap();
                } else {
                    gate.cancel(Some(&owner('a'))).unwrap();
                }
                assert_eq!(gate.lock().unwrap().expires, Some(expires));
                assert_eq!(
                    gate.lock().unwrap().previous.as_deref(),
                    Some(owner('a').as_str())
                );
                assert_eq!(
                    code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                    if expired {
                        "lease-lost"
                    } else if stopped {
                        "stopped"
                    } else {
                        "cancelled"
                    }
                );
                assert_eq!(
                    code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                    "lease-lost"
                );
            }
        }
    }

    #[test]
    fn revocation_closure_and_a_new_owner_suppress_an_old_cancellation() {
        let now = Instant::now();
        for ending in ["revoke", "close", "acquire"] {
            let gate = gate();
            let grant = gate
                .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
                .unwrap();
            gate.cancel(None).unwrap();
            match ending {
                "revoke" => gate.revoke().unwrap(),
                "close" => gate.close().unwrap(),
                "acquire" => {
                    gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)
                        .unwrap();
                    assert_eq!(code(gate.cancel(Some(&owner('a')))), "lease-lost");
                }
                _ => unreachable!(),
            }
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
                "lease-lost"
            );
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('a'), now)),
                "lease-lost"
            );
            assert_eq!(
                gate.active_at(gate.lock().unwrap(), now).unwrap(),
                ending == "acquire"
            );
            assert_eq!(
                gate.lock().unwrap().cancelled,
                (ending == "close").then(|| (owner('a'), false))
            );
        }
    }

    #[test]
    fn an_expired_owner_cannot_release_its_successor() {
        let start = Instant::now();
        let expires = start + super::super::GRANT_TTL;
        let gate = gate();
        let first = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, start, || {
                start
            })
            .unwrap();
        let second = gate
            .acquire_at(
                gate.lock().unwrap(),
                &owner('b'),
                Who::Agent,
                expires,
                || expires,
            )
            .unwrap();
        assert_eq!(code(gate.cancel(Some(&owner('a')))), "lease-lost");
        assert_eq!(
            code(first.authorize_at(first.gate.lock().unwrap(), expires)),
            "lease-lost"
        );
        drop(
            second
                .authorize_at(second.gate.lock().unwrap(), expires)
                .unwrap(),
        );
        assert_eq!(
            gate.lock().unwrap().expires,
            Some(expires + super::super::GRANT_TTL)
        );
        gate.cancel(Some(&owner('b'))).unwrap();
        gate.cancel(Some(&owner('b'))).unwrap();
        assert_eq!(
            code(second.authorize_at(second.gate.lock().unwrap(), expires)),
            "cancelled"
        );
        assert_eq!(
            code(second.authorize_at(second.gate.lock().unwrap(), expires)),
            "lease-lost"
        );
    }

    #[test]
    fn a_persons_lease_can_expire_while_agents_remain_stopped() {
        use std::time::Duration;

        let start = Instant::now();
        let expires = start + super::super::GRANT_TTL;
        let gate = gate();
        gate.hold(true).unwrap();
        let person = gate
            .acquire_at(
                gate.lock().unwrap(),
                &owner('a'),
                Who::Person,
                start,
                || start,
            )
            .unwrap();
        gate.hold(true).unwrap();
        drop(
            person
                .authorize_at(
                    person.gate.lock().unwrap(),
                    expires - Duration::from_nanos(1),
                )
                .unwrap(),
        );
        assert_eq!(
            code(person.authorize_at(person.gate.lock().unwrap(), expires)),
            "lease-lost"
        );
        assert_eq!(gate.holder_at(gate.lock().unwrap(), expires).unwrap(), None);
        gate.revoke().unwrap();
        assert!(gate.held().unwrap());
        assert_eq!(
            code(gate.acquire_at(
                gate.lock().unwrap(),
                &owner('b'),
                Who::Agent,
                expires,
                || expires
            )),
            "stopped"
        );
        gate.close().unwrap();
        let error = gate
            .acquire_at(gate.lock().unwrap(), "invalid", Who::Agent, expires, || {
                expires
            })
            .err()
            .unwrap();
        assert_eq!(error.code, Code::Busy);
        assert_eq!(error.holder, None);
        assert_eq!(
            code(gate.acquire_at(
                gate.lock().unwrap(),
                &owner('b'),
                Who::Person,
                expires,
                || expires
            )),
            "busy"
        );
    }

    #[test]
    fn a_poisoned_gate_refuses_every_operation() {
        let now = Instant::now();
        let gate = gate();
        let grant = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        assert!(
            std::panic::catch_unwind(|| {
                let _state = gate.lock().unwrap();
                panic!("poison ownership");
            })
            .is_err()
        );

        for error in [
            gate.active().err().unwrap(),
            gate.holder().err().unwrap(),
            gate.acquire(&owner('b'), Who::Agent).err().unwrap(),
            gate.grant(&owner('a')).err().unwrap(),
            gate.renew(&owner('a')).err().unwrap(),
            grant.authorize().err().unwrap(),
            gate.cancel(None).err().unwrap(),
            gate.hold(true).err().unwrap(),
            gate.held().err().unwrap(),
            gate.revoke().err().unwrap(),
            gate.close().err().unwrap(),
        ] {
            assert_eq!(error.code, Code::Native);
            assert_eq!(error.error, "Input ownership failed");
        }
    }

    #[test]
    fn a_stale_token_cannot_end_a_newer_controller() {
        let now = Instant::now();
        let gate = gate();
        let _first = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();

        gate.cancel(Some(&owner('a'))).unwrap();
        let _second = gate
            .acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)
            .unwrap();
        assert_eq!(code(gate.cancel(Some(&owner('a')))), "lease-lost");
        assert!(gate.active_at(gate.lock().unwrap(), now).unwrap());

        gate.grant_at(gate.lock().unwrap(), &owner('b'), now)
            .unwrap();
        gate.cancel(Some(&owner('b'))).unwrap();
        assert!(!gate.active_at(gate.lock().unwrap(), now).unwrap());
    }

    #[test]
    fn nothing_is_granted_after_a_cancel() {
        let now = Instant::now();
        let gate = gate();
        let _acquired = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        let grant = gate
            .grant_at(gate.lock().unwrap(), &owner('a'), now)
            .unwrap();
        drop(grant.authorize_at(grant.gate.lock().unwrap(), now).unwrap());
        gate.cancel(None).unwrap();

        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('a'), now, || now)),
            "lease-lost",
            "a keep-alive is not told"
        );
        assert_eq!(
            code(gate.grant_at(gate.lock().unwrap(), &owner('b'), now)),
            "lease-lost",
            "nor a stranger"
        );
        assert_eq!(
            code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
            "cancelled",
            "the action under way is"
        );
        assert_eq!(
            code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
            "lease-lost",
            "once"
        );

        let _again = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        gate.revoke().unwrap();

        assert_eq!(
            code(gate.grant_at(gate.lock().unwrap(), &owner('a'), now)),
            "lease-lost",
            "nobody stopped it"
        );
    }

    #[test]
    fn a_stop_cancels_an_agents_action_and_refuses_agents_until_resumed() {
        let now = Instant::now();
        let gate = gate();
        let _acquired = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        let grant = gate
            .grant_at(gate.lock().unwrap(), &owner('a'), now)
            .unwrap();

        gate.hold(true).unwrap();
        assert!(gate.held().unwrap());
        assert!(!gate.active_at(gate.lock().unwrap(), now).unwrap());
        assert_eq!(
            code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
            "stopped",
            "what it was doing stops"
        );
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
            "stopped"
        );

        let person = gate
            .acquire_at(gate.lock().unwrap(), &owner('c'), Who::Person, now, || now)
            .unwrap();
        gate.hold(true).unwrap();
        assert_eq!(
            code(person.authorize_at(person.gate.lock().unwrap(), now)),
            "ok",
            "the person keeps control"
        );
        gate.cancel(Some(&owner('c'))).unwrap();

        gate.hold(false).unwrap();
        assert!(!gate.held().unwrap());
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
            "ok"
        );
    }

    #[test]
    fn an_input_asked_for_after_a_stop_hears_why_control_ended() {
        let now = Instant::now();
        let gate = gate();
        let _acquired = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();
        gate.hold(true).unwrap();
        assert_eq!(
            code(gate.grant_at(gate.lock().unwrap(), &owner('a'), now)),
            "stopped",
            "not merely lost"
        );
        assert_eq!(
            code(gate.grant_at(gate.lock().unwrap(), &owner('a'), now)),
            "lease-lost",
            "told once"
        );

        gate.hold(false).unwrap();
        let _again = gate
            .acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)
            .unwrap();
        gate.cancel(None).unwrap();
        gate.hold(true).unwrap();
        assert_eq!(
            code(gate.grant_at(gate.lock().unwrap(), &owner('b'), now)),
            "cancelled",
            "the cause is what ended it, not what holds now"
        );
    }

    #[test]
    fn a_closed_gate_stays_closed() {
        let now = Instant::now();
        let gate = gate();
        let grant = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();

        gate.close().unwrap();
        assert!(!gate.active_at(gate.lock().unwrap(), now).unwrap());
        assert_eq!(
            code(grant.authorize_at(grant.gate.lock().unwrap(), now)),
            "lease-lost"
        );
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
            "busy"
        );

        gate.cancel(None).unwrap();
        assert_eq!(
            code(gate.acquire_at(gate.lock().unwrap(), &owner('b'), Who::Agent, now, || now)),
            "busy",
            "for good"
        );
    }
}
