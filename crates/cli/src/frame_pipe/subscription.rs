//! One observer's delivery flow: at most one credit, one picture in flight
//! and one acknowledged base, never a queue of pictures. A credit becomes a
//! delivery, the client decodes it, and only its commit, in time, makes it
//! the base that input may name. Every step names its credit and picture
//! exactly; the same request again is answered without effect, and any other
//! for a credit already settled ends the pair.
use atlas_core::frame_pipe::{
    diagnostics::Reason,
    model::{DropReason, Positive, Ref, ResyncReason},
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How long a credit may wait to be served before it expires.
pub(crate) const CREDIT_LIFE: Duration = Duration::from_millis(200);
/// How long a delivered picture may wait for its commit, drop or resync.
pub(crate) const COMMIT_LIFE: Duration = Duration::from_millis(500);
/// How many acknowledgements are remembered to answer their repeats.
const REMEMBERED: usize = 64;

/// A client's acknowledgement of one delivery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ack {
    Decoded(Ref),
    Commit(Ref),
    Drop(DropReason, Option<Ref>),
    Resync(ResyncReason),
}

/// Whether a step took effect, or repeated one that already had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Done,
    Repeat,
}

/// The credit outstanding and how far its delivery got.
#[derive(Debug)]
struct Delivery {
    credit: Positive,
    granted: Instant,
    /// The picture sent for it, once one was selected.
    sent: Option<Ref>,
    /// When the frame lane finished writing it.
    delivered: Option<Instant>,
    decoded: bool,
}

impl Delivery {
    /// Whether the picture was delivered and left unsettled past its life.
    fn overdue(&self, now: Instant) -> bool {
        self.delivered
            .is_some_and(|delivered| now - delivered >= COMMIT_LIFE)
    }
}

/// One acknowledgement remembered: its request, credit and content.
type Remembered = (u64, Positive, Ack);

/// One observer's subscription.
#[derive(Debug, Default)]
pub(crate) struct Subscription {
    subscribed: bool,
    last_credit: u64,
    delivery: Option<Delivery>,
    base: Option<Ref>,
    acknowledged: VecDeque<Remembered>,
}

impl Subscription {
    /// The picture the client committed last, which input may name.
    pub(crate) fn base(&self) -> Option<&Ref> {
        self.base.as_ref()
    }

    /// Start the flow; once per pair.
    ///
    /// # Errors
    ///
    /// `InvalidSchema` when it already began.
    pub(crate) fn subscribe(&mut self) -> Result<(), Reason> {
        if std::mem::replace(&mut self.subscribed, true) {
            return Err(Reason::InvalidSchema);
        }
        Ok(())
    }

    /// Start the flow again under a new profile: the delivery under way and
    /// the base are forgotten, as their pictures no longer fit; credits keep
    /// counting up.
    ///
    /// # Errors
    ///
    /// `InvalidSchema` before the flow first began.
    pub(crate) fn resubscribe(&mut self) -> Result<(), Reason> {
        if !self.subscribed {
            return Err(Reason::InvalidSchema);
        }
        self.delivery = None;
        self.base = None;
        Ok(())
    }

    /// Accept credit `credit`, granted at `now`.
    ///
    /// # Errors
    ///
    /// `InvalidSchema` before subscribing or for a credit no newer than the
    /// last, and `QueueOverflow` while another credit is outstanding.
    pub(crate) fn credit(&mut self, credit: Positive, now: Instant) -> Result<(), Reason> {
        if !self.subscribed || credit.get() <= self.last_credit {
            return Err(Reason::InvalidSchema);
        }
        if self.delivery.is_some() {
            return Err(Reason::QueueOverflow);
        }
        self.last_credit = credit.get();
        self.delivery = Some(Delivery {
            credit,
            granted: now,
            sent: None,
            delivered: None,
            decoded: false,
        });
        Ok(())
    }

