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

#[test]
fn a_host_exit_is_seen_without_a_record_write() {
    let directory = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let instance = Instance::new(
        pid,
        crate::host_session::ancestry::process(pid).unwrap().start,
    );
    let record = Record::new(
        &HostSession::new(Host::Codex, "root")
            .unwrap()
            .run_by(Some(instance)),
        100,
        None,
    );
    let began = std::time::Instant::now();
    let mut live = Snapshot::polled(directory.path(), [instance]).lifetime(&record, 100, false);
    while !live.live && began.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
        live = Snapshot::polled(directory.path(), [instance]).lifetime(&record, 100, false);
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(live.live);
    let began = std::time::Instant::now();
    loop {
        let lifetime = Snapshot::polled(directory.path(), [instance]).lifetime(&record, 100, false);
        if lifetime.ended == Some(100) {
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    }
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
    let gate = Arc::new(std::sync::Mutex::new(gate));
    let calls = Arc::new(AtomicUsize::new(0));
    let key = BatchKey {
        uid: crate::test_support::UID,
        output: "/tmp/one".into(),
        instances: vec![Instance::new(10, [1, 0])],
    };
    let began = std::time::Instant::now();
    let readers: Vec<_> = (0..50)
        .map(|_| {
            let gate = Arc::clone(&gate);
            let calls = Arc::clone(&calls);
            let key = key.clone();
            std::thread::spawn(move || {
                shared.get_blocking(key, Duration::from_millis(20), move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let _ = gate.lock().unwrap().recv_timeout(Duration::from_secs(2));
                    Snapshot::default()
                })
            })
        })
        .collect();
    for reader in readers {
        assert!(reader.join().unwrap().is_none());
    }
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(release);
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
    }
    assert_eq!(shared.get_blocking(base, WAIT, || 99), Some(99));
}

#[test]
fn an_expired_batch_supplies_no_positive_liveness_or_inferred_end() {
    let record = record();
    let mut snapshot = Snapshot::sample(record.host_process, |_| State::Gone);
    snapshot.sampled_at = Some(std::time::Instant::now() - FRESH * 3);
    let frozen = snapshot.trusted();
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
