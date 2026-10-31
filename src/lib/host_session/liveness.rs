//! One immutable, bounded process sample for a session reply.
use super::{
    Instance,
    record::{ACTIVE_FOR, Record},
};
use crate::{process::State, single_flight::Shared};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) const BUDGET: usize = 128;
const FRESH: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, PartialEq, Eq)]
struct BatchKey {
    uid: u32,
    output: PathBuf,
    instances: Vec<Instance>,
}

static BATCH: Shared<Snapshot, BatchKey> = Shared::new(FRESH);

#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    states: HashMap<Instance, State>,
    sampled_at: Option<std::time::Instant>,
}

impl Snapshot {
    /// Priority order is preserved when limiting the batch; its cache key
    /// compares the full sorted identities, account and output directory.
    pub(crate) fn polled(output: &Path, instances: impl IntoIterator<Item = Instance>) -> Self {
        let instances = bounded(instances);
        let key = BatchKey {
            uid: nix::unistd::geteuid().as_raw(),
            output: output.to_owned(),
            instances: instances.clone(),
        };
        BATCH
            .get_blocking(key, WAIT, move || {
                Self::sample(instances, |instance| instance.state())
            })
            .unwrap_or_default()
            .trusted()
    }

    pub(crate) fn sample(
        instances: impl IntoIterator<Item = Instance>,
        state: impl Fn(Instance) -> State,
    ) -> Self {
        let began = std::time::Instant::now();
        Self {
            sampled_at: Some(began),
            states: bounded(instances)
                .into_iter()
                .map(|instance| (instance, state(instance)))
                .collect(),
        }
    }

    /// Expiry is checked once when acquiring a reply, before any fields
    /// are projected. A slow batch cannot refresh its earliest sample.
    fn trusted(self) -> Self {
        if self.sampled_at.is_some_and(|at| at.elapsed() < FRESH * 2) {
            self
        } else {
            Self::default()
        }
    }

    pub(crate) fn state(&self, instance: Instance) -> State {
        self.states
            .get(&instance)
            .copied()
            .unwrap_or(State::Unknown)
    }

    pub(crate) fn running(&self, instance: Instance) -> bool {
        matches!(self.state(instance), State::Running | State::Stopped)
    }

    pub(crate) fn lifetime(&self, record: &Record, now: u64, work: bool) -> Lifetime {
        if let Some(ended) = record.ended {
            return Lifetime {
                ended: Some(ended),
                live: false,
                active: false,
            };
        }
        let recent = now.saturating_sub(record.last_seen) <= crate::time::ms(ACTIVE_FOR);
        let (ended, live, active) = match record.host_process.map(|instance| self.state(instance)) {
            Some(State::Gone) if !work => (Some(record.last_seen), false, false),
            Some(State::Running | State::Stopped) => (None, true, recent || work),
            Some(_) => (None, work, work),
            None => (
                None,
                work || now.saturating_sub(record.last_seen)
                    <= crate::time::ms(crate::session_view::LIVE_FOR),
                recent || work,
            ),
        };
        Lifetime {
            ended,
            live,
            active,
        }
    }
}

fn bounded(instances: impl IntoIterator<Item = Instance>) -> Vec<Instance> {
    let mut kept = Vec::with_capacity(BUDGET);
    for instance in instances {
        if !kept.contains(&instance) {
            kept.push(instance);
            if kept.len() == BUDGET {
                break;
            }
        }
    }
    kept.sort_by_key(|instance| (instance.pid, instance.start));
    kept
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lifetime {
    pub ended: Option<u64>,
    pub live: bool,
    pub active: bool,
}

#[cfg(test)]
mod tests;
