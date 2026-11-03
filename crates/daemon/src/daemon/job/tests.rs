use super::{
    observe::{left, observe},
    record::{Ended, Record, VERSION},
    reply::unconfirmed,
    verdict::{Observation, verdict},
    *,
};
use atlas_core::{
    host::Key,
    process::{Info, Instance, Start},
    protocol::Left,
};
use nix::errno::Errno;
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    task::Poll,
};

const UID: u32 = 502;
const BOOT: &str = "6A1E0000-0000-0000-0000-000000000000";
const START: Start = [1_761_650_000, 123_456];
const ID: &str = "7c2e91a04b3f";

fn record() -> Record {
    Record {
        version: VERSION,
        id: JobId::parse(ID).unwrap(),
        name: "nc".into(),
        task: None,
        session: None,
        uid: UID,
        boot: BOOT.into(),
        pid: Some(4312),
        start: Some(START),
        pgid: Some(4312),
        state: JobState::Running,
        started_at: 1,
        ends_at: u64::MAX,
        adopted: false,
        seen: Vec::new(),
        stopping: None,
        ended: None,
        log_resets: 0,
    }
}

fn process(pid: i32, start: Start, pgid: i32, ppid: i32) -> Info {
    Info {
        instance: Instance { pid, start },
        ppid,
        pgid,
        uid: UID,
        ruid: UID,
        svuid: UID,
        zombie: false,
        stopped: false,
        name: "sleep".into(),
    }
}

fn leader() -> Info {
    process(4312, START, 4312, 1)
}

fn later(by: u64) -> Start {
    [START[0] + by, START[1]]
}

fn seen(leader: Option<Info>, members: &[Info]) -> Observation {
    Observation {
        leader,
        members: members.to_vec(),
        own_child: false,
    }
}

fn judged(expect: Option<&Expected>, leader: Option<Info>, members: &[Info]) -> Verdict {
    verdict(&record(), expect, BOOT, UID, Some(&seen(leader, members)))
}

#[test]
fn a_matching_job_is_signalled_only_through_its_own_group() {
    let members = [leader(), process(4313, later(1), 4312, 4312)];
    let expected = record().expected().unwrap();
    assert_eq!(
        judged(Some(&expected), Some(leader()), &members),
        Verdict::Signal { pgid: 4312 }
    );
    assert_eq!(
        judged(None, Some(leader()), &members),
        Verdict::Signal { pgid: 4312 }
    );
}

#[test]
fn a_leader_this_daemon_has_not_reaped_counts_as_live_and_any_other_zombie_leader_counts_as_gone() {
    let member = process(4313, later(1), 4312, 4312);
    assert_eq!(
        judged(None, None, std::slice::from_ref(&member)),
        Verdict::LeaderGone {
            remaining: vec![member.clone()]
        }
    );
    // A leader that exited and is not this daemon's to collect is gone too.
    let mut zombie = leader();
    zombie.zombie = true;
    assert_eq!(
        judged(
            None,
            Some(zombie.clone()),
            &[zombie.clone(), member.clone()]
        ),
        Verdict::LeaderGone {
            remaining: vec![member.clone()]
        }
    );
    // This daemon's own unreaped leader still holds the group's ID.
    let own = Observation {
        own_child: true,
        ..seen(Some(zombie.clone()), &[zombie, member])
    };
    assert_eq!(
        verdict(&record(), None, BOOT, UID, Some(&own)),
        Verdict::Signal { pgid: 4312 }
    );
}

#[test]
fn a_reused_pid_in_a_leaderless_group_is_never_signalled() {
    let stranger = process(4312, later(60), 4312, 1);
    let his = process(4400, later(61), 4312, 4312);
    assert_eq!(
        judged(None, Some(stranger.clone()), &[stranger, his.clone()]),
        Verdict::Reused
    );
    // Even a stranger's group that outlived its own leader is not the job's.
    assert!(matches!(
        judged(None, None, &[his]),
        Verdict::LeaderGone { .. }
    ));
}

#[test]
fn a_record_that_disagrees_with_the_person_is_never_signalled() {
    let known = record().expected().unwrap();
    let changes: [fn(&mut Expected); 4] = [
        |expect| expect.boot = "another".into(),
        |expect| expect.pid += 1,
        |expect| expect.start[1] += 1,
        |expect| expect.pgid += 1,
    ];
    for change in changes {
        let mut expect = known.clone();
        change(&mut expect);
        assert_eq!(
            judged(Some(&expect), Some(leader()), &[leader()]),
            Verdict::Identity
        );
    }
}

#[test]
fn ownership_follows_the_kernels_own_rule() {
    // Another account's process: neither its real nor its saved ID is ours.
    let mut theirs = process(4313, later(1), 4312, 4312);
    (theirs.uid, theirs.ruid, theirs.svuid) = (501, 501, 501);
    assert_eq!(
        judged(None, Some(leader()), &[leader(), theirs]),
        Verdict::OtherAccount
    );

    // A set-user-ID-root program the helper started is still the helper's.
    let mut setuid = process(4313, later(1), 4312, 4312);
    (setuid.uid, setuid.svuid) = (0, 0);
    assert_eq!(
        judged(None, Some(leader()), &[leader(), setuid]),
        Verdict::Signal { pgid: 4312 }
    );

    // A process that exited belongs to no account.
    let mut gone = process(4314, later(1), 4312, 4312);
    (gone.uid, gone.ruid, gone.svuid, gone.zombie) = (0, 0, 0, true);
    assert_eq!(
        judged(None, Some(leader()), &[leader(), gone]),
        Verdict::Signal { pgid: 4312 }
    );

    let mut record = record();
    record.uid = 501;
    assert_eq!(
        verdict(&record, None, BOOT, UID, Some(&seen(Some(leader()), &[]))),
        Verdict::OtherAccount
    );
}

#[test]
fn a_leader_in_another_group_is_refused() {
    let moved = process(4312, START, 9999, 1);
    assert_eq!(judged(None, Some(moved), &[]), Verdict::Identity);
}

#[test]
fn a_job_from_another_boot_has_ended() {
    let mut record = record();
    record.boot = "an earlier boot".into();
    assert_eq!(
        verdict(
            &record,
            None,
            BOOT,
            UID,
            Some(&seen(Some(leader()), &[leader()]))
        ),
        Verdict::Restarted
    );
}

#[test]
fn a_group_with_no_live_member_has_ended() {
    assert_eq!(judged(None, None, &[]), Verdict::Ended);
    let mut zombie = leader();
    zombie.zombie = true;
    let own = Observation {
        own_child: true,
        ..seen(Some(zombie.clone()), &[zombie])
    };
    assert_eq!(
        verdict(&record(), None, BOOT, UID, Some(&own)),
        Verdict::Ended
    );
}

#[test]
fn processes_that_could_not_be_read_end_nothing() {
    assert_eq!(verdict(&record(), None, BOOT, UID, None), Verdict::Unknown);
}

