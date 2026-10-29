//! The person's control through a viewer: a [`Lease`] names the viewer
//! that holds it by a token that runs out unless renewed, and keeps the
//! controller lock until every input and release under it has settled. An
//! [`ActionPermit`] marks one input under way.
use super::{Controller, Ownership};
use crate::{
    error::{Code, Error, Result},
    files::token,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// The person's control of one helper through a viewer. A token names the
/// viewer that holds it and lasts its time to live from when it was taken
/// or last renewed. The controller lock is kept from the acquire until no
/// token is live, no input is in flight and no release is under way, so
/// control never ends while an input or its cleanup can still reach the
/// helper.
pub struct Lease {
    /// The token of the viewer holding control, until it is released or
    /// runs out. Only a caller presenting it may act or renew.
    pub(super) owner: Option<String>,
    /// The token of a lease that ran out, kept until its release begins, so
    /// that release can name it and control ends only once it cleaned up.
    pub(super) retired: Option<String>,
    /// The controller lock, held while there is a token, an input in flight
    /// or a release under way, and only then.
    descriptor: Option<Ownership>,
    /// When the owner's token runs out unless renewed.
    pub(super) expires: Instant,
    /// Whether an [`ActionPermit`] is out: never more than one at a time.
    pub(super) inflight: bool,
    /// Whether a release, or a token running out, has begun and not ended.
    cleaning: bool,
    /// How long a token lasts from when it was taken or renewed.
    ttl: Duration,
}

impl Default for Lease {
    fn default() -> Self {
        Self::new(Duration::from_secs(10))
    }
}

impl Lease {
    /// No control yet, with tokens that last `ttl`.
    pub fn new(ttl: Duration) -> Self {
        Self {
            owner: None,
            retired: None,
            descriptor: None,
            expires: Instant::now(),
            inflight: false,
            cleaning: false,
            ttl,
        }
    }

    /// Retire the owner's token once it ran out, beginning its release, and
    /// let go of the controller lock once nothing keeps it: no token, no
    /// input in flight and no release under way.
    pub fn expire(&mut self) {
        if self.owner.is_some() && Instant::now() >= self.expires {
            self.retired = self.owner.take();
            self.cleaning = true;
        }

        if self.owner.is_none() && !self.inflight && !self.cleaning {
            self.descriptor = None;
        }
    }

    /// Take control through `controller` for a new token, refused while the
    /// lock is still kept for another viewer or for one still settling. The
    /// token, which the viewer presents to act, renew and release.
    pub fn acquire(&mut self, controller: &Controller) -> Result<String> {
        self.expire();

        if self.descriptor.is_some() {
            return Err(Error::new(Code::Busy, "Another viewer has control"));
        }

        self.descriptor = Some(controller.acquire_for_person()?);
        let token = token();
        self.owner = Some(token.clone());
        self.expires = Instant::now() + self.ttl;

        Ok(token)
    }

    /// Refuse with `lease-lost` unless `token` is the live token: one that
    /// was released or ran out never acts again.
    pub fn require(&mut self, token: &str) -> Result<()> {
        self.expire();

        // Tokens are fixed-length random strings; compare without an early byte exit.
        let matches = self.owner.as_ref().is_some_and(|owner| {
            owner.len() == token.len()
                && owner
                    .bytes()
                    .zip(token.bytes())
                    .fold(0u8, |v, (a, b)| v | (a ^ b))
                    == 0
        });
        if !matches {
            return Err(Error::new(
                Code::LeaseLost,
                "Control was released or expired. Take control again.",
            ));
        }

        Ok(())
    }

    /// [`Lease::require`] `token`, then give it its full time to live again.
    pub fn renew(&mut self, token: &str) -> Result<()> {
        self.require(token)?;
        self.expires = Instant::now() + self.ttl;
        Ok(())
    }

    /// End the lease at once, the live and the retired token alike, with
    /// no release left to clean up: the lock goes unless an input is still
    /// in flight, and then goes with it.
    pub fn release(&mut self) {
        self.owner = None;
        self.retired = None;
        self.cleaning = false;
        self.expire();
    }

    /// Undo an acquire for `owner` that did not go through, unless `owner`
    /// is no longer the live token, an input is in flight or a release has
    /// begun: a late failure never ends a later viewer's control.
    pub fn abandon_acquire(&mut self, owner: &str) {
        if self.require(owner).is_ok() && !self.inflight && !self.cleaning {
            self.release();
        }
    }

    /// Begin ending control: the live or retired token, which `requested`
    /// must name when given, is taken and answered, and the lock stays
    /// held until [`Lease::end_release`]. None when there is nothing left
    /// to release.
    pub fn begin_release(&mut self, requested: Option<&str>) -> Result<Option<String>> {
        self.expire();
        let current = self.owner.as_ref().or(self.retired.as_ref());

        // Releasing a lease that already ended is a no-op; a token can
        // never end a different controller's lease.
        if requested.is_some() && current.is_none() {
            return Ok(None);
        }

        if requested.is_some() && requested != current.map(String::as_str) {
            return Err(Error::new(
                Code::LeaseLost,
                "This token cannot release another controller",
            ));
        }

        // Both the live and any retired token end here.
        let retired = self.retired.take();
        let previous = self.owner.take().or(retired);
        if previous.is_some() {
            self.cleaning = true;
        }

        Ok(previous)
    }

    /// The release [`Lease::begin_release`] began has cleaned up: the lock
    /// goes unless an input in flight still keeps it.
    pub fn end_release(&mut self) {
        self.cleaning = false;
        self.expire();
    }

    /// Whether the controller lock is still kept for this lease.
    pub fn active(&mut self) -> bool {
        self.expire();
        self.descriptor.is_some()
    }

    /// Whether a token ran out and its release has not begun yet.
    pub fn expired(&self) -> bool {
        self.retired.is_some()
    }
}

/// A [`Lease`] as the viewer's tasks share it.
pub type SharedLease = Arc<Mutex<Lease>>;

/// Holds the controller flock until an acknowledged or uncertain operation settles.
pub struct ActionPermit {
    lease: SharedLease,
}

impl ActionPermit {
    /// Mark an input under way for the holder of `token`, refused unless it
    /// is the live token and no other input is in flight. The lock stays
    /// held until the permit is dropped.
    pub async fn begin(lease: SharedLease, token: &str) -> Result<Self> {
        {
            let mut state = lease.lock().await;
            state.require(token)?;

            if state.inflight {
                return Err(Error::new(
                    Code::Busy,
                    "Wait for the current input to finish",
                ));
            }

            state.inflight = true;
        }

        Ok(Self { lease })
    }
}

impl Drop for ActionPermit {
    fn drop(&mut self) {
        let finish = |state: &mut Lease| {
            state.inflight = false;
            state.expire();
        };

        if let Ok(mut state) = self.lease.try_lock() {
            finish(&mut state);
        } else if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let lease = self.lease.clone();
            runtime.spawn(async move { finish(&mut *lease.lock().await) });
        } else {
            // Outside any runtime, waiting here blocks no async task.
            finish(&mut self.lease.blocking_lock());
        }
    }
}
