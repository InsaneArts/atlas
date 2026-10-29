use super::*;
use crate::process::{Info, Instance};
use nix::errno::Errno;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const UID: u32 = 502;
const BOOT: &str = "6A1E0000-0000-0000-0000-000000000000";
const START: [u64; 2] = [1_761_650_000, 123_456];
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

fn process(pid: i32, start: [u64; 2], pgid: i32, ppid: i32) -> Info {
    Info {
        instance: Instance { pid, start },
        ppid,
        pgid,
        uid: UID,
        ruid: UID,
        svuid: UID,
        zombie: false,
        name: "sleep".into(),
    }
}

fn leader() -> Info {
    process(4312, START, 4312, 1)
}

fn later(by: u64) -> [u64; 2] {
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
    assert!(JobIdentity::new(BOOT, 4312, START, 4312).is_some());
    for (pid, start, pgid) in [
        (4312, START, 4313),
        (0, START, 0),
        (1, START, 1),
        (100_000, START, 100_000),
        (-5, START, -5),
        (4312, [0, 1], 4312),
        (4312, [1, 1_000_000], 4312),
    ] {
        assert!(
            JobIdentity::new(BOOT, pid, start, pgid).is_none(),
            "{pid} {pgid}"
        );
    }
    assert!(JobIdentity::new("", 4312, START, 4312).is_none());

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
    observe(&mut noted, &job, &[leader(), child], &processes, |_| None);
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
    observe(&mut noted, &job, &processes, &processes, |_| None);
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
    let view = record.view();
    assert_eq!(view["how"], "leader-gone");
    assert_eq!(view["remaining"], 1);
    assert!(view.get("argv").is_none(), "arguments are never kept");
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
        if self.failing.load(Ordering::SeqCst) {
            return Err(Error::new("state", "The process list is unavailable"));
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
fn an_identity_refusal_followed_by_a_stop_never_signals() {
    // The PID's group is another's now: its leader moved to a group whose
    // ID is not its own PID.
    let moved = process(4312, START, 9999, 1);
    let table = Table::with(vec![moved, process(4313, later(1), 9999, 4312)]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    for _ in 0..3 {
        jobs.watch_once(true, false);
    }
    assert_eq!(shown(&jobs)["refused"], "identity");
    let refused = jobs.stop(
        &JobId::parse(ID).unwrap(),
        &record().expected().unwrap(),
        Duration::from_secs(1),
    );
    assert!(refused.get("signalled").is_none(), "{refused}");
    for _ in 0..3 {
        jobs.watch_once(true, false);
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
    jobs.watch_once(true, true);
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
    jobs.watch_once(true, false);
    assert!(table.signals().is_empty());
}

#[test]
fn a_failed_process_query_ends_nothing_and_is_tried_again() {
    let table = Table::with(vec![leader()]);
    let mut due = record();
    due.ends_at = 0;
    let (_folder, jobs) = jobs_with(&due, &table);
    table.failing.store(true, Ordering::SeqCst);
    jobs.watch_once(false, false);
    jobs.watch_once(false, false);
    let job = shown(&jobs);
    assert_eq!(job["state"], "stopping", "{job}");
    assert_eq!(job["refused"], "unobserved");
    assert!(table.signals().is_empty());

    table.failing.store(false, Ordering::SeqCst);
    jobs.watch_once(false, false);
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
    jobs.watch_once(true, false);
    let job = shown(&jobs);
    std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
    assert_eq!(job["state"], "stopping", "kept in memory: {job}");
    assert_eq!(jobs.running(), 1);
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
    jobs.watch_once(false, false);
    assert_eq!(std::fs::metadata(&log).unwrap().len(), 0);
    assert_eq!(shown(&jobs)["logResets"], 1);
}

#[test]
fn every_job_is_counted_from_the_records() {
    let table = Table::with(vec![leader()]);
    let (_folder, jobs) = jobs_with(&record(), &table);
    assert_eq!(jobs.running(), 1);
    table.processes.lock().unwrap().clear();
    jobs.watch_once(false, false);
    assert_eq!(jobs.running(), 0);
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
        jobs.watch_once(true, false);
    }
    assert!(table.signals().is_empty(), "{:?}", table.signals());
}

#[test]
fn a_record_that_cannot_be_read_is_counted_unresolved_and_named_by_a_stop_of_every_job() {
    let table = Table::with(vec![leader()]);
    let (folder, jobs) = jobs_with(&record(), &table);
    let other = JobId::parse("0123456789ab").unwrap();
    std::fs::write(folder.path().join(format!("jobs/{other}.json")), b"{").unwrap();
    jobs.watch_once(false, false);
    assert_eq!(
        jobs.census(),
        json!({"running": 1, "launching": 0, "unresolved": 1})
    );
    let reply = jobs.stop_all(StopReason::Person);
    assert_eq!(reply["unresolved"], json!([{"id": other}]), "{reply}");
    assert_eq!(table.signals(), [(4312, Signal::SIGTERM)]);
}