#[test]
fn a_job_identity_is_bounded_and_led_by_its_own_group() {
    assert_eq!(record().identity(), Some(Instance::new(4312, START)));
    for (pid, start, pgid) in [
        (4312, START, 4313),
        (0, START, 0),
        (1, START, 1),
        (100_000, START, 100_000),
        (-5, START, -5),
        (4312, [0, 1], 4312),
        (4312, [1, 1_000_000], 4312),
    ] {
        let mut unsound = record();
        (unsound.pid, unsound.start, unsound.pgid) = (Some(pid), Some(start), Some(pgid));
        assert!(unsound.identity().is_none(), "{pid} {pgid}");
    }
    let mut unnamed = record();
    unnamed.boot = String::new();
    assert!(unnamed.identity().is_none());

    let mut corrupt = record();
    corrupt.pgid = Some(4400);
    assert!(!corrupt.sound(), "a record whose group is not its leader's");
    assert_eq!(
        verdict(
            &corrupt,
            None,
            BOOT,
            UID,
            Some(&seen(Some(leader()), &[leader()]))
        ),
        Verdict::Identity
    );
}

#[test]
fn descendants_that_left_the_group_are_reported_not_signalled() {
    let job = Instance {
        pid: 4312,
        start: START,
    };
    let child = process(4313, later(1), 4312, 4312);
    let escaped = process(4400, later(2), 4400, 4313);
    let grandchild = process(4401, later(3), 4400, 4400);
    let unrelated = process(5000, later(1), 5000, 1);
    let processes = [
        leader(),
        child.clone(),
        escaped.clone(),
        grandchild,
        unrelated,
    ];
    let mut noted = Vec::new();
    observe(&mut noted, &job, &processes, |_| None);
    assert!(noted.contains(&escaped.instance));
    assert!(noted.iter().any(|instance| instance.pid == 4401));
    assert!(!noted.iter().any(|instance| instance.pid == 5000));

    let reused = process(4401, later(30), 4401, 1);
    let now = [leader(), escaped, reused];
    assert_eq!(
        left(&noted, &now, 4312, UID),
        vec![Left {
            pid: 4400,
            name: "sleep".into()
        }]
    );
}

#[test]
fn what_was_seen_stays_within_its_bound() {
    let job = Instance {
        pid: 4312,
        start: START,
    };
    let mut processes = vec![leader()];
    processes.extend((0..300).map(|n| process(5000 + n, later(1), 4312, 4312)));
    let mut noted = Vec::new();
    observe(&mut noted, &job, &processes, |_| None);
    assert_eq!(noted.len(), observe::SEEN_MAX);
}

#[test]
fn a_job_record_reads_back_as_it_was_written() {
    let mut record = record();
    record.state = JobState::Ended;
    record.ended = Some(Ended {
        how: How::LeaderGone,
        at: 5,
        signal: Some("SIGTERM".into()),
        exit_code: None,
        exit_signal: None,
        left: vec![Left {
            pid: 9,
            name: "sleep".into(),
        }],
    });
    let text = serde_json::to_string(&record).unwrap();
    assert!(text.contains("\"how\":\"leader-gone\""), "{text}");
    assert_eq!(serde_json::from_str::<Record>(&text).unwrap(), record);
    let view = json!(record.view());
    assert_eq!(view["how"], "leader-gone");
    assert_eq!(view["remaining"], 1);
    assert!(view.get("argv").is_none(), "arguments are never kept");
}

#[test]
fn age_retention_keeps_a_job_completion_until_its_write_is_acknowledged() {
    assert_unwritten_job_survives_retention(u64::MAX, 0);
}

#[test]
fn record_pressure_keeps_a_job_completion_until_its_write_is_acknowledged() {
    assert_unwritten_job_survives_retention(5, 65);
}

fn assert_unwritten_job_survives_retention(now: u64, pressure: usize) {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let mut job = record();
    store.write(&job).unwrap();
    let path = store.directory().join(format!("{}.json", job.id));
    let saved = path.with_extension("saved");
    let previous = std::fs::read(&path).unwrap();
    let log = store.log_path(&job.id);
    std::fs::write(&log, b"requested command output\n").unwrap();

    // Make the replacement fail independently of process privilege or umask.
    // Restore the durable running record before testing retention decisions.
    std::fs::rename(&path, &saved).unwrap();
    std::fs::create_dir(&path).unwrap();
    job.state = JobState::Ended;
    job.ended = Some(Ended {
        how: How::LeaderGone,
        at: 5,
        signal: None,
        exit_code: None,
        exit_signal: None,
        left: Vec::new(),
    });
    assert!(!store.put(&job));
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&saved, &path).unwrap();

    for number in 0..pressure {
        let mut newer = job.clone();
        newer.id = JobId::parse(&format!("{number:012x}")).unwrap();
        newer.ended.as_mut().unwrap().at = 6;
        store.write(&newer).unwrap();
    }
    store.prune(now);
    if pressure > 0 {
        assert_eq!(store.records().len(), 64);
    }
    assert!(matches!(store.read(&job.id), Some(Found::Record(found)) if *found == job));
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    assert_eq!(std::fs::read(&log).unwrap(), b"requested command output\n");

    assert!(store.put(&job));
    store.prune(u64::MAX);
    assert!(!store.holds(&job.id));
    assert!(!path.exists());
    assert!(!log.exists());
}

#[test]
fn job_log_pruning_keeps_unreadable_and_possibly_live_records() {
    for now in [0, u64::MAX] {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path()).unwrap();
        let running = record();
        store.write(&running).unwrap();
        let mut future = running.clone();
        future.id = JobId::parse("abcdef123456").unwrap();
        future.version += 1;
        store.write(&future).unwrap();
        let unreadable = JobId::parse("fedcba654321").unwrap();
        std::fs::write(
            store.directory().join(format!("{unreadable}.json")),
            b"{unreadable-recovery-record",
        )
        .unwrap();
        let unrelated = store.directory().join("future-version.evidence");
        std::fs::write(&unrelated, b"unknown recovery evidence").unwrap();
        let mut evidence = Vec::new();
        for id in [&running.id, &future.id, &unreadable] {
            let path = store.directory().join(format!("{id}.json"));
            evidence.push((path.clone(), std::fs::read(path).unwrap()));
            let log = store.log_path(id);
            std::fs::write(&log, b"explicitly requested output\n").unwrap();
            evidence.push((log, b"explicitly requested output\n".to_vec()));
        }
        for number in 0..65 {
            let mut ended = running.clone();
            ended.id = JobId::parse(&format!("{number:012x}")).unwrap();
            ended.state = JobState::Ended;
            ended.ended = Some(Ended {
                how: How::LeaderGone,
                at: 5,
                signal: None,
                exit_code: None,
                exit_signal: None,
                left: Vec::new(),
            });
            store.write(&ended).unwrap();
        }

        store.prune(now);
        assert_eq!(store.records().len(), if now == 0 { 64 } else { 3 });
        assert!(matches!(store.read(&running.id), Some(Found::Record(found)) if *found == running));
        assert!(matches!(store.read(&future.id), Some(Found::Unreadable(id)) if id == future.id));
        assert!(matches!(store.read(&unreadable), Some(Found::Unreadable(id)) if id == unreadable));
        for (path, bytes) in evidence {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        assert_eq!(
            std::fs::read(unrelated).unwrap(),
            b"unknown recovery evidence"
        );
    }
}

#[test]
fn a_settled_job_is_retained_at_the_seven_day_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let mut ended = record();
    ended.state = JobState::Ended;
    ended.ended = Some(Ended {
        how: How::LeaderGone,
        at: 5,
        signal: None,
        exit_code: None,
        exit_signal: None,
        left: Vec::new(),
    });
    store.write(&ended).unwrap();
    let week = atlas_core::time::ms(Duration::from_secs(7 * 24 * 60 * 60));
    store.prune(5 + week);
    assert!(matches!(store.read(&ended.id), Some(Found::Record(found)) if *found == ended));
    store.prune(6 + week);
    assert!(!store.holds(&ended.id));
}

