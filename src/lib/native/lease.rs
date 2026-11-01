//! Ownership is independent of execution: posting takes this gate briefly;
//! cancellation never waits behind a drag, text insertion or cleanup.
use crate::error::{Code, Error, Result};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

pub(super) use crate::error::Holder as Who;
#[cfg(test)]
use crate::error::Holder;

#[derive(Default)]
pub(super) struct Gate(Mutex<State>);

#[derive(Default)]
pub(super) struct State {
    /// Retained after expiry and release, so only this token may clean up.
    owner: Option<Owner>,
    closed: bool,
    /// Stopping agents does not prevent the person from taking control.
    held: bool,
}

struct Owner {
    token: String,
    who: Who,
    expires: Instant,
    status: Status,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Active,
    Lost,
    Cancelled,
    Stopped,
}

impl State {
    fn expire_at(&mut self, now: Instant) {
        if let Some(owner) = &mut self.owner
            && owner.expires <= now
            && owner.status == Status::Active
        {
            owner.status = Status::Lost;
        }
    }

    fn holder(&self) -> Option<Who> {
        self.owner
            .as_ref()
            .filter(|owner| owner.status == Status::Active)
            .map(|owner| owner.who)
    }

    fn holder_at(&mut self, now: Instant) -> Option<Who> {
        self.expire_at(now);
        self.holder()
    }

    /// An action hears its cancellation once; renewal only hears lease loss.
    fn require_at(&mut self, token: &str, acting: bool, now: Instant) -> Result<&mut Owner> {
        self.expire_at(now);
        if !self.closed
            && let Some(owner) = self.owner.as_mut().filter(|owner| owner.token == token)
        {
            match owner.status {
                Status::Active => return Ok(owner),
                Status::Cancelled | Status::Stopped if acting => {
                    let status = owner.status;
                    owner.status = Status::Lost;
                    return Err(if status == Status::Stopped {
                        Error::stopped()
                    } else {
                        Error::new(Code::Cancelled, "Native control was stopped")
                    });
                }
                Status::Cancelled | Status::Stopped | Status::Lost => {}
            }
        }
        Err(Error::new(
            Code::LeaseLost,
            "Native control was released or expired",
        ))
    }
}

