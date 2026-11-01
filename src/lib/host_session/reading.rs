use crate::{
    activity_log,
    client::Client,
    host_session::{Key, liveness::Snapshot, record::Seen, store::Store},
    session_view::{Scene, Who},
};
use std::sync::Arc;

/// What a status says that the sessions' states depend on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SessionFacts {
    /// Whether an outcome is unknown.
    pub(crate) pending: bool,
    /// Whether the input service says an agent, not the person, holds
    /// control.
    pub(crate) agent_holds: bool,
    /// Now, in nanoseconds since 1970.
    pub(crate) now: u128,
}

/// The records and bounded note candidates for one frozen reply.
pub(crate) struct SessionReading {
    pub(crate) seen: Arc<[Seen]>,
    pub(crate) rows: Vec<crate::activity_log::Row>,
    pub(crate) causes: Vec<crate::control::Cause>,
    pub(crate) holder: Option<crate::control::Note>,
    pub(crate) waiting: Vec<crate::control::Note>,
    pub(crate) running: Vec<crate::control::Note>,
    pub(crate) processes: Snapshot,
}

/// How many recent activity rows each session's state and latest action
/// are read from.
pub(crate) const SESSION_ROWS: usize = 64;

/// The live notes, the holder's first, then the waiting commands' and the
/// running commands', naming a session that has no record among `seen`:
/// each such session once.
pub(crate) fn unrecorded<'a>(
    holder: Option<&'a crate::control::Note>,
    waiting: &'a [crate::control::Note],
    running: &'a [crate::control::Note],
    seen: &[Seen],
) -> Vec<&'a crate::control::Note> {
    let notes = holder.into_iter().chain(waiting).chain(running);
    let mut found: Vec<&crate::control::Note> = Vec::new();
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
pub(crate) fn first_command_running(client: &Client, seen: &[Seen]) -> bool {
    SessionReading::collect(client).is_some_and(|reading| {
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
    pub(crate) fn collect(client: &Client) -> Option<Self> {
        Self::collect_with(client, Snapshot::polled)
    }

    pub(crate) fn collect_with(
        client: &Client,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Option<Self> {
        let output = client.output_dir().ok()?;
        let seen = Store::at(output).recent();
        let rows = activity_log::recent(output, SESSION_ROWS);
        let causes = client
            .controller
            .pending_causes_within(std::time::Duration::from_millis(50))
            .unwrap_or_default();
        let holder = client.controller.holder_note();
        let waiting = client.controller.waiting_candidates();
        let running = client.controller.running_candidates();
        Some(
            Self {
                seen,
                rows,
                causes,
                holder,
                waiting,
                running,
                processes: Snapshot::default(),
            }
            .sampled(output, sample),
        )
    }

    pub(crate) fn sample(
        mut self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
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

    pub(crate) fn sampled_for_service_change(
        self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Self {
        let mut reading = self.sample(output, sample);
        let remains = |note: &crate::control::Note| {
            reading.processes.state(note.process) != crate::process::State::Gone
        };
        reading.holder = reading.holder.filter(remains);
        reading.waiting.retain(remains);
        reading.running.retain(remains);
        reading
    }

    pub(crate) fn sampled(
        mut self,
        output: &std::path::Path,
        sample: impl FnOnce(&std::path::Path, Vec<crate::process::Instance>) -> Snapshot,
    ) -> Self {
        self = self.sample(output, sample);
        self.holder = self
            .holder
            .filter(|note| self.processes.running(note.process));
        self.waiting
            .retain(|note| self.processes.state(note.process) == crate::process::State::Running);
        self.running
            .retain(|note| self.processes.running(note.process));
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

    pub(crate) fn work(&self, key: &Key, facts: SessionFacts) -> bool {
        self.scene(facts).work(&Who::Host(key.clone()))
    }
}