#[test]
fn log_reset_preserves_the_writers_append_handle_and_only_resets_above_the_threshold() {
    use std::io::Write;

    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let id = record().id;
    let path = store.log_path(&id);
    std::fs::write(&path, b"12345").unwrap();
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    assert!(!store.bound_log(&id, 5));
    assert_eq!(std::fs::read(&path).unwrap(), b"12345");
    writer.write_all(b"6").unwrap();
    assert!(store.bound_log(&id, 5));
    assert!(std::fs::read(&path).unwrap().is_empty());
    writer.write_all(b"next").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"next");
    drop(writer);

    std::fs::remove_file(&path).unwrap();
    let elsewhere = directory.path().join("unrelated-output");
    std::fs::write(&elsewhere, b"must survive").unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert!(!store.bound_log(&id, 5));
    assert_eq!(std::fs::read(elsewhere).unwrap(), b"must survive");
}

/// A stand-in kernel: a process table the test changes, every signal kept.
#[derive(Default)]
struct Table {
    processes: Mutex<Vec<Info>>,
    /// Whether reads of the table fail.
    failing: AtomicBool,
    /// After this many reads of one process, its PID is another process's.
    reused_after: AtomicUsize,
    /// Whether the table's leader is replaced, with a group of its own,
    /// while a group is being listed.
    replace_during_group: AtomicBool,
    reads: AtomicUsize,
    panic_on_read: AtomicBool,
    watching: Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
    signals: Mutex<Vec<(i32, Signal)>>,
}

impl Table {
    fn with(processes: Vec<Info>) -> Arc<Self> {
        Arc::new(Self {
            processes: Mutex::new(processes),
            reused_after: AtomicUsize::new(usize::MAX),
            ..Self::default()
        })
    }

    fn signals(&self) -> Vec<(i32, Signal)> {
        self.signals.lock().unwrap().clone()
    }

    fn read(&self) -> Result<Vec<Info>> {
        let watching = self.watching.lock().unwrap().take();
        if let Some((entered, release)) = watching {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        assert!(
            !self.panic_on_read.load(Ordering::SeqCst),
            "private worker cause"
        );
        if self.failing.load(Ordering::SeqCst) {
            return Err(Error::new(Code::State, "The process list is unavailable"));
        }
        Ok(self.processes.lock().unwrap().clone())
    }
}

/// The table as a kernel, shared with the test.
struct Shared(Arc<Table>);

impl Kernel for Shared {
    fn process(&self, pid: i32) -> Result<Option<Info>> {
        let reads = self.0.reads.fetch_add(1, Ordering::SeqCst) + 1;
        let mut found = self
            .0
            .read()?
            .into_iter()
            .find(|info| info.instance.pid == pid);
        if reads >= self.0.reused_after.load(Ordering::SeqCst)
            && let Some(found) = found.as_mut()
        {
            found.instance.start = later(600);
        }
        Ok(found)
    }

    fn group(&self, pgid: i32) -> Result<Vec<Info>> {
        let listed = self
            .0
            .read()?
            .into_iter()
            .filter(|info| info.pgid == pgid)
            .collect();
        if self.0.replace_during_group.swap(false, Ordering::SeqCst) {
            *self.0.processes.lock().unwrap() = vec![
                process(4312, later(90), 4312, 1),
                process(4500, later(91), 4312, 4312),
            ];
        }
        Ok(listed)
    }

    fn of_user(&self, _: u32) -> Result<Vec<Info>> {
        self.0.read()
    }

    fn session(&self, _: i32) -> Option<i32> {
        None
    }

    fn killpg(&self, pgid: i32, signal: Signal) -> std::result::Result<(), Errno> {
        self.0.signals.lock().unwrap().push((pgid, signal));
        let live = self
            .0
            .processes
            .lock()
            .unwrap()
            .iter()
            .any(|info| info.pgid == pgid && !info.zombie);
        if live { Ok(()) } else { Err(Errno::ESRCH) }
    }
}

/// Jobs in a folder of their own, with `record` already kept there, over
/// the stand-in `table`.
fn jobs_with(record: &Record, table: &Arc<Table>) -> (tempfile::TempDir, Jobs) {
    let folder = tempfile::tempdir().unwrap();
    let jobs = folder.path().join("jobs");
    std::fs::create_dir(&jobs).unwrap();
    std::fs::write(
        jobs.join(format!("{}.json", record.id)),
        serde_json::to_vec(record).unwrap(),
    )
    .unwrap();
    let opened = Jobs::open_in(
        folder.path(),
        PathBuf::from("/nonexistent"),
        UID,
        1 << 20,
        Box::new(Shared(table.clone())),
        BOOT.into(),
    )
    .unwrap();
    (folder, opened)
}

fn shown(jobs: &Jobs) -> Value {
    jobs.show(Some(&JobId::parse(ID).unwrap()))["job"].clone()
}

#[test]
fn listing_jobs_refuses_when_the_bounded_scan_cannot_account_for_every_record() {
    let table = Table::with(vec![leader()]);
    let (directory, jobs) = jobs_with(&record(), &table);
    let folder = directory.path().join("jobs");
    for index in 0..atlas_core::record_folder::LISTED {
        std::fs::write(folder.join(format!("unrelated-{index}")), "").unwrap();
    }
    let reply = jobs.show(None);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["reason"], "unobserved");
    assert!(
        reply["error"]
            .as_str()
            .unwrap()
            .contains("may still be running")
    );
    assert_eq!(jobs.show(Some(&record().id))["job"]["state"], "running");
}

#[test]
fn listing_jobs_refuses_when_the_records_folder_cannot_be_scanned() {
    let table = Table::with(vec![leader()]);
    let (directory, jobs) = jobs_with(&record(), &table);
    let folder = directory.path().join("jobs");
    std::fs::rename(&folder, directory.path().join("kept-jobs")).unwrap();
    std::fs::write(folder, "not a folder").unwrap();
    let reply = jobs.show(None);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["reason"], "unobserved");
}

