use super::*;

const UID: u32 = 502;
const BOOT: &str = "6A1E0000-0000-0000-0000-000000000000";
const START: [u64; 2] = [1_761_650_000, 123_456];

fn record() -> Record {
    Record {
        version: VERSION,
        id: JobId::parse("7c2e91a04b3f").unwrap(),
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
        ends_at: 2,
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

fn judged(expect: Option<&Expected>, leader: Option<&Info>, members: &[Info]) -> Verdict {
    verdict(&record(), expect, BOOT, UID, leader, members)
}

#[test]
fn a_matching_job_is_signalled_only_through_its_own_group() {
    let members = [leader(), process(4313, later(1), 4312, 4312)];
    let expected = record().expected().unwrap();
    assert_eq!(
        judged(Some(&expected), Some(&members[0]), &members),
        Verdict::Signal { pgid: 4312 }
    );
    assert_eq!(
        judged(None, Some(&members[0]), &members),
        Verdict::Signal { pgid: 4312 }
    );
    // A leader that has exited leaves its group to its other members.
    assert_eq!(
        judged(None, None, &members[1..]),
        Verdict::Signal { pgid: 4312 }
    );
}

#[test]
fn a_reused_pid_is_never_signalled() {
    let reused = process(4312, later(60), 4312, 1);
    assert_eq!(
        judged(None, Some(&reused), std::slice::from_ref(&reused)),
        Verdict::Reused
    );
}

#[test]
fn a_record_that_disagrees_with_the_person_is_never_signalled() {
    let members = [leader()];
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
            judged(Some(&expect), Some(&members[0]), &members),
            Verdict::Identity
        );
    }
}

#[test]
fn a_member_running_as_another_account_stops_the_whole_stop() {
    let mut setuid = process(4313, later(1), 4312, 4312);
    setuid.uid = 0;
    let members = [leader(), setuid.clone()];
    assert_eq!(
        judged(None, Some(&members[0]), &members),
        Verdict::OtherAccount
    );

    let mut real = setuid;
    real.uid = UID;
    real.ruid = 501;
    let members = [leader(), real];
    assert_eq!(
        judged(None, Some(&members[0]), &members),
        Verdict::OtherAccount
    );

    let mut theirs = leader();
    theirs.ruid = 501;
    assert_eq!(
        judged(None, Some(&theirs), std::slice::from_ref(&theirs)),
        Verdict::OtherAccount
    );

    let mut record = record();
    record.uid = 501;
    assert_eq!(
        verdict(&record, None, BOOT, UID, Some(&leader()), &[leader()]),
        Verdict::OtherAccount
    );
}

#[test]
fn a_member_older_than_the_job_is_refused() {
    let older = process(4313, [START[0] - 1, START[1]], 4312, 1);
    let members = [leader(), older];
    assert_eq!(judged(None, Some(&members[0]), &members), Verdict::Identity);
}

#[test]
fn a_leader_in_another_group_is_refused() {
    let moved = process(4312, START, 9999, 1);
    assert_eq!(judged(None, Some(&moved), &[]), Verdict::Identity);
}

#[test]
fn a_job_from_another_boot_has_ended() {
    let mut record = record();
    record.boot = "an earlier boot".into();
    assert_eq!(
        verdict(&record, None, BOOT, UID, Some(&leader()), &[leader()]),
        Verdict::Restarted
    );
}

#[test]
fn a_group_with_no_members_has_ended() {
    assert_eq!(judged(None, None, &[]), Verdict::Ended);

    // Only the daemon's own unreaped leader is left.
    let mut zombie = leader();
    zombie.zombie = true;
    assert_eq!(
        judged(None, Some(&zombie), std::slice::from_ref(&zombie)),
        Verdict::Ended
    );
}

#[test]
fn a_record_without_a_process_is_never_signalled() {
    let mut record = record();
    record.pid = None;
    assert_eq!(
        verdict(&record, None, BOOT, UID, None, &[]),
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
    let mut seen = Vec::new();
    observe(&mut seen, &job, &[leader(), child], &processes, |_| None);
    assert!(seen.contains(&escaped.instance));
    assert!(seen.iter().any(|instance| instance.pid == 4401));
    assert!(!seen.iter().any(|instance| instance.pid == 5000));

    // Its parent gone, the escaped process is still known by what was seen;
    // one whose PID is another process's now is not.
    let reused = process(4401, later(30), 4401, 1);
    let now = [leader(), escaped, reused];
    let left = left(&seen, &now, 4312, UID);
    assert_eq!(
        left,
        vec![Left {
            pid: 4400,
            name: "sleep".into()
        }]
    );
}

#[test]
fn a_process_in_the_jobs_session_is_its_own() {
    let job = Instance {
        pid: 4312,
        start: START,
    };
    let moved = process(4500, later(1), 4500, 1);
    let mut seen = Vec::new();
    observe(
        &mut seen,
        &job,
        &[leader()],
        &[leader(), moved.clone()],
        |pid| (pid == 4500).then_some(4312),
    );
    assert!(seen.contains(&moved.instance));
}

#[test]
fn what_was_seen_stays_within_its_bound() {
    let job = Instance {
        pid: 4312,
        start: START,
    };
    let children: Vec<Info> = (0..300)
        .map(|n| process(5000 + n, later(1), 4312, 4312))
        .collect();
    let mut processes = vec![leader()];
    processes.extend(children.iter().cloned());
    let mut seen = Vec::new();
    observe(&mut seen, &job, &processes, &processes, |_| None);
    assert_eq!(seen.len(), SEEN_MAX);
}

#[test]
fn a_job_record_reads_back_as_it_was_written() {
    let mut record = record();
    record.state = JobState::Ended;
    record.ended = Some(Ended {
        how: How::OnConsole,
        at: 5,
        signal: Some("SIGTERM".into()),
        exit_code: Some(0),
        exit_signal: None,
        left: vec![Left {
            pid: 9,
            name: "sleep".into(),
        }],
    });
    let text = serde_json::to_string(&record).unwrap();
    assert!(text.contains("\"how\":\"on-console\""), "{text}");
    assert_eq!(serde_json::from_str::<Record>(&text).unwrap(), record);

    let view = record.view();
    assert_eq!(view["how"], "on-console");
    assert_eq!(view["left"][0]["pid"], 9);
    assert!(view.get("argv").is_none(), "arguments are never kept");
}
