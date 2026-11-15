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
    /// On a machine desktop, which generation of its session may act; none
    /// on the helper login, where the controller's own account is the
    /// boundary.
    authority: Option<Authority>,
}

/// Which generation of a machine desktop's session gives control now, and
/// until when; and the newest one retired for good, which never comes back.
#[derive(Default)]
struct Authority {
    /// The live generation, and its deadline in nanoseconds on the
    /// continuous clock.
    live: Option<(u64, u64)>,
    retired: u64,
}

impl Authority {
    /// Retire `generation` and every one before it.
    fn retire(&mut self, generation: u64) {
        self.retired = self.retired.max(generation);
        if self.live.is_some_and(|(live, _)| live <= generation) {
            self.live = None;
        }
    }

    /// Whether `generation` gives control at `now`; one past its deadline
    /// is retired for good.
    fn gives(&mut self, generation: Option<u64>, now: Option<u64>) -> bool {
        if let (Some((live, deadline)), Some(now)) = (self.live, now)
            && now >= deadline
        {
            self.retire(live);
        }
        match (self.live, generation, now) {
            (Some((live, _)), Some(generation), Some(_)) => live == generation,
            _ => false,
        }
    }
}

/// Now on the continuous clock, in nanoseconds: the clock a machine
/// desktop's daemon sets deadlines on. None when it cannot be read.
fn continuous() -> Option<u64> {
    let now = nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC).ok()?;
    let seconds = u64::try_from(now.tv_sec()).ok()?;
    let nanos = u64::try_from(now.tv_nsec()).ok()?;
    seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
}