#[test]
fn an_identity_refusal_followed_by_a_stop_never_signals() {
    // The PID's group is another's now: its leader moved to a group whose
    // ID is not its own PID.
    let moved = process(4312, START, 9999, 1);
    let table = Table::with(vec![moved, process(4313, later(1), 9999, 4312)]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    for _ in 0..3 {
        jobs.watch_once(Some(How::Person)).unwrap();
    }
    assert_eq!(shown(&jobs)["refused"], "identity");
    let refused = jobs.stop(
        &JobId::parse(ID).unwrap(),
        &record().expected().unwrap(),
        Duration::from_secs(1),
    );
    assert!(refused.get("signalled").is_none(), "{refused}");
    for _ in 0..3 {
        jobs.watch_once(Some(How::Person)).unwrap();
    }
    assert!(table.signals().is_empty(), "{:?}", table.signals());
}

#[test]
fn an_adopted_group_without_its_leader_is_reported_never_signalled() {
    let member = process(4313, later(1), 4312, 1);
    let table = Table::with(vec![member]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    let job = shown(&jobs);
    assert_eq!(job["state"], "ended", "{job}");
    assert_eq!(job["how"], "leader-gone");
    assert_eq!(job["left"][0]["pid"], 4313);
    jobs.stop_all(StopReason::Person);
    jobs.watch_once(Some(How::OnConsole)).unwrap();
    assert!(table.signals().is_empty());
}

#[test]
fn a_record_whose_group_is_not_its_leaders_is_refused_on_load() {
    let mut corrupt = record();
    corrupt.pgid = Some(4313);
    let table = Table::with(vec![leader(), process(4313, later(1), 4313, 4312)]);
    let (_folder, jobs) = jobs_with(&corrupt, &table);
    assert_eq!(shown(&jobs)["state"], "unknown");
    jobs.stop_all(StopReason::Person);
    jobs.watch_once(Some(How::Person)).unwrap();
    assert!(table.signals().is_empty());
}

#[test]
fn a_failed_process_query_ends_nothing_and_is_tried_again() {
    let table = Table::with(vec![leader()]);
    let mut due = record();
    due.ends_at = 0;
    let (_folder, jobs) = jobs_with(&due, &table);
    table.failing.store(true, Ordering::SeqCst);
    jobs.watch_once(None).unwrap();
    jobs.watch_once(None).unwrap();
    let job = shown(&jobs);
    assert_eq!(job["state"], "stopping", "{job}");
    assert_eq!(job["refused"], "unobserved");
    assert!(table.signals().is_empty());

    table.failing.store(false, Ordering::SeqCst);
    jobs.watch_once(None).unwrap();
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
}

#[test]
fn every_signal_is_decided_by_a_look_taken_right_before_it() {
    // The stop's own look finds the job; by the look right before TERM its
    // PID is another process's.
    let table = Table::with(vec![leader()]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    table.reads.store(0, Ordering::SeqCst);
    table.reused_after.store(2, Ordering::SeqCst);
    let reply = jobs.stop(
        &JobId::parse(ID).unwrap(),
        &record().expected().unwrap(),
        Duration::from_secs(1),
    );
    assert!(reply.get("signalled").is_none(), "{reply}");
    assert!(table.signals().is_empty());
}

#[test]
fn a_stop_is_sent_even_when_its_record_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let records = folder.path().join("jobs");
    std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o500)).unwrap();
    jobs.watch_once(Some(How::Person)).unwrap();
    let job = shown(&jobs);
    std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
    assert_eq!(job["state"], "stopping", "kept in memory: {job}");
    assert_eq!(jobs.running().unwrap(), 1);
}

#[test]
fn the_log_of_an_ended_job_whose_processes_still_write_is_kept_in_bounds() {
    let table = Table::with(Vec::new());
    let mut ended = record();
    ended.state = JobState::Ended;
    ended.ended = Some(Ended {
        how: How::LeaderGone,
        at: now_ms(),
        signal: None,
        exit_code: None,
        exit_signal: None,
        left: vec![Left {
            pid: 4313,
            name: "sleep".into(),
        }],
    });
    let (folder, jobs) = jobs_with(&ended, &table);
    let log = folder.path().join(format!("jobs/{ID}.log"));
    std::fs::write(&log, vec![b'x'; 2 << 20]).unwrap();
    jobs.watch_once(None).unwrap();
    assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
    assert_eq!(shown(&jobs)["logResets"], 1);
}

