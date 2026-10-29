//! Supervision: about once a second the daemon looks at every job, ends
//! what has ended, stops what must stop (its time is up, the person stopped
//! agents, the helper came on screen), carries stops on from TERM to KILL,
//! notes descendants to report what left a group, keeps logs within their
//! limit, and prunes old records now and then.
use super::{
    Jobs, State,
    observe::{as_left, left, observe},
    record::{Ended, Record, Stopping},
    signal::Signalled,
    store::Found,
    verdict::Verdict,
};
use crate::{
    daemon::{
        protocol::{How, JobState, Left},
        run,
    },
    host_session::now_ms,
    process::Info,
};
use nix::sys::signal::Signal;
use std::{
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// How often old records are pruned.
const PRUNE_EVERY: Duration = Duration::from_secs(60 * 60);
/// How long KILL has, once sent, before a job is said to be stuck.
const KILLED: Duration = Duration::from_secs(2);
/// How often KILL is sent again to a stuck job, and for how long.
const KILL_AGAIN: Duration = Duration::from_secs(5);
const KILL_FOR: Duration = Duration::from_secs(60);
/// How often jobs are looked at while one is stopping, and otherwise.
const WATCH_STOPPING: Duration = Duration::from_millis(200);
const WATCH: Duration = Duration::from_secs(1);
/// The grace a stop that no client asked for gives.
pub(super) const GRACE_MS: u64 = 5_000;

impl Jobs {
    /// Look at every job once: end what has ended, stop what ran past its
    /// time, everything once the person has stopped agents (`stopped`) or
    /// the helper is on screen (`on_console`), carry stops on, keep logs
    /// within their limit, and prune old records now and then. What a job
    /// is, is read from its record each time, never from a count.
    pub fn watch_once(&self, stopped: bool, on_console: bool) {
        let mut state = self.state();
        let now = now_ms();
        let records = state.store.records();
        let processes = self.kernel.of_user(self.uid).ok();
        for read in records {
            let Found::Record(mut record) = read else {
                continue;
            };
            if record.state == JobState::Launched && !state.launching.contains(&record.id) {
                self.found_launched(&mut state, &mut record, processes.as_deref());
            }
            self.look(
                &mut state,
                &mut record,
                processes.as_deref(),
                stopped,
                on_console,
                now,
            );
            if state.store.bound_log(&record.id, self.log_limit) {
                record.log_resets = record.log_resets.saturating_add(1);
                state.store.put(&record);
            }
        }
        self.count(&mut state);
        if state
            .pruned
            .is_none_or(|pruned| pruned.elapsed() >= PRUNE_EVERY)
        {
            state.store.prune(now);
            state.pruned = Some(Instant::now());
        }
    }

    /// How long until the next look: sooner while a job is stopping.
    pub fn interval(&self) -> Duration {
        if self.state().stopping {
            WATCH_STOPPING
        } else {
            WATCH
        }
    }

    /// Count again, from the records, the jobs that may still run and
    /// whether one is stopping.
    pub(super) fn count(&self, state: &mut State) {
        let (mut running, mut stopping, mut unresolved) = (0, false, 0);
        for read in state.store.records() {
            match read {
                // A job still launching is counted as launching.
                Found::Record(record) if state.launching.contains(&record.id) => {}
                Found::Unreadable(id) if state.launching.contains(&id) => {}
                Found::Record(record) => {
                    running += usize::from(record.live());
                    stopping |= record.state == JobState::Stopping;
                }
                Found::Unreadable(_) => unresolved += 1,
            }
        }
        state.running = running;
        state.stopping = stopping;
        state.unresolved = unresolved;
    }

    /// Look at one job: stop a running one when it must stop, note its
    /// descendants, end it once nothing of it is left, and carry a stop on.
    pub(super) fn look(
        &self,
        state: &mut State,
        record: &mut Record,
        processes: Option<&[Info]>,
        stopped: bool,
        on_console: bool,
        now: u64,
    ) {
        match record.state {
            JobState::Running => {
                let why = if on_console {
                    Some(How::OnConsole)
                } else if stopped {
                    Some(How::Person)
                } else if now >= record.ends_at {
                    Some(How::Timeout)
                } else {
                    None
                };
                if let Some(why) = why {
                    return self.begin_stop(state, record, why, GRACE_MS, processes, now);
                }
                let (verdict, _) = self.judge(state, record, None);
                if !self.settle(state, record, &verdict, How::Exited, processes)
                    && matches!(verdict, Verdict::Signal { .. })
                {
                    let before = record.seen.len();
                    note_seen(record, processes, self);
                    if record.seen.len() != before {
                        state.store.put(record);
                    }
                }
            }
            JobState::Stopping => self.escalate(state, record, processes, now),
            _ => {}
        }
    }

    /// Settle a job whose record says launched while no start is launching
    /// it: run on when it is still the job, ended when it is not, and
    /// otherwise shown as it is and never signalled.
    pub(super) fn found_launched(
        &self,
        state: &mut State,
        record: &mut Record,
        processes: Option<&[Info]>,
    ) {
        let (verdict, _) = self.judge(state, record, None);
        if let Verdict::Signal { .. } = verdict {
            record.state = JobState::Running;
            record.adopted |= !state.children.contains_key(&record.id);
            state.store.put(record);
        } else {
            self.settle(state, record, &verdict, How::Gone, processes);
        }
    }

    /// Begin stopping `record` for `why`. A fresh look decides: a job that
    /// has ended is ended; one whose processes do not match it, or cannot be
    /// read, is marked stopping with nothing sent, and is looked at again;
    /// otherwise the stop is kept first, then TERM is sent, after another
    /// fresh look.
    pub(super) fn begin_stop(
        &self,
        state: &mut State,
        record: &mut Record,
        why: How,
        grace_ms: u64,
        processes: Option<&[Info]>,
        now: u64,
    ) {
        let (verdict, _) = self.judge(state, record, None);
        if self.settle(state, record, &verdict, How::Exited, processes) {
            return;
        }
        if matches!(verdict, Verdict::Signal { .. }) {
            note_seen(record, processes, self);
        }
        let mut stopping = Stopping::new(why, now, grace_ms);
        if !matches!(verdict, Verdict::Signal { .. }) {
            stopping.refused = Some(verdict.reason());
        }
        record.state = JobState::Stopping;
        record.stopping = Some(stopping);
        state.store.put(record);
        state.stopping = true;
        if matches!(verdict, Verdict::Signal { .. }) {
            self.send_term(state, record, processes, now);
        }
    }

    /// Send the TERM a stop has not yet sent, after a fresh look.
    fn send_term(
        &self,
        state: &mut State,
        record: &mut Record,
        processes: Option<&[Info]>,
        now: u64,
    ) {
        let Some(mut stopping) = record.stopping.clone() else {
            return;
        };
        match self.signal(state, record, None, Signal::SIGTERM) {
            Signalled::Sent => {
                stopping.term_sent = true;
                stopping.signal = Some("SIGTERM".into());
                stopping.since = now;
                stopping.refused = None;
            }
            Signalled::NotSent(verdict) => {
                if self.settle(state, record, &verdict, stopping.why, processes) {
                    return;
                }
                stopping.refused = Some(verdict.reason());
            }
        }
        record.stopping = Some(stopping);
        state.store.put(record);
    }

    /// Carry a stop on: send the TERM it could not send yet; once its grace
    /// has passed, KILL; once KILL has had its time, say how many are stuck
    /// and send it again now and then for a minute; once nothing of it is
    /// left, end it. Each signal is decided by a fresh look of its own.
    fn escalate(
        &self,
        state: &mut State,
        record: &mut Record,
        processes: Option<&[Info]>,
        now: u64,
    ) {
        let Some(mut stopping) = record.stopping.clone() else {
            return;
        };
        if !stopping.term_sent {
            return self.send_term(state, record, processes, now);
        }
        let (verdict, seen) = self.judge(state, record, None);
        if self.settle(state, record, &verdict, stopping.why, processes) {
            return;
        }
        if !matches!(verdict, Verdict::Signal { .. }) {
            let why = verdict.reason();
            if stopping.refused != Some(why) {
                stopping.refused = Some(why);
                record.stopping = Some(stopping);
                state.store.put(record);
            }
            return;
        }
        note_seen(record, processes, self);
        stopping.refused = None;

        let after = |since: u64, wait: Duration| now >= since.saturating_add(crate::time::ms(wait));
        let kill = match (stopping.killed, stopping.killed_last) {
            (None, _) => after(stopping.since, Duration::from_millis(stopping.grace_ms)),
            (Some(first), Some(last)) => {
                after(first, KILLED) && after(last, KILL_AGAIN) && !after(first, KILL_FOR)
            }
            (Some(_), None) => true,
        };
        if stopping.killed.is_some_and(|first| after(first, KILLED)) {
            stopping.stuck = seen
                .as_ref()
                .map(|seen| seen.members.iter().filter(|member| !member.zombie).count());
        }
        if kill {
            match self.signal(state, record, None, Signal::SIGKILL) {
                Signalled::Sent => {
                    stopping.killed.get_or_insert(now);
                    stopping.killed_last = Some(now);
                    stopping.signal = Some("SIGKILL".into());
                }
                Signalled::NotSent(verdict) => {
                    record.stopping = Some(stopping.clone());
                    if self.settle(state, record, &verdict, stopping.why, processes) {
                        return;
                    }
                    stopping.refused = Some(verdict.reason());
                }
            }
        }
        if record.stopping.as_ref() != Some(&stopping) {
            record.stopping = Some(stopping);
            state.store.put(record);
        }
    }

    /// End `record` when `verdict` says nothing of it can be signalled any
    /// more: ended (`how`), its leader gone (its remaining processes are
    /// listed, never signalled), its PID used again, or another boot.
    /// Whether it was a verdict that ends the job.
    pub(super) fn settle(
        &self,
        state: &mut State,
        record: &mut Record,
        verdict: &Verdict,
        how: How,
        processes: Option<&[Info]>,
    ) -> bool {
        match verdict {
            Verdict::Ended => self.finish(state, record, how, processes, Vec::new()),
            Verdict::LeaderGone { remaining } => {
                let remaining = remaining.iter().map(as_left).collect();
                self.finish(state, record, How::LeaderGone, processes, remaining)
            }
            Verdict::Reused => self.finish(state, record, How::Gone, processes, Vec::new()),
            Verdict::Restarted => self.finish(state, record, How::Restarted, processes, Vec::new()),
            Verdict::Signal { .. }
            | Verdict::Identity
            | Verdict::OtherAccount
            | Verdict::Unknown => return false,
        };
        true
    }

    /// End `record` `how`: its leader's exit collected when this daemon
    /// started it, and what left its group, with `remaining`, listed. A
    /// leader of this daemon's that cannot be collected yet is looked at
    /// again next time.
    fn finish(
        &self,
        state: &mut State,
        record: &mut Record,
        how: How,
        processes: Option<&[Info]>,
        remaining: Vec<Left>,
    ) {
        let mut exit = None;
        if let Some(child) = state.children.get_mut(&record.id) {
            match child.try_wait() {
                Ok(Some(status)) => exit = Some(status),
                Ok(None)
                    if matches!(
                        how,
                        How::Exited | How::Stopped | How::Timeout | How::Person | How::OnConsole
                    ) =>
                {
                    return;
                }
                _ => {}
            }
            state.children.remove(&record.id);
        }
        let mut left_over = remaining;
        if let (Some(processes), Some(identity)) = (processes, record.identity()) {
            for escaped in left(&record.seen, processes, identity.pgid(), self.uid) {
                if !left_over.iter().any(|known| known.pid == escaped.pid) {
                    left_over.push(escaped);
                }
            }
        }
        record.state = JobState::Ended;
        record.ended = Some(Ended {
            how,
            at: now_ms(),
            signal: record
                .stopping
                .as_ref()
                .and_then(|stopping| stopping.signal.clone()),
            exit_code: exit.and_then(|status| status.code()),
            exit_signal: exit
                .and_then(|status| status.signal())
                .map(run::signal_name),
            left: left_over,
        });
        state.store.put(record);
    }

    /// Find again, when the daemon starts, every job its records say may
    /// still run: one whose leader is still the job's is supervised again,
    /// one that ended or whose leader is gone is ended, and one whose
    /// record cannot be read, or whose processes do not match it or cannot
    /// be read, is shown as it is and never signalled.
    pub(super) fn adopt(&self) {
        let mut state = self.state();
        let processes = self.kernel.of_user(self.uid).ok();
        for read in state.store.records() {
            let Found::Record(mut record) = read else {
                continue;
            };
            if !record.live() {
                continue;
            }
            let (verdict, _) = self.judge(&state, &record, None);
            match verdict {
                Verdict::Signal { .. } => {
                    record.adopted = true;
                    if record.state == JobState::Launched {
                        record.state = JobState::Running;
                    }
                    state.store.put(&record);
                }
                Verdict::Ended => {
                    self.finish(
                        &mut state,
                        &mut record,
                        How::Gone,
                        processes.as_deref(),
                        Vec::new(),
                    );
                }
                verdict => {
                    self.settle(
                        &mut state,
                        &mut record,
                        &verdict,
                        How::Gone,
                        processes.as_deref(),
                    );
                }
            }
        }
        self.count(&mut state);
    }
}

/// Note `record`'s descendants among `processes`, to report later which
/// left its group.
fn note_seen(record: &mut Record, processes: Option<&[Info]>, jobs: &Jobs) {
    let (Some(processes), Some(identity)) = (processes, record.identity()) else {
        return;
    };
    let members: Vec<Info> = processes
        .iter()
        .filter(|process| process.pgid == identity.pgid())
        .cloned()
        .collect();
    observe(
        &mut record.seen,
        &identity.leader(),
        &members,
        processes,
        |pid| jobs.kernel.session(pid),
    );
}

/// Watch `jobs` until dropped: about once a second, five times a second
/// while one is stopping. A Stop is seen through the input service's
/// saved `stop_marker`; the helper on screen through `on_console`.
pub async fn supervise(jobs: Arc<Jobs>, stop_marker: PathBuf, on_console: fn() -> bool) {
    loop {
        tokio::time::sleep(jobs.interval()).await;
        let watched = jobs.clone();
        let marker = stop_marker.clone();
        let _ = tokio::task::spawn_blocking(move || {
            watched.watch_once(marker.exists(), on_console());
        })
        .await;
    }
}