impl Gate {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.0
            .lock()
            .map_err(|_| Error::new(Code::Native, "Input ownership failed"))
    }

    pub(super) fn active(&self) -> Result<bool> {
        Ok(self.lock()?.holder_at(Instant::now()).is_some())
    }

    pub(super) fn holder(&self) -> Result<Option<Who>> {
        Ok(self.lock()?.holder_at(Instant::now()))
    }

    pub(super) fn acquire(self: &Arc<Self>, owner: &str, who: Who) -> Result<Grant> {
        self.acquire_at(self.lock()?, owner, who, Instant::now(), Instant::now)
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
        let holder = state.holder();
        if !state.closed && holder == Some(Who::Person) && who == Who::Agent {
            return Err(
                Error::new(Code::Busy, crate::error::PERSON_HAS_CONTROL).held_by(Who::Person)
            );
        }
        if state.closed || holder.is_some() {
            let error = Error::new(
                Code::Busy,
                "The native input service is unavailable or already controlled",
            );
            return Err(match holder.filter(|_| !state.closed) {
                Some(holder) => error.held_by(holder),
                None => error,
            });
        }
        if crate::files::Token::parse(owner).is_none() {
            return Err(Error::new(Code::Input, "Invalid native owner token"));
        }
        state.owner = Some(Owner {
            token: owner.to_owned(),
            who,
            expires: deadline_now() + super::GRANT_TTL,
            status: Status::Active,
        });
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    /// An input request is an action under way, so it hears Stop or cancel.
    pub(super) fn grant(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        self.grant_at(self.lock()?, owner, Instant::now())
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
        self.renew_at(self.lock()?, owner, Instant::now(), Instant::now)
    }

    fn renew_at(
        &self,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<()> {
        let owner = state.require_at(owner, false, now)?;
        owner.expires = deadline_now() + super::GRANT_TTL;
        Ok(())
    }

    /// Cancellation deliberately does not sample expiry: an unobserved expiry
    /// still permits the action to hear who ended it, rather than mere loss.
    pub(super) fn cancel(&self, token: Option<&str>) -> Result<()> {
        let mut state = self.lock()?;
        if let Some(token) = token
            && state
                .owner
                .as_ref()
                .is_none_or(|owner| owner.token != token)
        {
            return Err(Error::new(
                Code::LeaseLost,
                "This token cannot release another controller",
            ));
        }
        if let Some(owner) = &mut state.owner
            && owner.status == Status::Active
        {
            owner.status = Status::Cancelled;
        }
        Ok(())
    }

    pub(super) fn hold(&self, on: bool) -> Result<()> {
        let mut state = self.lock()?;
        state.held = on;
        if on
            && let Some(owner) = &mut state.owner
            && owner.who == Who::Agent
            && owner.status == Status::Active
        {
            owner.status = Status::Stopped;
        }
        Ok(())
    }

    pub(super) fn held(&self) -> Result<bool> {
        Ok(self.lock()?.held)
    }

    /// Session loss and action failure suppress any earlier cancellation.
    pub(super) fn revoke(&self) -> Result<()> {
        if let Some(owner) = &mut self.lock()?.owner {
            owner.status = Status::Lost;
        }
        Ok(())
    }

    pub(super) fn close(&self) -> Result<()> {
        let mut state = self.lock()?;
        state.closed = true;
        if let Some(owner) = &mut state.owner
            && owner.status == Status::Active
        {
            owner.status = Status::Lost;
        }
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
        self.authorize_at(self.gate.lock()?, Instant::now())
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

    fn cancelled(state: &State) -> Option<(String, bool)> {
        let owner = state.owner.as_ref()?;
        match owner.status {
            Status::Cancelled | Status::Stopped => {
                Some((owner.token.clone(), owner.status == Status::Stopped))
            }
            Status::Active | Status::Lost => None,
        }
    }

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
    fn reacquiring_the_same_token_replaces_its_prior_end_reason() -> Result<()> {
        let now = Instant::now();
        for status in [Status::Lost, Status::Cancelled, Status::Stopped] {
            let gate = gate();
            let token = owner('a');
            let old = gate.acquire_at(gate.lock()?, &token, Who::Agent, now, || now)?;
            match status {
                Status::Lost => gate.revoke()?,
                Status::Cancelled => gate.cancel(Some(&token))?,
                Status::Stopped => {
                    gate.hold(true)?;
                    gate.hold(false)?;
                }
                Status::Active => {}
            }
            let _new = gate.acquire_at(gate.lock()?, &token, Who::Agent, now, || now)?;
            assert_eq!(code(old.authorize_at(gate.lock()?, now)), "ok");
            let state = gate.lock()?;
            assert_eq!(cancelled(&state), None);
        }
        Ok(())
    }

    #[test]
    fn a_second_controller_is_busy() {
        let now = Instant::now();
        let gate = gate();
        assert_eq!(gate.lock().unwrap().holder_at(now), None);
        let _grant = gate
            .acquire_at(gate.lock().unwrap(), &owner('a'), Who::Agent, now, || now)
            .unwrap();

        assert!(gate.lock().unwrap().holder_at(now).is_some());
        assert_eq!(gate.lock().unwrap().holder_at(now), Some(Holder::Agent));
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

        assert_eq!(gate.lock().unwrap().holder_at(now), Some(Holder::Person));
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

        assert!(gate.lock().unwrap().holder_at(now).is_none());
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
                gate.lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .map(|owner| owner.expires),
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
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
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
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
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
            assert_eq!(
                gate.lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .map(|owner| owner.expires),
                Some(expires)
            );
            (gate, grant)
        };

        for (now, valid) in [
            (expires - Duration::from_nanos(1), true),
            (expires, false),
            (expires + Duration::from_nanos(1), false),
        ] {
            let (gate, _) = acquired();
            assert_eq!(gate.lock().unwrap().holder_at(now).is_some(), valid);

            let (gate, _) = acquired();
            assert_eq!(
                gate.lock().unwrap().holder_at(now),
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
                gate.lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .map(|owner| owner.expires),
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
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
            Some(deadline)
        );
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('b'), renewed, || renewed)),
            "lease-lost"
        );
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
            Some(deadline)
        );
        drop(
            first
                .authorize_at(first.gate.lock().unwrap(), start + Duration::from_secs(6))
                .unwrap(),
        );
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
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
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
            Some(deadline)
        );
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('a'), acquired, || acquired)),
            "lease-lost"
        );
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
            Some(deadline)
        );
        assert_eq!(
            code(gate.renew_at(gate.lock().unwrap(), &owner('b'), deadline, || deadline)),
            "lease-lost"
        );
        assert_eq!(
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
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
            assert_eq!(
                gate.lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .map(|owner| owner.expires),
                Some(expires)
            );
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('b'), expires)),
                "lease-lost"
            );
            assert_eq!(
                cancelled(&gate.lock().unwrap()),
                Some((owner('a'), stopped))
            );
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                if stopped { "stopped" } else { "cancelled" }
            );
            assert_eq!(cancelled(&gate.lock().unwrap()), None);
            assert_eq!(
                code(grant.authorize_at(grant.gate.lock().unwrap(), expires)),
                "lease-lost"
            );
            assert_eq!(
                code(gate.grant_at(gate.lock().unwrap(), &owner('a'), expires)),
                "lease-lost"
            );
            assert_eq!(
                gate.lock()
                    .unwrap()
                    .owner
                    .as_ref()
                    .map(|owner| owner.expires),
                Some(expires)
            );
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
                    assert!(gate.lock().unwrap().holder_at(expires).is_none());
                }
                if stopped {
                    gate.hold(true).unwrap();
                } else {
                    gate.cancel(Some(&owner('a'))).unwrap();
                }
                assert_eq!(
                    gate.lock()
                        .unwrap()
                        .owner
                        .as_ref()
                        .map(|owner| owner.expires),
                    Some(expires)
                );
                assert_eq!(
                    gate.lock()
                        .unwrap()
                        .owner
                        .as_ref()
                        .map(|owner| owner.token.as_str()),
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
                gate.lock().unwrap().holder_at(now).is_some(),
                ending == "acquire"
            );
            assert_eq!(
                cancelled(&gate.lock().unwrap()),
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
            gate.lock()
                .unwrap()
                .owner
                .as_ref()
                .map(|owner| owner.expires),
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
        assert_eq!(gate.lock().unwrap().holder_at(expires), None);
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
        assert!(gate.lock().unwrap().holder_at(now).is_some());

        gate.grant_at(gate.lock().unwrap(), &owner('b'), now)
            .unwrap();
        gate.cancel(Some(&owner('b'))).unwrap();
        assert!(gate.lock().unwrap().holder_at(now).is_none());
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
        assert!(gate.lock().unwrap().holder_at(now).is_none());
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
        assert!(gate.lock().unwrap().holder_at(now).is_none());
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