#[test]
fn every_job_is_counted_from_the_records() {
    let table = Table::with(vec![leader()]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    assert_eq!(jobs.running().unwrap(), 1);
    table.processes.lock().unwrap().clear();
    jobs.watch_once(None).unwrap();
    assert_eq!(jobs.running().unwrap(), 0);
    assert_eq!(shown(&jobs)["how"], "exited");
}

#[test]
fn two_reads_that_disagree_on_the_leader_say_nothing_about_the_group() {
    let listed = leader();
    let mut read_after = leader();
    read_after.instance.start = later(90);
    let seen = Observation {
        leader: Some(read_after),
        members: vec![listed],
        own_child: false,
    };
    assert_eq!(
        verdict(&record(), None, BOOT, UID, Some(&seen)),
        Verdict::Unknown
    );

    let mut owner = leader();
    owner.ruid = 501;
    owner.svuid = 501;
    let seen = Observation {
        leader: Some(owner),
        members: vec![leader()],
        own_child: false,
    };
    assert_eq!(
        verdict(&record(), None, BOOT, UID, Some(&seen)),
        Verdict::Unknown
    );

    // Read alive after a listing of its group that lacked it.
    let seen = Observation {
        leader: Some(leader()),
        members: vec![process(4313, later(1), 4312, 4312)],
        own_child: false,
    };
    assert_eq!(
        verdict(&record(), None, BOOT, UID, Some(&seen)),
        Verdict::Unknown
    );
}

#[test]
fn a_leader_replaced_while_its_group_is_listed_is_never_signalled() {
    let table = Table::with(vec![leader(), process(4313, later(1), 4312, 4312)]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    table.replace_during_group.store(true, Ordering::SeqCst);
    let reply = jobs.stop(
        &JobId::parse(ID).unwrap(),
        &record().expected().unwrap(),
        Duration::from_secs(1),
    );
    assert!(reply.get("signalled").is_none(), "{reply}");
    for _ in 0..3 {
        jobs.watch_once(Some(How::Person)).unwrap();
    }
    assert!(table.signals().is_empty(), "{:?}", table.signals());
}

#[test]
fn a_record_that_cannot_be_read_is_counted_unresolved_and_named_by_a_stop_of_every_job() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let other = JobId::parse("0123456789ab").unwrap();
    std::fs::write(folder.path().join(format!("jobs/{other}.json")), b"{").unwrap();
    jobs.watch_once(None).unwrap();
    assert_eq!(
        json!(jobs.census().unwrap()),
        json!({"running": 1, "launching": 0, "unresolved": 1})
    );
    let reply = jobs.stop_all(StopReason::Person);
    assert_eq!(reply["unresolved"], json!([{"id": other}]), "{reply}");
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
}

/// Every reply as it goes on the wire: Atlas.app and older builds read
/// these keys and values.
#[test]
fn the_replies_keep_their_wire_shape() {
    let id = JobId::parse(ID).unwrap();
    let expect = record().expected().unwrap();
    let mut out = Vec::new();
    let table = Table::with(vec![leader()]);
    let (_f, jobs) = jobs_with(&record(), &table);
    out.push(("show-one", jobs.show(Some(&id))));
    out.push(("show-all", jobs.show(None)));
    out.push(("census", json!(jobs.census().unwrap())));
    out.push(("log-missing", jobs.log(&id, 10)));
    out.push((
        "show-unknown",
        jobs.show(Some(&JobId::parse("0123456789ab").unwrap())),
    ));
    out.push((
        "stop-unknown",
        jobs.stop(
            &JobId::parse("0123456789ab").unwrap(),
            &expect,
            Duration::from_secs(1),
        ),
    ));
    let mut wrong = expect.clone();
    wrong.pid += 1;
    out.push((
        "stop-identity",
        jobs.stop(&id, &wrong, Duration::from_secs(1)),
    ));
    out.push((
        "stop-signalled",
        jobs.stop(&id, &expect, Duration::from_secs(1)),
    ));
    out.push(("show-stopping", jobs.show(Some(&id))));
    out.push((
        "stop-already-stopping",
        jobs.stop(&id, &expect, Duration::from_secs(1)),
    ));
    out.push(("stop-all", jobs.stop_all(StopReason::Person)));
    let mut ended = record();
    ended.state = JobState::Ended;
    ended.adopted = true;
    ended.task = Key::parse("0123456789ab");
    ended.ended = Some(Ended {
        how: How::LeaderGone,
        at: 7,
        signal: Some("SIGTERM".into()),
        exit_code: Some(3),
        exit_signal: Some("SIGKILL".into()),
        left: vec![Left {
            pid: 4313,
            name: "sleep".into(),
        }],
    });
    let table = Table::with(vec![]);
    let (f, jobs) = jobs_with(&ended, &table);
    std::fs::write(f.path().join(format!("jobs/{ID}.log")), b"hello").unwrap();
    out.push(("show-ended", jobs.show(Some(&id))));
    out.push((
        "stop-ended",
        jobs.stop(&id, &expect, Duration::from_secs(1)),
    ));
    out.push(("log", jobs.log(&id, 3)));
    let table = Table::with(vec![leader()]);
    let mut due = record();
    due.ends_at = 0;
    let (_f, jobs) = jobs_with(&due, &table);
    table.failing.store(true, Ordering::SeqCst);
    jobs.watch_once(None).unwrap();
    jobs.watch_once(None).unwrap();
    out.push(("show-refused", jobs.show(Some(&id))));
    out.push(("stop-all-unobserved", jobs.stop_all(StopReason::OnConsole)));
    let t2 = Table::with(vec![leader()]);
    let (_g, j2) = jobs_with(&record(), &t2);
    t2.failing.store(true, Ordering::SeqCst);
    out.push((
        "stop-unobserved",
        j2.stop(&id, &expect, Duration::from_secs(1)),
    ));
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let other = JobId::parse("0123456789ab").unwrap();
    std::fs::write(folder.path().join(format!("jobs/{other}.json")), b"{").unwrap();
    out.push(("show-all-unreadable", jobs.show(None)));
    out.push((
        "stop-unreadable",
        jobs.stop(&other, &expect, Duration::from_secs(1)),
    ));
    out.push(("stop-all-unreadable", jobs.stop_all(StopReason::Stopped)));
    let mut held = record();
    held.state = JobState::Stopping;
    held.stopping = Some(Stopping {
        refused: Some(JobRefusal::OtherAccount),
        stuck: Some(2),
        ..Stopping::new(How::Timeout, 1, 5)
    });
    let mut stranger = leader();
    stranger.uid = 501;
    stranger.ruid = 501;
    stranger.svuid = 501;
    let table = Table::with(vec![stranger]);
    let (_h, jobs) = jobs_with(&held, &table);
    out.push(("show-held", jobs.show(Some(&id))));
    out.push(("stop-all-held", jobs.stop_all(StopReason::Person)));
    out.push(("unconfirmed", unconfirmed(&record(), "said")));
    out.push((
        "limit",
        JobFailure::new(JobRefusal::Limit, "msg").into_value(),
    ));
    let expected = [
        (
            "show-one",
            r#"{"action":"show","job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"start":[1761650000,123456],"startedAt":1,"state":"running","task":null,"uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "show-all",
            r#"{"action":"show","jobs":[{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"start":[1761650000,123456],"startedAt":1,"state":"running","task":null,"uid":502}],"ok":true,"op":"job"}"#,
        ),
        ("census", r#"{"launching":0,"running":1,"unresolved":0}"#),
        (
            "log-missing",
            r#"{"action":"log","data":"","id":"7c2e91a04b3f","logResets":0,"ok":true,"op":"job","outputTruncated":false,"size":0,"task":null}"#,
        ),
        (
            "show-unknown",
            r#"{"code":"job","error":"No background job with that ID is known in the helper","id":"0123456789ab","ok":false,"reason":"unknown","uncertain":false}"#,
        ),
        (
            "stop-unknown",
            r#"{"code":"job","error":"No background job with that ID is known in the helper","id":"0123456789ab","ok":false,"reason":"unknown","uncertain":false}"#,
        ),
        (
            "stop-identity",
            r#"{"code":"job","error":"The job is no longer the process Atlas started, so nothing was signalled","id":"7c2e91a04b3f","ok":false,"reason":"identity","uncertain":false}"#,
        ),
        (
            "stop-signalled",
            r#"{"action":"stop","job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"signal":"SIGTERM","start":[1761650000,123456],"startedAt":1,"state":"stopping","task":null,"uid":502},"ok":true,"op":"job","signalled":"SIGTERM"}"#,
        ),
        (
            "show-stopping",
            r#"{"action":"show","job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"signal":"SIGTERM","start":[1761650000,123456],"startedAt":1,"state":"stopping","task":null,"uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "stop-already-stopping",
            r#"{"action":"stop","alreadyStopping":true,"job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"signal":"SIGTERM","start":[1761650000,123456],"startedAt":1,"state":"stopping","task":null,"uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "stop-all",
            r#"{"action":"stop-all","ended":0,"ok":true,"op":"job","refused":[],"stopping":1,"unknown":[],"unresolved":[]}"#,
        ),
        (
            "show-ended",
            r#"{"action":"show","job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endedAt":7,"endsAt":18446744073709551615,"exitCode":3,"exitSignal":"SIGKILL","how":"leader-gone","id":"7c2e91a04b3f","left":[{"name":"sleep","pid":4313}],"logResets":0,"name":"nc","pgid":4312,"pid":4312,"remaining":1,"session":null,"signal":"SIGTERM","start":[1761650000,123456],"startedAt":1,"state":"ended","task":"0123456789ab","uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "stop-ended",
            r#"{"action":"stop","alreadyEnded":true,"job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endedAt":7,"endsAt":18446744073709551615,"exitCode":3,"exitSignal":"SIGKILL","how":"leader-gone","id":"7c2e91a04b3f","left":[{"name":"sleep","pid":4313}],"logResets":0,"name":"nc","pgid":4312,"pid":4312,"remaining":1,"session":null,"signal":"SIGTERM","start":[1761650000,123456],"startedAt":1,"state":"ended","task":"0123456789ab","uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "log",
            r#"{"action":"log","data":"bGxv","id":"7c2e91a04b3f","logResets":0,"ok":true,"op":"job","outputTruncated":true,"size":5,"task":"0123456789ab"}"#,
        ),
        (
            "show-refused",
            r#"{"action":"show","job":{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":0,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"refused":"unobserved","session":null,"signal":null,"start":[1761650000,123456],"startedAt":1,"state":"stopping","task":null,"uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "stop-all-unobserved",
            r#"{"action":"stop-all","ended":0,"ok":true,"op":"job","refused":[],"stopping":0,"unknown":[{"id":"7c2e91a04b3f"}],"unresolved":[]}"#,
        ),
        (
            "stop-unobserved",
            r#"{"code":"job","error":"Atlas could not read the job's processes, so nothing was signalled; try again","id":"7c2e91a04b3f","ok":false,"reason":"unobserved","uncertain":false}"#,
        ),
        (
            "show-all-unreadable",
            r#"{"action":"show","jobs":[{"error":"Atlas cannot tell whether this job's process is running","id":"0123456789ab","state":"unknown"},{"adopted":true,"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"start":[1761650000,123456],"startedAt":1,"state":"running","task":null,"uid":502}],"ok":true,"op":"job"}"#,
        ),
        (
            "stop-unreadable",
            r#"{"code":"job","error":"Atlas cannot tell whether this job's process is running","id":"0123456789ab","ok":false,"reason":"unknown","uncertain":false}"#,
        ),
        (
            "stop-all-unreadable",
            r#"{"action":"stop-all","ended":0,"ok":true,"op":"job","refused":[],"stopping":1,"unknown":[],"unresolved":[{"id":"0123456789ab"}]}"#,
        ),
        (
            "show-held",
            r#"{"action":"show","job":{"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"refused":"other-account","remaining":2,"session":null,"signal":null,"start":[1761650000,123456],"startedAt":1,"state":"stopping","task":null,"uid":502},"ok":true,"op":"job"}"#,
        ),
        (
            "stop-all-held",
            r#"{"action":"stop-all","ended":0,"ok":true,"op":"job","refused":[{"id":"7c2e91a04b3f","reason":"other-account"}],"stopping":0,"unknown":[],"unresolved":[]}"#,
        ),
        (
            "unconfirmed",
            r#"{"code":"job","error":"said","id":"7c2e91a04b3f","job":{"boot":"6A1E0000-0000-0000-0000-000000000000","endsAt":18446744073709551615,"id":"7c2e91a04b3f","logResets":0,"name":"nc","pgid":4312,"pid":4312,"session":null,"start":[1761650000,123456],"startedAt":1,"state":"running","task":null,"uid":502},"ok":false,"reason":"unconfirmed","uncertain":true}"#,
        ),
        (
            "limit",
            r#"{"code":"job","error":"msg","ok":false,"reason":"limit","uncertain":false}"#,
        ),
    ];
    assert_eq!(out.len(), expected.len());
    for ((name, value), (expected_name, expected)) in out.iter().zip(expected) {
        assert_eq!(*name, expected_name);
        assert_eq!(serde_json::to_string(value).unwrap(), expected, "{name}");
    }
}

#[tokio::test]
async fn a_failed_supervisor_never_reports_a_settled_job() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let jobs = Arc::new(jobs);
    let path = folder.path().join(format!("jobs/{ID}.json"));
    let unreadable = folder.path().join("jobs/0123456789ab.json");
    std::fs::write(&unreadable, b"{").unwrap();
    jobs.watch_once(None).unwrap();
    let before = std::fs::read(&path).unwrap();
    table.panic_on_read.store(true, Ordering::SeqCst);
    let (_stopping, stopped) = tokio::sync::watch::channel(false);
    let mut supervisor = Box::pin(supervise(
        jobs.clone(),
        folder.path().join("stop"),
        || false,
        stopped,
    ));
    // Poll directly so the first timer is registered without running a blocking worker.
    // Resume real time before joining that worker: paused-time auto-advance must not
    // race its readiness or turn the outer watchdog into an immediate timeout.
    tokio::time::pause();
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(supervisor.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    tokio::time::advance(Duration::from_millis(999)).await;
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(supervisor.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    assert!(!jobs.unavailable.load(Ordering::SeqCst));
    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::time::resume();
    let outcome = tokio::time::timeout(Duration::from_secs(10), supervisor)
        .await
        .unwrap();
    let error = outcome.unwrap_err();
    assert_eq!(error.code, Code::Job);
    let cause = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<tokio::task::JoinError>()
        .unwrap();
    assert!(cause.is_panic());
    assert!(!json!(error).to_string().contains("private worker cause"));
    assert!(jobs.census().is_err());
    assert!(jobs.running().is_err());
    assert!(jobs.folder().is_err());
    assert!(jobs.interval().is_err());
    assert!(jobs.watch_once(None).is_err());
    for reply in [
        jobs.show(None),
        jobs.log(&record().id, 10),
        jobs.stop(&record().id, &record().expected().unwrap(), Duration::ZERO),
        jobs.stop_all(StopReason::Person),
        jobs.start(JobStart {
            argv: vec!["/usr/bin/true".into()],
            lifetime: Duration::from_secs(60),
            name: None,
            task: None,
            session: None,
        })
        .await,
    ] {
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(reply["code"], "job", "{reply}");
        assert!(reply.get("job").is_none(), "{reply}");
    }
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(std::fs::read(unreadable).unwrap(), b"{");
    let state = jobs
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!((state.running, state.unresolved), (1, 1));
    assert!(table.signals().is_empty());
    assert!(jobs.unavailable.load(Ordering::SeqCst));
}

#[test]
fn a_cancelled_queued_watch_makes_supervision_unavailable() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    let table = Table::with(vec![leader()]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    let jobs = Arc::new(jobs);
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    runtime.block_on(async {
        let blocker = tokio::task::spawn_blocking(move || {
            entered.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        started.await.unwrap();
        let watched = jobs.clone();
        let queued = tokio::task::spawn_blocking(move || watched.watch_once(None));
        queued.abort();
        release.send(()).unwrap();
        blocker.await.unwrap();
        let outcome = queued.await;
        let error = jobs.worker_outcome(outcome).unwrap_err();
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<tokio::task::JoinError>()
                .unwrap()
                .is_cancelled()
        );
        assert!(jobs.census().is_err());
        assert_eq!(jobs.state.lock().unwrap().running, 1);
        assert!(table.signals().is_empty());
    });
}

#[test]
fn poisoned_job_state_is_unavailable_until_reopened() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = jobs.state.lock().unwrap();
            panic!("private poison cause");
        }))
        .is_err()
    );
    assert!(jobs.census().is_err());
    jobs.state.clear_poison();
    assert!(
        jobs.census().is_err(),
        "clearing poison cannot restore supervision"
    );
    assert_eq!(jobs.state.lock().unwrap().running, 1);
    assert_eq!(jobs.show(None)["code"], "job");
    let reopened = Jobs::open_in(
        folder.path(),
        PathBuf::from("/nonexistent"),
        UID,
        LOG_LIMIT,
        Box::new(Shared(table.clone())),
        BOOT.into(),
    )
    .unwrap();
    assert_eq!(reopened.running().unwrap(), 1);
    assert_eq!(shown(&reopened)["state"], "running");
    assert!(table.signals().is_empty());
}

