use super::*;
use crate::host_session::{Host, HostSession};

fn record() -> Record {
    Record::new(
        &HostSession::new(Host::Codex, "root")
            .unwrap()
            .run_by(Some(Instance::new(10, [1, 0]))),
        100,
        None,
    )
}

#[test]
fn one_reply_samples_each_instance_once() {
    let calls = std::cell::Cell::new(0);
    let instance = record().host_process.unwrap();
    let snapshot = Snapshot::sample([instance; 100], |_| {
        calls.set(calls.get() + 1);
        State::Running
    });
    for _ in 0..20 {
        assert!(snapshot.lifetime(&record(), 100, false).live);
    }
    assert_eq!(calls.get(), 1);
}

#[test]
fn a_liveness_batch_never_exceeds_its_process_budget() {
    let calls = std::cell::Cell::new(0);
    let snapshot = Snapshot::sample((1..1000).map(|pid| Instance::new(pid, [1, 0])), |_| {
        calls.set(calls.get() + 1);
        State::Running
    });
    assert_eq!(calls.get(), BUDGET);
    assert_eq!(snapshot.state(Instance::new(129, [1, 0])), State::Unknown);
}

#[test]
fn a_stopped_host_is_still_live() {
    let record = record();
    let snapshot = Snapshot::sample(record.host_process, |_| State::Stopped);
    assert_eq!(
        snapshot.lifetime(&record, u64::MAX, false),
        Lifetime {
            ended: None,
            live: true,
            active: false
        }
    );
}

#[test]
fn an_unknown_process_state_is_not_an_end() {
    let record = record();
    let snapshot = Snapshot::default();
    assert_eq!(
        snapshot.lifetime(&record, 100, false),
        Lifetime {
            ended: None,
            live: false,
            active: false
        }
    );
}

#[test]
fn a_gone_host_stays_live_while_its_verified_command_runs() {
    let record = record();
    let snapshot = Snapshot::sample(record.host_process, |_| State::Gone);
    assert_eq!(
        snapshot.lifetime(&record, 100, true),
        Lifetime {
            ended: None,
            live: true,
            active: true
        }
    );
    assert_eq!(
        snapshot.lifetime(&record, 100, false),
        Lifetime {
            ended: Some(100),
            live: false,
            active: false
        }
    );
}

#[test]
fn an_explicit_end_retains_surviving_work_without_reviving() {
    let mut record = record();
    record.ended = Some(200);
    let snapshot = Snapshot::sample(record.host_process, |_| State::Running);
    for work in [false, true] {
        assert_eq!(
            snapshot.lifetime(&record, 300, work),
            Lifetime {
                ended: Some(200),
                live: false,
                active: false
            }
        );
    }
}

#[test]
fn a_pid_reused_with_another_start_is_gone() {
    let current = Instance::current().unwrap();
    let reused = Instance::new(current.pid, [1, 0]);
    let snapshot = Snapshot::sample([reused], |instance| instance.state());
    assert_eq!(snapshot.state(reused), State::Gone);
}

struct HostChild(std::process::Child);

impl Drop for HostChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        self.0.wait().unwrap();
    }
}

