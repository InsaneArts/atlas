//! Ownership is independent of execution: posting takes this gate briefly;
//! cancellation never waits behind a drag, text insertion or cleanup.
use atlas_core::error::{Code, Error, Result};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

pub(super) use atlas_core::error::Holder as Who;

/// Refuse `who` because `holder` has control: an agent finding the person is
/// told to wait for them, and anyone else hears `otherwise`.
pub(super) fn busy(holder: Who, who: Who, otherwise: &str) -> Error {
    let words = if holder == Who::Person && who == Who::Agent {
        atlas_core::error::PERSON_HAS_CONTROL
    } else {
        otherwise
    };
    Error::new(Code::Busy, words).held_by(holder)
}

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
        if matches!(&self.owner, Some(owner) if owner.expires <= now) {
            self.end(Status::Lost);
        }
    }

    /// End an active owner for this reason; an earlier end keeps its own.
    fn end(&mut self, status: Status) {
        if let Some(owner) = &mut self.owner
            && owner.status == Status::Active
        {
            owner.status = status;
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
        const UNAVAILABLE: &str = "The native input service is unavailable or already controlled";

        state.expire_at(now);
        if state.closed {
            return Err(Error::new(Code::Busy, UNAVAILABLE));
        }
        if state.held && who == Who::Agent {
            return Err(Error::stopped());
        }
        if let Some(holder) = state.holder() {
            return Err(busy(holder, who, UNAVAILABLE));
        }
        if atlas_core::files::Token::parse(owner).is_none() {
            return Err(Error::new(Code::Input, "Invalid native owner token"));
        }
        state.owner = Some(Owner {
            token: owner.to_owned(),
            who,
            expires: deadline_now() + atlas_core::session::GRANT_TTL,
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
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
        })
    }

    pub(super) fn renew(&self, owner: &str) -> Result<()> {
        self.renew_at(self.lock()?, owner, Instant::now(), Instant::now)
    }

    /// [`Gate::acquire`], its lease lasting `longer` past the usual one, so
    /// a test that is slow to run never sees it expire.
    #[cfg(test)]
    pub(super) fn acquire_lasting(
        self: &Arc<Self>,
        owner: &str,
        who: Who,
        longer: std::time::Duration,
    ) -> Result<Grant> {
        let now = Instant::now();
        self.acquire_at(self.lock()?, owner, who, now, || now + longer)
    }

    /// [`Gate::renew`], the lease lasting `longer` past the usual one.
    #[cfg(test)]
    pub(super) fn renew_lasting(&self, owner: &str, longer: std::time::Duration) -> Result<()> {
        let now = Instant::now();
        self.renew_at(self.lock()?, owner, now, || now + longer)
    }

    fn renew_at(
        &self,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<()> {
        let owner = state.require_at(owner, false, now)?;
        owner.expires = deadline_now() + atlas_core::session::GRANT_TTL;
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
        state.end(Status::Cancelled);
        Ok(())
    }

    pub(super) fn hold(&self, on: bool) -> Result<()> {
        let mut state = self.lock()?;
        state.held = on;
        if on && matches!(&state.owner, Some(owner) if owner.who == Who::Agent) {
            state.end(Status::Stopped);
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
        state.end(Status::Lost);
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

    /// This grant's owner, while that same owner still holds control and is
    /// the person: never another holder that came in since.
    pub(super) fn person(&self) -> Option<&str> {
        let state = self.authorize().ok()?;
        (state.holder() == Some(Who::Person)).then_some(self.owner.as_str())
    }
}

#[cfg(test)]
mod tests;
