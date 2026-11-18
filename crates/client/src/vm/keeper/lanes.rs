//! The agents' lanes the keeper carries now. Each request that carries an
//! agent's work to the guest takes a ticket first and holds it until its
//! lane is cancelled or ended and its credit is back. The person's
//! maintenance holds them all at once: from then no ticket is given, every
//! open one is told to stop, and the hold is answered only once the last of
//! them is gone. Whatever else comes to carry an agent's work joins by taking
//! a ticket of its own kind.
use atlas_core::files::Token;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};
use tokio::sync::{Notify, watch};

/// What an agent's lane carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Work {
    /// A read: a job's list or log, apps, windows, copies, elements.
    Read,
    /// A change: a run, a job's start or stop, input.
    Change,
    /// Files staged into the guest.
    Stage,
    /// A picture, a live picture or a file's read.
    Picture,
    /// A proof: the guest's hellos and its session proof.
    Observe,
}

/// What holds every agent's lane: the person's maintenance, by its ID and
/// the VM's name as its record says them, when they could be read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Hold {
    pub(super) change: Option<Token>,
    pub(super) name: Option<String>,
}

/// The lanes open now, and what holds them.
#[derive(Default)]
pub(super) struct Lanes {
    open: Mutex<Open>,
    /// Told whenever a ticket goes.
    gone: Notify,
}

#[derive(Default)]
struct Open {
    next: u64,
    lanes: BTreeMap<u64, (Work, watch::Sender<bool>)>,
    held: Option<Hold>,
}

/// One agent's lane admitted, until it is dropped.
pub(super) struct Ticket<'a> {
    lanes: &'a Lanes,
    id: u64,
    stopped: watch::Receiver<bool>,
}

impl Ticket<'_> {
    /// Whether a hold told this lane to stop.
    pub(super) fn is_stopped(&self) -> bool {
        *self.stopped.borrow()
    }

    /// Completes once a hold tells this lane to stop; never otherwise.
    pub(super) async fn stopped(&self) {
        let mut stopped = self.stopped.clone();
        if stopped.wait_for(|stopped| *stopped).await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.lanes.open().lanes.remove(&self.id);
        self.lanes.gone.notify_waiters();
    }
}

impl Lanes {
    fn open(&self) -> MutexGuard<'_, Open> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A ticket for a lane carrying `work`; what holds every lane instead,
    /// while one does.
    pub(super) fn admit(&self, work: Work) -> Result<Ticket<'_>, Hold> {
        let mut open = self.open();
        if let Some(held) = &open.held {
            return Err(held.clone());
        }
        let id = open.next;
        open.next += 1;
        let (stopping, stopped) = watch::channel(false);
        open.lanes.insert(id, (work, stopping));
        Ok(Ticket {
            lanes: self,
            id,
            stopped,
        })
    }

    /// What holds every lane now, if anything.
    pub(super) fn held(&self) -> Option<Hold> {
        self.open().held.clone()
    }

    /// Hold every lane for `hold`: none admitted from now, and every open
    /// one told to stop. How many of each work were open.
    pub(super) fn hold(&self, hold: Hold) -> BTreeMap<Work, usize> {
        let mut open = self.open();
        open.held = Some(hold);
        let mut told = BTreeMap::new();
        for (work, stopping) in open.lanes.values() {
            stopping.send_replace(true);
            *told.entry(*work).or_default() += 1;
        }
        told
    }

    /// Completes once no lane is open.
    pub(super) async fn drained(&self) {
        loop {
            let gone = self.gone.notified();
            tokio::pin!(gone);
            gone.as_mut().enable();
            if self.open().lanes.is_empty() {
                return;
            }
            gone.await;
        }
    }

    /// Let lanes be admitted again.
    pub(super) fn release(&self) {
        self.open().held = None;
    }

    /// How many lanes are open now.
    #[cfg(test)]
    pub(super) fn open_now(&self) -> usize {
        self.open().lanes.len()
    }
}

#[cfg(test)]
#[path = "lanes_tests.rs"]
mod tests;
