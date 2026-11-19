//! What the service keeps up whether or not anybody reads its status: it
//! follows the settings, ends control the helper can no longer use and
//! starts input once the helper logs in. A status read does all three, so
//! the service reads its own status whenever a period goes by with no
//! reader's, once for everyone, as the panel does while closed. Every other
//! desktop served is read on a schedule of its own, which reads of the
//! viewed one never put off, so Second Login is kept up while a VM is
//! viewed.
use super::Viewer;
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

/// How long the service lets go by with no status read before it reads one;
/// shorter in tests, which wait for real reads.
#[cfg(not(test))]
const PERIOD: Duration = Duration::from_secs(5);
#[cfg(test)]
const PERIOD: Duration = Duration::from_millis(200);

/// Keep `viewer` up until the task ends.
pub(super) async fn keep_up(viewer: Arc<Viewer>) {
    let reading = viewer.clone();
    let viewed = every_quiet_period(&viewer, move || {
        let viewer = reading.clone();
        async move {
            let _ = viewer.status_for(false).await;
        }
    });
    tokio::join!(viewed, keep_others_up_every_period(&viewer));
}

/// Read every desktop's status but the viewed one's each [`PERIOD`], one
/// round at a time, whatever reads of the viewed one come meanwhile: none
/// is read for them, so none puts these off.
async fn keep_others_up_every_period(viewer: &Arc<Viewer>) {
    let mut periods = tokio::time::interval(PERIOD);
    periods.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        periods.tick().await;
        viewer.keep_others_up().await;
    }
}

/// Call `read` whenever a [`PERIOD`] went by since the newest status read
/// began, one at a time: a reader's read puts the next one off.
async fn every_quiet_period<F: Future<Output = ()>>(viewer: &Viewer, mut read: impl FnMut() -> F) {
    loop {
        tokio::time::sleep_until(viewer.upkeep_due()).await;
        // A reader may have read while this one slept.
        if viewer.upkeep_due() <= Instant::now() {
            read().await;
        }
    }
}

impl Viewer {
    /// Read the status of every desktop served but the one viewed, all at
    /// once, each within the time a status read is given.
    pub(super) async fn keep_others_up(self: &Arc<Self>) {
        let until = Instant::now() + atlas_client::overview::READ_DEADLINE;
        let viewed = self.client();
        let mut reads = tokio::task::JoinSet::new();
        for desk in self.desks() {
            if Arc::ptr_eq(&desk.client, &viewed) {
                continue;
            }
            let viewer = self.clone();
            reads.spawn(async move {
                viewer.status_of(&desk.client, false, until).await;
            });
        }
        while reads.join_next().await.is_some() {}
    }

    /// A status read began now.
    pub(super) fn status_read(&self) {
        self.last_status.send_replace(Some(Instant::now()));
    }

    /// When the service reads its own status next: a period after the newest
    /// status read began, or now when none has.
    fn upkeep_due(&self) -> Instant {
        self.last_status
            .borrow()
            .map_or_else(Instant::now, |read| read + PERIOD)
    }
}

#[cfg(test)]
#[path = "upkeep_tests.rs"]
mod tests;
