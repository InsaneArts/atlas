//! One observer's delivery flow: at most one credit, one picture in flight
//! and one acknowledged base, never a queue of pictures. A credit becomes a
//! delivery, the client decodes it, and only its commit makes it the base
//! that input may name. Every step names its credit and picture exactly;
//! the same acknowledgement again is answered without effect, and any other
//! for a credit already settled ends the pair.
use atlas_core::frame_pipe::{
    diagnostics::Reason,
    model::{Positive, Ref},
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
    Drop,
    Resync,
}

/// Whether a step took effect, or repeated one that already had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Done,
    Repeat,
}

#[derive(Debug)]
enum Stage {
    Waiting,
    Sending(Ref),
    Delivered { reference: Ref, at: Instant },
    Decoded { reference: Ref, at: Instant },
}

#[derive(Debug)]
struct Delivery {
    credit: Positive,
    granted: Instant,
    stage: Stage,
}

/// One observer's subscription.
#[derive(Debug, Default)]
pub(crate) struct Subscription {
    subscribed: bool,
    last_credit: u64,
    delivery: Option<Delivery>,
    base: Option<Ref>,
    acknowledged: VecDeque<(Positive, Ack)>,
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
            stage: Stage::Waiting,
        });
        Ok(())
    }

    /// The credit to serve with `reference`, when one waits unexpired.
    pub(crate) fn select(&mut self, reference: Ref, now: Instant) -> Option<Positive> {
        let delivery = self.delivery.as_mut()?;
        if !matches!(delivery.stage, Stage::Waiting) || now - delivery.granted >= CREDIT_LIFE {
            return None;
        }
        delivery.stage = Stage::Sending(reference);
        Some(delivery.credit)
    }

    /// The picture for `credit` was fully written at `now`.
    pub(crate) fn delivered(&mut self, credit: Positive, now: Instant) {
        if let Some(delivery) = self
            .delivery
            .as_mut()
            .filter(|delivery| delivery.credit == credit)
            && let Stage::Sending(reference) = &delivery.stage
        {
            delivery.stage = Stage::Delivered {
                reference: reference.clone(),
                at: now,
            };
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
        match delivery.stage {
            Stage::Waiting if now - delivery.granted >= CREDIT_LIFE => {
                let credit = delivery.credit;
                self.delivery = None;
                Ok(Some(credit))
            }
            Stage::Delivered { at, .. } | Stage::Decoded { at, .. } if now - at >= COMMIT_LIFE => {
                Err(Reason::ClientStale)
            }
            _ => Ok(None),
        }
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

    /// Apply the client's `ack` for `credit` at `now`.
    ///
    /// # Errors
    ///
    /// `WrongBase` for an acknowledgement that names another credit or
    /// picture, or conflicts with one already given, and `InvalidSchema`
    /// for one out of order. Either ends the pair.
    pub(crate) fn acknowledge(
        &mut self,
        credit: Positive,
        ack: Ack,
        now: Instant,
    ) -> Result<Step, Reason> {
        if self.acknowledged.contains(&(credit, ack.clone())) {
            return Ok(Step::Repeat);
        }
        let delivery = self
            .delivery
            .as_mut()
            .filter(|delivery| delivery.credit == credit)
            .ok_or(Reason::WrongBase)?;
        let sent = match &delivery.stage {
            Stage::Waiting => return Err(Reason::InvalidSchema),
            Stage::Sending(reference)
            | Stage::Delivered { reference, .. }
            | Stage::Decoded { reference, .. } => reference,
        };
        match (&ack, &delivery.stage) {
            // The frame lane's end and the control lane's Decoded race:
            // either may arrive first.
            (Ack::Decoded(named), Stage::Sending(_) | Stage::Delivered { .. }) => {
                if named != sent {
                    return Err(Reason::WrongBase);
                }
                let at = match delivery.stage {
                    Stage::Delivered { at, .. } => at,
                    _ => now,
                };
                delivery.stage = Stage::Decoded {
                    reference: named.clone(),
                    at,
                };
                self.remember(credit, ack);
                return Ok(Step::Done);
            }
            (Ack::Commit(named), Stage::Decoded { .. }) => {
                if named != sent {
                    return Err(Reason::WrongBase);
                }
                self.base = Some(named.clone());
            }
            (Ack::Drop, _) => {}
            (Ack::Resync, _) => self.base = None,
            _ => return Err(Reason::InvalidSchema),
        }
        self.delivery = None;
        self.remember(credit, ack);
        Ok(Step::Done)
    }

    fn remember(&mut self, credit: Positive, ack: Ack) {
        if self.acknowledged.len() == REMEMBERED {
            self.acknowledged.pop_front();
        }
        self.acknowledged.push_back((credit, ack));
    }
}

#[cfg(test)]
mod tests;