#[tokio::test]
async fn stopping_supervision_joins_the_in_flight_worker() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let jobs = Arc::new(jobs);
    let (entered, watching) = tokio::sync::oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    *table.watching.lock().unwrap() = Some((entered, waiting));
    let (stopping, stopped) = tokio::sync::watch::channel(false);
    let mut supervisor = Box::pin(supervise(
        jobs.clone(),
        folder.path().join("stop"),
        || false,
        stopped,
    ));
    // Register and advance only the first sampling timer. The real worker handshake
    // and its shutdown watchdog run after resuming real time, without auto-advance.
    tokio::time::pause();
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(supervisor.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    tokio::time::advance(Duration::from_millis(999)).await;
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(supervisor.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    assert!(table.watching.lock().unwrap().is_some());
    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::time::resume();
    let supervisor = tokio::spawn(supervisor);
    tokio::time::timeout(Duration::from_secs(10), watching)
        .await
        .unwrap()
        .unwrap();
    stopping.send_replace(true);
    assert!(
        !supervisor.is_finished(),
        "the worker still owns its observation"
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), supervisor)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(jobs.running().unwrap(), 1);
    assert!(table.signals().is_empty());
}

#[tokio::test]
async fn failed_observation_preserves_records_and_child_identity() {
    use std::os::unix::fs::PermissionsExt;
    for failure in ["unavailable", "poisoned"] {
        let table = Table::with(vec![leader()]);
        let (folder, mut jobs) = jobs_with(&record(), &table);
        let launcher = folder.path().join("launcher");
        // The input pipe holds this disposable launcher; its timeout bounds a broken test.
        std::fs::write(&launcher, "#!/bin/sh\nread -r -t 20 go\n").unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
        jobs.launcher = launcher;
        let jobs = Arc::new(jobs);
        let start = JobStart {
            argv: vec!["/usr/bin/true".into()],
            lifetime: Duration::from_secs(60),
            name: None,
            task: None,
            session: None,
        };
        let prepared = jobs.prepare(&start).unwrap();
        let id = prepared.record.id.clone();
        let path = folder.path().join(format!("jobs/{id}.json"));
        let before = std::fs::read(&path).unwrap();
        match failure {
            "poisoned" => assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _state = jobs.state.lock().unwrap();
                    panic!("private poison cause");
                }))
                .is_err()
            ),
            _ => jobs.fail_supervision(),
        }
        let reply = jobs.launch(prepared, &start).await;
        assert_eq!(reply["uncertain"], true, "{reply}");
        assert_eq!(reply["job"]["id"], id.as_str());
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert!(jobs.census().is_err());
        assert!(table.signals().is_empty());
        let mut child = {
            let mut state = jobs
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            assert!(state.launching.contains(&id));
            state.children.remove(&id).unwrap()
        };
        let pid = child.id().unwrap();
        let identity = atlas_core::process::start(i32::try_from(pid).unwrap()).unwrap();
        assert!(
            child.try_wait().unwrap().is_none(),
            "the child was retained alive"
        );
        assert_eq!(
            atlas_core::process::start(i32::try_from(pid).unwrap()).unwrap(),
            identity
        );
        child.kill().await.unwrap();
    }
}

