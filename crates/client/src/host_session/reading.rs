use crate::{
    activity_log,
    host_session::{
        Key,
        liveness::{Lifetime, Snapshot},
        names::{name_of, names_with_left},
        record::Seen,
        store::Store,
    },
    session_view::{self, Scene, Who},
};
use atlas_control::Controller;
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// What a status says that the sessions' states depend on.
#[derive(Clone, Copy, Debug)]
pub struct SessionFacts {
    /// Whether an outcome is unknown.
    pub(crate) pending: bool,
    /// Whether the input service says an agent, not the person, holds
    /// control.
    pub(crate) agent_holds: bool,
    /// Now, in nanoseconds since 1970.
    pub(crate) now: u128,
}

/// The records and bounded note candidates for one frozen reply.
pub struct SessionReading {
    pub seen: Arc<[Seen]>,
    pub(crate) rows: Vec<crate::activity_log::Row>,
    pub(crate) causes: Vec<atlas_control::Cause>,
    pub(crate) holder: Option<atlas_control::Note>,
    pub(crate) waiting: Vec<atlas_control::Note>,
    pub(crate) running: Vec<atlas_control::Note>,
    pub processes: Snapshot,
}

/// How many recent activity rows each session's state and latest action
/// are read from.
pub(crate) const SESSION_ROWS: usize = 64;

/// The live notes, the holder's first, then the waiting commands' and the
/// running commands', naming a session that has no record among `seen`:
/// each such session once.
pub(crate) fn unrecorded<'a>(
    holder: Option<&'a atlas_control::Note>,
    waiting: &'a [atlas_control::Note],
    running: &'a [atlas_control::Note],
    seen: &[Seen],
) -> Vec<&'a atlas_control::Note> {
    let notes = holder.into_iter().chain(waiting).chain(running);
    let mut found: Vec<&atlas_control::Note> = Vec::new();
    for note in notes {
        let Some(key) = &note.session else { continue };
        let known = seen.iter().any(|seen| seen.record.key == *key)
            || found.iter().any(|kept| kept.session.as_ref() == Some(key));
        if !known {
            found.push(note);
        }
    }
    found
}

/// Whether a live holder, waiting or running note names a session with no
/// record yet among `seen`: a session on its first command.
pub fn first_command_running(
    output: Option<&Path>,
    controller: &Controller,
    seen: &[Seen],
) -> bool {
    SessionReading::collect(output, controller).is_some_and(|reading| {
        !unrecorded(
            reading.holder.as_ref(),
            &reading.waiting,
            &reading.running,
            seen,
        )
        .is_empty()
    })
}

impl SessionReading {
    pub fn collect(output: Option<&Path>, controller: &Controller) -> Option<Self> {
        Self::collect_with(output, controller, Snapshot::polled)
    }

    pub(crate) fn collect_with(
        output: Option<&Path>,
        controller: &Controller,
        sample: impl FnOnce(&std::path::Path, Vec<atlas_core::process::Instance>) -> Snapshot,
    ) -> Option<Self> {
        let output = output?;
        let seen = Store::at(output).recent();
        let rows = activity_log::recent(output, SESSION_ROWS);
        let causes = controller
            .pending_causes_within(std::time::Duration::from_millis(50))
            .unwrap_or_default();
        let holder = controller.holder_note();
        let waiting = controller.waiting_candidates();
        let running = controller.running_candidates();
        let mut reading = Self {
            seen,
            rows,
            causes,
            holder,
            waiting,
            running,
            processes: Snapshot::default(),
        }
        .sample(output, sample);
        reading.processes.keep_live(
            &mut reading.holder,
            &mut reading.waiting,
            &mut reading.running,
        );
        Some(reading)
    }