#[test]
fn a_host_exit_is_seen_without_a_record_write() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    let mut child = HostChild(
        Command::new("/bin/sh")
            .args(["-c", "printf 'ready\\n'; read -r continuation"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let ready = std::thread::scope(|scope| {
        let (cancel, cancelled) = std::sync::mpsc::channel();
        let process = &mut child.0;
        let watchdog = scope.spawn(move || {
            if cancelled.recv_timeout(Duration::from_secs(5)).is_err() {
                let _ = process.kill();
                process.wait().unwrap();
            }
        });
        let mut ready = String::new();
        let result = std::io::BufReader::new(stdout).read_line(&mut ready);
        let _ = cancel.send(());
        watchdog.join().unwrap();
        result.unwrap();
        ready
    });
    assert_eq!(ready, "ready\n");
    let pid = i32::try_from(child.0.id()).unwrap();
    let instance = Instance::new(pid, crate::host::ancestry::process(pid).unwrap().start);
    let record = Record::new(
        &HostSession::new(Host::Codex, "root")
            .unwrap()
            .run_by(Some(instance)),
        100,
        None,
    );
    let at = std::time::Instant::now();
    let sampled = Snapshot::sample_at([instance], |instance| instance.state(), at);
    assert!(
        sampled
            .clone()
            .trusted_at(at)
            .lifetime(&record, 100, false)
            .live
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let expired = at + FRESH * 2;
    assert_eq!(sampled.trusted_at(expired).state(instance), State::Unknown);
    let gone = Snapshot::sample_at([instance], |instance| instance.state(), expired);
    assert_eq!(
        gone.trusted_at(expired).lifetime(&record, 100, false).ended,
        Some(100)
    );
    assert_eq!(record.ended, None, "no record rewrite was needed");
}

#[test]
fn a_blocked_liveness_read_starts_only_one_worker() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    let shared: &'static Shared<Snapshot, BatchKey> = Box::leak(Box::new(Shared::new(FRESH)));
    let (release, gate) = mpsc::channel::<()>();
    let (started, began) = mpsc::channel();
    let (completed, ended) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let key = BatchKey {
        uid: crate::test_support::UID,
        output: "/tmp/one".into(),
        instances: vec![Instance::new(10, [1, 0])],
    };
    let counting = calls.clone();
    assert!(
        shared
            .get_blocking(key.clone(), Duration::ZERO, move || {
                counting.fetch_add(1, Ordering::SeqCst);
                started.send(()).unwrap();
                let _ = gate.recv();
                completed.send(()).unwrap();
                Snapshot::default()
            })
            .is_none()
    );
    began.recv_timeout(Duration::from_secs(5)).unwrap();
    let readers: Vec<_> = (0..50)
        .map(|_| {
            let key = key.clone();
            std::thread::spawn(move || {
                shared.get_blocking(key, Duration::ZERO, || panic!("second liveness worker"))
            })
        })
        .collect();
    let refused: Vec<_> = readers
        .into_iter()
        .map(|reader| reader.join().unwrap())
        .collect();
    drop(release);
    ended.recv_timeout(Duration::from_secs(5)).unwrap();
    shared.join_workers();
    assert!(refused.iter().all(Option::is_none));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn liveness_cache_keys_do_not_cross_accounts_outputs_or_instances() {
    let shared: &'static Shared<usize, BatchKey> = Box::leak(Box::new(Shared::new(FRESH)));
    let base = BatchKey {
        uid: crate::test_support::UID,
        output: "/tmp/one".into(),
        instances: vec![Instance::new(10, [1, 0])],
    };
    let keys = [
        base.clone(),
        BatchKey {
            uid: base.uid + 1,
            ..base.clone()
        },
        BatchKey {
            output: "/tmp/two".into(),
            ..base.clone()
        },
        BatchKey {
            instances: vec![Instance::new(10, [2, 0])],
            ..base.clone()
        },
    ];
    for (index, key) in keys.into_iter().enumerate() {
        assert_eq!(shared.get_blocking(key, WAIT, move || index), Some(index));
        shared.join_workers();
    }
    assert_eq!(shared.get_blocking(base, WAIT, || 99), Some(99));
    shared.join_workers();
}

#[test]
fn a_liveness_sample_never_infers_death_after_expiry() {
    let record = record();
    let at = std::time::Instant::now();
    for state in [State::Running, State::Stopped, State::Gone, State::Unknown] {
        let sample = Snapshot::sample_at(record.host_process, |_| state, at);
        assert_eq!(
            sample
                .clone()
                .trusted_at(at + FRESH * 2 - Duration::from_nanos(1))
                .state(record.host_process.unwrap()),
            state
        );
        assert_eq!(
            sample
                .trusted_at(at + FRESH * 2)
                .state(record.host_process.unwrap()),
            State::Unknown
        );
    }
    let snapshot = Snapshot::sample_at(record.host_process, |_| State::Gone, at);
    let frozen = snapshot.trusted_at(at + FRESH * 2);
    assert_eq!(
        frozen.lifetime(&record, 100, false),
        Lifetime {
            ended: None,
            live: false,
            active: false
        }
    );
    let (shown, _) = crate::session_view::Scene {
        rows: &[],
        processes: &frozen,
        holder: None,
        waiting: &[],
        running: &[],
        causes: &[],
        pending: false,
        agent_holds: false,
        now: 100_000_000,
    }
    .project(&record);
    let history = crate::session_view::HistorySource {
        records: &[record],
        rows: &[],
        captures: &Default::default(),
        copies: &Default::default(),
        tasks: &[],
        now: 100,
    }
    .history_with(&frozen, |_| false);
    assert_eq!(
        (shown.state, history[0].state),
        (
            crate::session_view::SessionState::Idle,
            crate::session_view::SessionState::Idle
        )
    );
    assert!(!shown.live && !shown.relevant());
    assert_eq!(history[0].ended_at, None);
}