struct Owner {
    token: String,
    who: Who,
    expires: Instant,
    status: Status,
    /// The machine session generation it was granted in.
    generation: Option<u64>,
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
        // On a machine desktop, only an owner of the live generation holds
        // control: every look at it sees the authority run out, the
        // service's tick too, so what it held is released.
        let generation = self.owner.as_ref().and_then(|owner| owner.generation);
        if let Some(authority) = &mut self.authority
            && !authority.gives(generation, continuous())
        {
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
    /// On a machine desktop the owner answers only a connection of its own
    /// generation, `asked`.
    fn require_at(
        &mut self,
        token: &str,
        asked: Option<u64>,
        acting: bool,
        now: Instant,
    ) -> Result<&mut Owner> {
        // Checked at every event, as each one takes this gate.
        self.expire_at(now);
        let machine = self.authority.is_some();
        if !self.closed
            && let Some(owner) = self
                .owner
                .as_mut()
                .filter(|owner| owner.token == token && (!machine || owner.generation == asked))
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

    #[cfg(test)]
    pub(super) fn acquire(self: &Arc<Self>, owner: &str, who: Who) -> Result<Grant> {
        self.acquire_in(owner, who, None)
    }

    /// [`Gate::acquire`] for a connection opened in a machine session's
    /// `generation`: only the live one gives control there.
    pub(super) fn acquire_in(
        self: &Arc<Self>,
        owner: &str,
        who: Who,
        generation: Option<u64>,
    ) -> Result<Grant> {
        self.acquire_at_in(
            self.lock()?,
            owner,
            who,
            generation,
            Instant::now(),
            Instant::now,
        )
    }

    #[cfg(test)]
    fn acquire_at(
        self: &Arc<Self>,
        state: MutexGuard<'_, State>,
        owner: &str,
        who: Who,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<Grant> {
        self.acquire_at_in(state, owner, who, None, now, deadline_now)
    }

    fn acquire_at_in(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        who: Who,
        generation: Option<u64>,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<Grant> {
        const UNAVAILABLE: &str = "The native input service is unavailable or already controlled";

        state.expire_at(now);
        if state.closed {
            return Err(Error::new(Code::Busy, UNAVAILABLE));
        }
        if let Some(authority) = &mut state.authority
            && !authority.gives(generation, continuous())
        {
            return Err(Error::new(
                Code::LeaseLost,
                "This desktop's session no longer gives control",
            ));
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
            generation,
        });
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
            generation,
        })
    }

    /// An input request is an action under way, so it hears Stop or cancel.
    #[cfg(test)]
    pub(super) fn grant(self: &Arc<Self>, owner: &str) -> Result<Grant> {
        self.grant_in(owner, None)
    }

    /// [`Gate::grant`], asked on a connection of a machine session's
    /// `generation`.
    pub(super) fn grant_in(
        self: &Arc<Self>,
        owner: &str,
        generation: Option<u64>,
    ) -> Result<Grant> {
        self.grant_at(self.lock()?, owner, generation, Instant::now())
    }

    fn grant_at(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        generation: Option<u64>,
        now: Instant,
    ) -> Result<Grant> {
        state.require_at(owner, generation, true, now)?;
        Ok(Grant {
            gate: self.clone(),
            owner: owner.to_owned(),
            generation,
        })
    }

    #[cfg(test)]
    pub(super) fn renew(&self, owner: &str) -> Result<()> {
        self.renew_in(owner, None)
    }

    /// [`Gate::renew`], asked on a connection of a machine session's
    /// `generation`.
    pub(super) fn renew_in(&self, owner: &str, generation: Option<u64>) -> Result<()> {
        self.renew_at(
            self.lock()?,
            owner,
            generation,
            Instant::now(),
            Instant::now,
        )
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
        self.renew_at(self.lock()?, owner, None, now, || now + longer)
    }

    fn renew_at(
        &self,
        mut state: MutexGuard<'_, State>,
        owner: &str,
        generation: Option<u64>,
        now: Instant,
        deadline_now: impl FnOnce() -> Instant,
    ) -> Result<()> {
        let owner = state.require_at(owner, generation, false, now)?;
        owner.expires = deadline_now() + atlas_core::session::GRANT_TTL;
        Ok(())
    }

    /// Cancellation deliberately does not sample expiry: an unobserved expiry
    /// still permits the action to hear who ended it, rather than mere loss.
    pub(super) fn cancel(&self, token: Option<&str>) -> Result<()> {
        self.cancel_in(token, None)
    }

    /// [`Gate::cancel`], asked on a connection of a machine session's
    /// `generation`: a token of another generation releases nothing.
    pub(super) fn cancel_in(&self, token: Option<&str>, generation: Option<u64>) -> Result<()> {
        let mut state = self.lock()?;
        let machine = state.authority.is_some();
        if let Some(token) = token
            && state.owner.as_ref().is_none_or(|owner| {
                owner.token != token || (machine && owner.generation != generation)
            })
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

    /// Whether an owner of `generation`, or of one before it, still has
    /// control.
    pub(super) fn owns_through(&self, generation: u64) -> Result<bool> {
        let state = self.lock()?;
        Ok(state.owner.as_ref().is_some_and(|owner| {
            owner.status == Status::Active
                && owner.generation.is_none_or(|owned| owned <= generation)
        }))
    }

    /// A gate for a machine desktop: no generation gives control until one
    /// is renewed.
    pub(super) fn machine() -> Self {
        Self(Mutex::new(State {
            authority: Some(Authority::default()),
            ..State::default()
        }))
    }

    /// `generation` gives control until `deadline`, as the daemon renews it.
    /// A newer generation retires the one before for good, ending its owner;
    /// one already retired, or a deadline already past, changes nothing.
    pub(super) fn renew_authority(&self, generation: u64, deadline: u64) -> Result<()> {
        let mut state = self.lock()?;
        let Some(authority) = &mut state.authority else {
            return Ok(());
        };
        let now = continuous();
        // One past its deadline is retired before anything renews it.
        authority.gives(None, now);
        let behind = authority.live.is_some_and(|(live, _)| generation < live);
        if generation <= authority.retired || behind || now.is_none_or(|now| now >= deadline) {
            return Ok(());
        }
        if let Some((live, _)) = authority.live
            && live < generation
        {
            authority.retire(live);
        }
        authority.live = Some((generation, deadline));
        let replaced = state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.generation.is_some_and(|owned| owned < generation));
        if replaced {
            state.end(Status::Lost);
        }
        Ok(())
    }

    /// `generation`'s authority is over, and every earlier one's: its owner
    /// loses control, and what it held is released as after an expiry.
    pub(super) fn revoke_authority(&self, generation: u64) -> Result<()> {
        let mut state = self.lock()?;
        let Some(authority) = &mut state.authority else {
            return Ok(());
        };
        authority.retire(generation);
        let ended = state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.generation.is_none_or(|owned| owned <= generation));
        if ended {
            state.end(Status::Lost);
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
    /// The machine session generation it was granted in.
    generation: Option<u64>,
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
        state.require_at(&self.owner, self.generation, true, now)?;
        Ok(state)
    }

    /// This grant's owner, when it was granted to the person: never another
    /// holder that came in since. It only looks, so a Stop or a release
    /// that landed meanwhile keeps its own refusal for the batch to meet.
    pub(super) fn person(&self) -> Option<&str> {
        let state = self.gate.lock().ok()?;
        let theirs = state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.token == self.owner && owner.who == Who::Person);
        theirs.then_some(self.owner.as_str())
    }
}

#[cfg(test)]
#[path = "lease_tests.rs"]
mod tests;