    pub(crate) fn sample(
        mut self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<atlas_core::process::Instance>) -> Snapshot,
    ) -> Self {
        let instances = self
            .holder
            .iter()
            .chain(&self.waiting)
            .chain(&self.running)
            .map(|note| note.process)
            .chain(self.seen.iter().filter_map(|seen| seen.record.host_process))
            .collect();
        self.processes = sample(output, instances);
        self
    }

    pub(crate) fn scene(&self, facts: SessionFacts) -> Scene<'_> {
        Scene {
            rows: &self.rows,
            processes: &self.processes,
            holder: self.holder.as_ref(),
            waiting: &self.waiting,
            running: &self.running,
            causes: &self.causes,
            pending: facts.pending,
            agent_holds: facts.agent_holds,
            now: facts.now,
        }
    }

    pub fn work(&self, key: &Key, facts: SessionFacts) -> bool {
        self.scene(facts).work(&Who::Host(key.clone()))
    }
}

/// A frozen session selection shared by display and service replacement.
pub(crate) struct Listed<'a> {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) ended: Option<u64>,
    pub(crate) source: Source<'a>,
    pub(crate) last_seen: u64,
    pub(crate) shown: session_view::Shown,
    pub(crate) running_command: bool,
}

pub(crate) enum Source<'a> {
    Recorded(&'a Seen, usize, Lifetime),
    First(&'a atlas_control::Note),
    Terminal { started: u64, now: u64 },
}

impl SessionReading {
    pub(crate) fn live(&self, facts: SessionFacts) -> Vec<Listed<'_>> {
        let seen = &self.seen;
        let causes = &self.causes;
        let holder = &self.holder;
        let waiting = &self.waiting;
        let running = &self.running;
        let scene = self.scene(facts);
        let command_runs = |key: Option<&Key>| {
            causes
                .iter()
                .any(|cause| cause.command && cause.session.as_ref() == key)
        };
        let now_ms = u64::try_from(facts.now / 1_000_000).unwrap_or(u64::MAX);
        let mut unknown: BTreeMap<&Key, usize> = BTreeMap::new();
        for cause in causes.iter().filter(|cause| !cause.running) {
            if let Some(session) = &cause.session {
                *unknown.entry(session).or_default() += 1;
            }
        }
        // A session whose record has not been written is named by its command's
        // note meanwhile, by the same rule as a record.
        let first = unrecorded(holder.as_ref(), waiting, running, seen);
        let named = names_with_left(
            seen.iter().map(|seen| &seen.record),
            [],
            first.iter().copied(),
        );

        let mut listed: Vec<Listed> = seen
            .iter()
            .filter_map(|seen| {
                let record = &seen.record;
                let uncertain = unknown.get(&record.key).copied().unwrap_or(0);
                let (shown, lifetime) = scene.project(record);
                shown.relevant().then(|| Listed {
                    key: record.key.to_string(),
                    name: name_of(&named, &record.key, Some(record.host)),
                    ended: lifetime.ended,
                    source: Source::Recorded(seen, uncertain, lifetime),
                    last_seen: record.last_seen,
                    running_command: command_runs(Some(&record.key)),
                    shown,
                })
            })
            .collect();
        for note in first {
            let Some(key) = &note.session else { continue };
            listed.push(Listed {
                key: key.to_string(),
                name: name_of(&named, key, note.host),
                ended: None,
                source: Source::First(note),
                last_seen: note.since,
                shown: scene.shown(&Who::Host(key.clone()), false),
                running_command: command_runs(Some(key)),
            });
        }
        if let Some((started, last_seen)) = scene.terminal() {
            listed.push(Listed {
                key: String::new(),
                name: session_view::TERMINAL.to_owned(),
                ended: None,
                source: Source::Terminal {
                    started,
                    now: now_ms,
                },
                last_seen,
                shown: scene.shown(&Who::Terminal, false),
                running_command: command_runs(None),
            });
        }
        listed.sort_by_key(|listed| (listed.shown.state, std::cmp::Reverse(listed.last_seen)));
        listed
    }
}