    /// The credit to serve with `reference`, when one waits unexpired.
    pub(crate) fn select(&mut self, reference: Ref, now: Instant) -> Option<Positive> {
        let delivery = self
            .delivery
            .as_mut()
            .filter(|delivery| delivery.sent.is_none() && now - delivery.granted < CREDIT_LIFE)?;
        delivery.sent = Some(reference);
        Some(delivery.credit)
    }

    /// The picture for `credit` was fully written at `now`; its commit life
    /// starts here.
    pub(crate) fn delivered(&mut self, credit: Positive, now: Instant) {
        if let Some(delivery) = self.delivery.as_mut().filter(|delivery| {
            delivery.credit == credit && delivery.sent.is_some() && delivery.delivered.is_none()
        }) {
            delivery.delivered = Some(now);
        }
    }

    /// What lapsed by `now`: an unserved credit that expired, which is
    /// retired and returned for CreditExpired.
    ///
    /// # Errors
    ///
    /// `ClientStale` when a delivered picture waited past its commit life,
    /// which ends the pair.
    pub(crate) fn lapse(&mut self, now: Instant) -> Result<Option<Positive>, Reason> {
        let Some(delivery) = &self.delivery else {
            return Ok(None);
        };
        if delivery.overdue(now) {
            return Err(Reason::ClientStale);
        }
        if delivery.sent.is_none() && now - delivery.granted >= CREDIT_LIFE {
            let credit = delivery.credit;
            self.delivery = None;
            return Ok(Some(credit));
        }
        Ok(None)
    }

    /// Retire the outstanding credit without an acknowledgement, for an
    /// error that names it.
    pub(crate) fn retire(&mut self, credit: Positive) -> bool {
        let named = self
            .delivery
            .as_ref()
            .is_some_and(|delivery| delivery.credit == credit);
        if named {
            self.delivery = None;
        }
        named
    }

    /// Apply the client's `ack` for `credit`, made as request `request`, at
    /// `now`.
    ///
    /// # Errors
    ///
    /// `ClientStale` for one that came after the delivery's commit life,
    /// `WrongBase` for one that names another credit or picture, or
    /// conflicts with one already given, and `InvalidSchema` for one out of
    /// order. Each ends the pair.
    pub(crate) fn acknowledge(
        &mut self,
        request: u64,
        credit: Positive,
        ack: Ack,
        now: Instant,
    ) -> Result<Step, Reason> {
        let remembered = (request, credit, ack);
        if self.acknowledged.contains(&remembered) {
            return Ok(Step::Repeat);
        }
        let (_, _, ack) = &remembered;
        let delivery = self
            .delivery
            .as_mut()
            .filter(|delivery| delivery.credit == credit)
            .ok_or(Reason::WrongBase)?;
        if delivery.overdue(now) {
            return Err(Reason::ClientStale);
        }
        let sent = delivery.sent.as_ref().ok_or(Reason::InvalidSchema)?;
        let settles = match ack {
            Ack::Decoded(named) | Ack::Commit(named) if named != sent => {
                return Err(Reason::WrongBase);
            }
            Ack::Drop(_, Some(named)) if named != sent => return Err(Reason::WrongBase),
            // The frame lane's end and the control lane's Decoded race:
            // either may arrive first.
            Ack::Decoded(_) if !delivery.decoded => {
                delivery.decoded = true;
                false
            }
            Ack::Commit(named) if delivery.decoded => {
                self.base = Some(named.clone());
                true
            }
            Ack::Drop(..) => true,
            Ack::Resync(_) => {
                self.base = None;
                true
            }
            _ => return Err(Reason::InvalidSchema),
        };
        if settles {
            self.delivery = None;
        }
        if self.acknowledged.len() == REMEMBERED {
            self.acknowledged.pop_front();
        }
        self.acknowledged.push_back(remembered);
        Ok(Step::Done)
    }
}

#[cfg(test)]
mod tests;