#[tokio::test]
async fn an_unwritable_launch_record_does_not_disable_supervision() {
    use std::os::unix::fs::PermissionsExt;
    let table = Table::with(vec![leader()]);
    let (folder, mut jobs) = jobs_with(&record(), &table);
    let launcher = folder.path().join("launcher");
    std::fs::write(&launcher, "#!/bin/sh\nread -r -t 20 go\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    jobs.launcher = launcher;
    let jobs = Arc::new(jobs);
    let start = JobStart {
        argv: vec!["/usr/bin/true".into()],
        lifetime: Duration::from_secs(60),
        name: None,
        task: None,
        session: None,
    };
    let prepared = jobs.prepare(&start).unwrap();
    let id = prepared.record.id.clone();
    let path = folder.path().join(format!("jobs/{id}.json"));
    let log = folder.path().join(format!("jobs/{id}.log"));
    // Refuse the record write while still allowing record and log cleanup.
    std::fs::create_dir(path.with_extension("json.partial")).unwrap();
    let reply = jobs.launch(prepared, &start).await;
    assert_eq!(
        reply,
        json!({
            "ok": false,
            "code": "spawn-failed",
            "error": "The job's process could not be observed or recorded",
            "uncertain": false,
        })
    );
    assert!(!path.exists());
    assert!(!log.exists());
    assert!(jobs.state().unwrap().children.is_empty());
    assert_eq!(jobs.census().unwrap().running, 1);
    assert_eq!(jobs.show(Some(&record().id))["ok"], true);
    assert!(table.signals().is_empty());
}

#[tokio::test]
async fn a_panicked_launch_confirmation_retains_the_launched_child() {
    use std::os::unix::fs::PermissionsExt;
    let table = Table::with(vec![leader()]);
    let (folder, mut jobs) = jobs_with(&record(), &table);
    let launcher = folder.path().join("launcher");
    // This protocol stand-in has a bounded lifetime; it never receives a group signal.
    std::fs::write(
        &launcher,
        "#!/bin/sh\nread -r -t 20 go\nprintf 'setsid\\n'\nexec 1>&-\nexec /bin/sleep 20\n",
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
    jobs.launcher = launcher;
    let jobs = Arc::new(jobs);
    let start = JobStart {
        argv: vec!["/usr/bin/true".into()],
        lifetime: Duration::from_secs(60),
        name: None,
        task: None,
        session: None,
    };
    let prepared = jobs.prepare(&start).unwrap();
    let id = prepared.record.id.clone();
    table.panic_on_read.store(true, Ordering::SeqCst);
    let reply = jobs.launch(prepared, &start).await;
    assert_eq!(reply["uncertain"], true, "{reply}");
    assert!(jobs.census().is_err());
    let mut child = {
        let mut state = jobs
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Found::Record(record) = state.store.read(&id).unwrap() else {
            panic!("record was lost")
        };
        assert_eq!(record.state, JobState::Launched);
        let child = state.children.remove(&id).unwrap();
        assert_eq!(
            record.pid,
            child.id().map(|pid| i32::try_from(pid).unwrap())
        );
        assert_eq!(
            record.start,
            Some(atlas_core::process::start(record.pid.unwrap()).unwrap())
        );
        child
    };
    assert!(child.try_wait().unwrap().is_none());
    assert!(table.signals().is_empty());
    child.kill().await.unwrap();
}

#[test]
fn reopened_jobs_are_adopted_by_fresh_identity() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let unreadable = folder.path().join("jobs/0123456789ab.json");
    std::fs::write(&unreadable, b"{").unwrap();
    jobs.fail_supervision();
    drop(jobs);
    let reopen = || {
        Jobs::open_in(
            folder.path(),
            PathBuf::from("/nonexistent"),
            UID,
            LOG_LIMIT,
            Box::new(Shared(table.clone())),
            BOOT.into(),
        )
        .unwrap()
    };
    let adopted = reopen();
    assert_eq!(shown(&adopted)["state"], "running");
    assert_eq!(shown(&adopted)["adopted"], true);
    assert_eq!(adopted.census().unwrap().unresolved, 1);
    drop(adopted);
    *table.processes.lock().unwrap() = vec![process(4312, later(1), 4312, 1)];
    let refused = reopen();
    let reply = refused.stop(&record().id, &record().expected().unwrap(), Duration::ZERO);
    assert!(reply.get("signalled").is_none(), "{reply}");
    assert_eq!(shown(&refused)["state"], "ended");
    assert_eq!(std::fs::read(unreadable).unwrap(), b"{");
    assert!(table.signals().is_empty());
}

fn raw_job_files(folder: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(folder.join("jobs"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[tokio::test]
async fn refused_raw_job_fields_never_signal_or_rewrite_the_running_job() {
    use crate::daemon::serve::tests::exchange_with_jobs;

    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let jobs = Arc::new(jobs);
    let before = raw_job_files(folder.path());
    let reads = table.reads.load(Ordering::SeqCst);
    let expected = json!(record().expected().unwrap());
    let start = json!({"op":"job","action":"start","argv":["/usr/bin/true"],
        "uid":UID,"lifetimeMs":1000,"name":"job","task":"123456abcdef",
        "hostSession":"abcdef123456","session":{"securitySessionId":42,"displayId":7}});
    let stop = json!({"op":"job","action":"stop","id":ID,"expect":expected,"graceMs":1000});
    let all = json!({"op":"job","action":"stop-all","reason":"person"});
    let mut refused = Vec::new();
    for (request, fields) in [
        (&start, &["op", "action", "argv", "uid", "lifetimeMs"][..]),
        (&stop, &["op", "action", "id", "expect"][..]),
        (&all, &["op", "action", "reason"][..]),
    ] {
        for field in fields {
            let mut missing = (*request).clone();
            missing.as_object_mut().unwrap().remove(*field);
            refused.push(missing.to_string());
            for value in [Value::Null, json!(true), json!([]), json!({})] {
                let mut malformed = (*request).clone();
                malformed[*field] = value;
                refused.push(malformed.to_string());
            }
        }
    }
    for field in ["name", "task", "hostSession", "session"] {
        for value in [json!(true), json!(7), json!([]), json!({})] {
            let mut malformed = start.clone();
            malformed[field] = value;
            refused.push(malformed.to_string());
        }
    }
    for field in ["boot", "pid", "start", "pgid"] {
        let mut missing = stop.clone();
        missing["expect"].as_object_mut().unwrap().remove(field);
        refused.push(missing.to_string());
        for value in [Value::Null, json!(true), json!([]), json!({})] {
            let mut malformed = stop.clone();
            malformed["expect"][field] = value;
            refused.push(malformed.to_string());
        }
    }
    for (request, field, value) in [
        (&start, "argv", json!([])),
        (&start, "argv", json!([""])),
        (&start, "argv", json!(["/usr/bin/true", null])),
        (&start, "lifetimeMs", json!(999)),
        (&start, "lifetimeMs", json!(86400001)),
        (&start, "uid", json!(-1)),
        (&start, "uid", json!(4294967296_u64)),
        (&start, "name", json!("")),
        (&start, "name", json!("é".repeat(65))),
        (&start, "name", json!("a\nb")),
        (&start, "task", json!("ABCDEF123456")),
        (&start, "hostSession", json!("../session")),
        (&start, "session", json!({"securitySessionId":42})),
        (
            &start,
            "session",
            json!({"securitySessionId":42,"displayId":7,"uid":UID}),
        ),
        (&stop, "id", json!("7C2E91A04B3F")),
        (&stop, "id", json!("7c2e91a04b3")),
        (&stop, "graceMs", json!(999)),
        (&stop, "graceMs", json!(60001)),
        (&stop, "graceMs", json!("1000")),
        (
            &stop,
            "expect",
            json!({"boot":BOOT,"pid":4312,"start":START,"pgid":4312,"uid":UID}),
        ),
        (&all, "reason", json!("later")),
    ] {
        let mut malformed = (*request).clone();
        malformed[field] = value;
        refused.push(malformed.to_string());
    }
    refused.push(format!(
        r#"{{"op":"job","action":"stop","id":"{ID}","expect":{{"boot":"{BOOT}","pid":4312,"\u0070id":null,"start":{start},"pgid":4312}}}}"#,
        start = json!(START)
    ));
    for request in refused {
        let replies = exchange_with_jobs(jobs.clone(), &request).await;
        assert_eq!(replies.len(), 1, "{request}: {replies:?}");
        assert_eq!(replies[0]["code"], "bad-request", "{request}: {replies:?}");
        assert_eq!(replies[0]["uncertain"], false, "{request}");
        assert!(table.signals().is_empty(), "{request}");
        assert_eq!(table.reads.load(Ordering::SeqCst), reads, "{request}");
        assert_eq!(raw_job_files(folder.path()), before, "{request}");
    }

    let replies = exchange_with_jobs(jobs, &stop.to_string()).await;
    assert_eq!(replies[0]["ok"], true, "{replies:?}");
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
    assert!(table.reads.load(Ordering::SeqCst) > reads);
    assert_ne!(raw_job_files(folder.path()), before);
}

#[tokio::test]
async fn raw_job_stop_defaults_and_duplicate_members_keep_the_selected_effect() {
    use crate::daemon::serve::tests::exchange_with_jobs;

    let expected = json!(record().expected().unwrap());
    for (fields, grace) in [
        (String::new(), 5000),
        (r#", "graceMs":null"#.into(), 5000),
        (r#", "graceMs":0,"graceMs":1000"#.into(), 1000),
        (r#", "graceMs":1000,"\u0067raceMs":null"#.into(), 5000),
        (r#", "other":null,"other":true"#.into(), 5000),
        (
            format!(
                r#", "expect":{{"pid":null,"boot":"{BOOT}","start":{start},"pgid":4312,"\u0070id":4312}}"#,
                start = json!(START)
            ),
            5000,
        ),
    ] {
        let table = Table::with(vec![leader()]);
        let (folder, jobs) = jobs_with(&record(), &table);
        let jobs = Arc::new(jobs);
        let request = format!(
            r#"{{"op":"unknown","op":"job","action":"stop","id":"{ID}","expect":{expected}{fields}}}"#
        );
        let replies = exchange_with_jobs(jobs.clone(), &request).await;
        assert_eq!(replies[0]["ok"], true, "{request}: {replies:?}");
        assert_eq!(table.signals(), [(4312, Signal::SIGTERM)], "{request}");
        let stored: Record = serde_json::from_slice(
            &std::fs::read(folder.path().join("jobs").join(format!("{ID}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(stored.state, JobState::Stopping, "{request}");
        assert_eq!(stored.stopping.unwrap().grace_ms, grace, "{request}");
        assert!(jobs.state().unwrap().children.is_empty());
    }
}

#[tokio::test]
async fn raw_job_stop_all_reasons_signal_only_the_recorded_group() {
    use crate::daemon::serve::tests::exchange_with_jobs;

    for reason in [
        json!("person"),
        json!("on-console"),
        json!("stopped"),
        json!({"person":null}),
    ] {
        let table = Table::with(vec![leader(), process(9000, later(1), 9000, 1)]);
        let (folder, jobs) = jobs_with(&record(), &table);
        let jobs = Arc::new(jobs);
        let request = format!(
            r#"{{"op":"job","action":"stop-all","reason":"later","reason":{reason},"other":1}}"#
        );
        let replies = exchange_with_jobs(jobs.clone(), &request).await;
        assert_eq!(replies[0]["ok"], true, "{request}: {replies:?}");
        assert_eq!(replies[0]["stopping"], 1);
        assert_eq!(table.signals(), [(4312, Signal::SIGTERM)], "{request}");
        assert_eq!(raw_job_files(folder.path()).len(), 1);
        assert!(jobs.state().unwrap().children.is_empty());
    }
}

#[tokio::test]
async fn raw_job_expectation_mismatches_never_signal_or_rewrite_the_record() {
    use crate::daemon::serve::tests::exchange_with_jobs;

    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let jobs = Arc::new(jobs);
    let before = raw_job_files(folder.path());
    let expected = json!(record().expected().unwrap());
    for (field, value) in [
        ("boot", json!("")),
        ("boot", json!("another-boot")),
        ("pid", json!(0)),
        ("pid", json!(4313)),
        ("pgid", json!(-1)),
        ("pgid", json!(4313)),
        ("start", json!([0, 0])),
        ("start", json!([START[0], 1000000])),
        ("start", json!(later(1))),
    ] {
        let mut mismatch = expected.clone();
        mismatch[field] = value;
        let request = json!({"op":"job","action":"stop","id":ID,"expect":mismatch});
        let replies = exchange_with_jobs(jobs.clone(), &request.to_string()).await;
        assert_eq!(replies[0]["ok"], false, "{request}: {replies:?}");
        assert_eq!(replies[0]["reason"], "identity", "{request}");
        assert_eq!(replies[0]["uncertain"], false, "{request}");
        assert!(table.signals().is_empty(), "{request}");
        assert_eq!(raw_job_files(folder.path()), before, "{request}");
    }
    let request = json!({"op":"job","action":"stop","id":ID,"expect":expected});
    let replies = exchange_with_jobs(jobs, &request.to_string()).await;
    assert_eq!(replies[0]["ok"], true, "{replies:?}");
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
    assert_ne!(raw_job_files(folder.path()), before);
}
