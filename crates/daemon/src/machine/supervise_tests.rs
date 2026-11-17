use super::*;
use atlas_session::clock::Manual;
use std::{
    os::{fd::OwnedFd, unix::process::ExitStatusExt},
    process::{Child, ExitStatus},
    sync::mpsc,
    time::Instant,
};

/// The supervisor these tests run in a child of their own: the test binary
/// again, running only this test, with the invocation's words after `--`.
#[test]
fn a_supervisor_runs_what_it_is_given_when_started_for_these_tests() {
    let words: Vec<OsString> = std::env::args_os()
        .skip_while(|word| word != "--")
        .skip(1)
        .collect();
    if words.is_empty() {
        return;
    }
    std::process::exit(i32::from(start(&words)));
}

/// Where the supervisors these tests start leave their proofs.
fn proofs() -> PathBuf {
    static PROOFS: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    PROOFS
        .get_or_init(|| tempfile::tempdir().unwrap())
        .path()
        .to_owned()
}

/// A supervisor of the program `words` name, in a group of its own, with
/// an 800 ms lease, told to start, and the daemon's end of its lifeline.
fn supervisor(words: &[&str]) -> (Child, UnixStream) {
    let (child, mut lifeline) = silent_supervisor(words);
    lifeline.write_all(&[RENEW]).unwrap();
    (child, lifeline)
}

/// A supervisor as [`supervisor`] starts one, not yet told to start.
fn silent_supervisor(words: &[&str]) -> (Child, UnixStream) {
    use std::os::unix::process::CommandExt;
    let invocation = Invocation {
        lease: Duration::from_millis(800),
        proofs: proofs(),
        argv0: words[0].into(),
        program: words[0].into(),
        arguments: words[1..].iter().map(OsString::from).collect(),
        handed: false,
    };
    let supervisor = invocation::Supervisor::of_these_tests();
    let (daemon, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let mut command = Command::new(supervisor.program());
    command
        .args(supervisor.arguments(&invocation))
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .process_group(0);
    (atlas_core::spawn::spawn(&mut command).unwrap(), daemon)
}

/// How the program ended, as the supervisor says it on `lifeline`, after
/// saying it started.
fn told(lifeline: &mut UnixStream) -> Option<ExitStatus> {
    loop {
        let mut frame = Vec::new();
        let mut byte = [0; 1];
        while frame.len() < invocation::FRAME && lifeline.read(&mut byte).ok()? == 1 {
            frame.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        if frame != invocation::STARTED && frame != invocation::STOPPING {
            return invocation::heard(&frame);
        }
    }
}

/// Whether the group `child` leads ends within `within`.
fn ends_within(child: &mut Child, within: Duration) -> bool {
    let group = Pid::from_raw(child.id() as i32);
    let started = Instant::now();
    while started.elapsed() < within {
        let _ = child.try_wait();
        if killpg(group, None).is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_lifeline_that_closes_ends_the_group_at_once() {
    let (mut child, lifeline) = supervisor(&["/bin/sleep", "30"]);
    std::thread::sleep(Duration::from_millis(300));

    drop(lifeline);

    assert!(ends_within(&mut child, Duration::from_millis(500)));
}

#[test]
fn a_lease_gone_unrenewed_ends_the_group() {
    let (mut child, lifeline) = supervisor(&["/bin/sleep", "30"]);

    // The lifeline stays open; nothing renews the lease.
    assert!(ends_within(&mut child, Duration::from_secs(5)));
    drop(lifeline);
}

#[test]
fn renewals_keep_the_work_running_until_they_stop() {
    let (mut child, mut lifeline) = supervisor(&["/bin/sleep", "30"]);
    let group = Pid::from_raw(child.id() as i32);

    for _ in 0..10 {
        lifeline.write_all(&[RENEW]).unwrap();
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(killpg(group, None).is_ok(), "still running while renewed");

    assert!(ends_within(&mut child, Duration::from_secs(5)));
    drop(lifeline);
}

#[test]
fn the_work_ending_by_itself_is_told_with_its_status_and_ends_its_supervisor() {
    let (mut child, mut lifeline) = supervisor(&["/usr/bin/false"]);
    let _ = lifeline.write_all(&[RENEW]);

    let told = told(&mut lifeline);

    assert_eq!(told.and_then(|status| status.code()), Some(1));
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

#[test]
fn work_left_behind_by_a_program_that_ended_is_supervised_until_it_ends_too() {
    // Its background process outlives the shell, in the shell's group.
    let (mut child, mut lifeline) = supervisor(&["/bin/sh", "-c", "/bin/sleep 30 &"]);
    std::thread::sleep(Duration::from_millis(500));
    lifeline.write_all(&[RENEW]).unwrap();
    assert!(
        killpg(Pid::from_raw(child.id() as i32), None).is_ok(),
        "still supervising what the shell left"
    );
    drop(lifeline);

    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn work_that_leaves_the_group_is_ended_with_it() {
    // A new session of its own, out of the supervisor's group.
    let (mut child, lifeline) = supervisor(&[
        "/usr/bin/perl",
        "-e",
        "use POSIX; POSIX::setsid(); sleep 30",
    ]);
    std::thread::sleep(Duration::from_millis(500));
    let escaped: Vec<i32> = atlas_core::process::children(child.id() as i32).unwrap_or_default();
    assert!(
        !escaped.is_empty(),
        "the program that left the group was found"
    );

    drop(lifeline);

    assert!(ends_within(&mut child, Duration::from_secs(2)));
    for pid in escaped {
        assert!(
            atlas_core::process::info(pid)
                .unwrap()
                .is_none_or(|info| info.zombie)
        );
    }
}

#[test]
fn work_whose_parent_stays_in_the_group_is_ended_once_it_leaves_it() {
    // The shell stays in the group; its child leaves it.
    let (mut child, lifeline) = supervisor(&[
        "/bin/sh",
        "-c",
        "/usr/bin/perl -e 'use POSIX; POSIX::setsid(); sleep 30' & wait",
    ]);
    std::thread::sleep(Duration::from_millis(500));
    let group = child.id() as i32;
    let escaped: Vec<i32> = descendants(group)
        .into_iter()
        .filter(|pid| {
            atlas_core::process::info(*pid)
                .unwrap()
                .is_some_and(|info| info.pgid != group)
        })
        .collect();
    assert!(
        !escaped.is_empty(),
        "the grandchild that left the group was found"
    );

    drop(lifeline);

    assert!(ends_within(&mut child, Duration::from_secs(2)));
    for pid in escaped {
        assert!(
            atlas_core::process::info(pid)
                .unwrap()
                .is_none_or(|info| info.zombie)
        );
    }
}

/// Every process descended from `pid`, as the kernel says now.
fn descendants(pid: i32) -> Vec<i32> {
    let mut found = Vec::new();
    let mut looking = vec![pid];
    while let Some(parent) = looking.pop() {
        for child in atlas_core::process::children(parent).unwrap_or_default() {
            found.push(child);
            looking.push(child);
        }
    }
    found
}

/// A clock the test holds a hand on while a lease reads it.
struct Shared(Arc<Manual>);

impl Clock for Shared {
    fn now(&self) -> Option<Duration> {
        self.0.now()
    }
}

/// A lease of a second on a clock the test moves.
fn lease() -> (Lease<Shared>, Arc<Manual>) {
    let clock = Arc::new(Manual::default());
    let lease = Lease::start(Shared(Arc::clone(&clock)), Duration::from_secs(1)).unwrap();
    (lease, clock)
}

#[test]
fn a_lease_on_a_clock_that_cannot_be_read_never_starts() {
    let clock = Manual::default();
    clock.set(None);

    assert!(Lease::start(clock, Duration::from_secs(1)).is_none());
}

#[test]
fn a_renewal_runs_the_lease_on_from_when_it_was_received() {
    let (lease, clock) = lease();
    clock.advance(Duration::from_millis(900));

    lease.renew();
    clock.advance(Duration::from_millis(900));

    assert!(lease.live());
    clock.advance(Duration::from_millis(100));
    assert!(!lease.live());
}

#[test]
fn a_renewal_received_after_the_lease_ran_out_never_revives_it() {
    let (lease, clock) = lease();
    clock.advance(Duration::from_secs(1));

    lease.renew();

    assert!(!lease.live());
    lease.renew();
    assert!(!lease.live());
}

#[test]
fn a_lost_lease_stays_lost_whatever_renews_it_after() {
    let (lease, _clock) = lease();

    lease.lose();
    lease.renew();

    assert!(!lease.live());
}

#[test]
fn a_renewal_with_the_clock_unreadable_loses_the_lease() {
    let (lease, clock) = lease();
    clock.set(None);

    lease.renew();
    clock.set(Some(Duration::ZERO));

    assert!(!lease.live());
}

#[test]
fn waiting_on_a_lease_ends_once_it_is_lost() {
    let (lease, _clock) = lease();
    let lease = Arc::new(lease);
    let waiting = std::thread::spawn({
        let lease = Arc::clone(&lease);
        move || lease.ended()
    });

    lease.lose();

    waiting.join().unwrap();
}

#[test]
fn waiting_on_a_lease_ends_once_its_time_runs_out() {
    let (lease, clock) = lease();
    let lease = Arc::new(lease);
    let waiting = std::thread::spawn({
        let lease = Arc::clone(&lease);
        move || lease.ended()
    });

    clock.advance(Duration::from_secs(1));

    waiting.join().unwrap();
}

/// A kernel a test makes up: one group, children it will not list, and a
/// listing that waits until the test lets it go.
#[derive(Default)]
struct MadeUp {
    group: Vec<Info>,
    /// Listing children fails, and so does describing any process.
    refusing: bool,
    /// The first listing of children waits for this to be let go.
    held: Mutex<Option<mpsc::Receiver<()>>>,
    /// Processes it describes, each the child of its `ppid`.
    born: Vec<Info>,
    /// Listed in the group the first time only, then gone from it.
    leaving: Mutex<Option<Info>>,
}

impl Kernel for MadeUp {
    fn group(&self, _pgid: i32) -> atlas_core::error::Result<Vec<Info>> {
        let mut group = self.group.clone();
        group.extend(self.leaving.lock().unwrap().take());
        Ok(group)
    }

    fn children(&self, pid: i32) -> atlas_core::error::Result<Vec<i32>> {
        let held = self.held.lock().unwrap().take();
        if let Some(held) = held {
            let _ = held.recv();
        }
        if self.refusing {
            return Err(std::io::Error::other("refused").into());
        }
        Ok(self
            .born
            .iter()
            .filter(|info| info.ppid == pid)
            .map(|info| info.instance.pid)
            .collect())
    }

    fn info(&self, pid: i32) -> atlas_core::error::Result<Option<Info>> {
        if self.refusing {
            return Err(std::io::Error::other("refused").into());
        }
        Ok(self
            .born
            .iter()
            .find(|info| info.instance.pid == pid)
            .cloned())
    }

    fn ended(&self, _instance: &Instance) -> bool {
        false
    }
}

/// A process the made-up kernel lists in group `pgid`.
fn listed(pid: i32, pgid: i32) -> Info {
    Info {
        instance: Instance::new(pid, [1, 0]),
        ppid: 1,
        pgid,
        uid: 0,
        ruid: 0,
        svuid: 0,
        zombie: false,
        stopped: false,
        name: String::new(),
    }
}

fn own() -> i32 {
    std::process::id() as i32
}

#[test]
fn a_look_that_fails_after_the_group_never_makes_the_supervisor_a_target() {
    let kernel = MadeUp {
        group: vec![listed(own(), 77), listed(99_001, 77)],
        refusing: true,
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.look(), None);

    assert_eq!(work.tracked(), vec![Instance::new(99_001, [1, 0])]);
}

#[test]
fn ending_never_waits_on_a_look_held_up_in_the_kernel() {
    let (letting, held) = mpsc::channel();
    let kernel = MadeUp {
        group: vec![listed(99_002, 77)],
        held: Mutex::new(Some(held)),
        ..MadeUp::default()
    };
    let work = Arc::new(Work::new(kernel, Pid::from_raw(77)));
    let looking = std::thread::spawn({
        let work = Arc::clone(&work);
        move || work.look()
    });
    std::thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    let targets = work.tracked();

    assert!(started.elapsed() < Duration::from_millis(50));
    assert_eq!(targets, vec![Instance::new(99_002, [1, 0])]);
    letting.send(()).unwrap();
    assert_eq!(looking.join().unwrap(), Some(true));
}

#[test]
fn a_process_seen_once_is_ended_while_the_kernel_cannot_say_it_has() {
    let kernel = MadeUp {
        group: vec![listed(99_003, 77)],
        ..MadeUp::default()
    };
    let mut work = Work::new(kernel, Pid::from_raw(77));
    assert_eq!(work.look(), Some(true));

    // It left the group, and the kernel no longer describes anything.
    work.kernel.group.clear();
    work.kernel.refusing = true;

    assert_eq!(work.look(), None);
    assert_eq!(work.tracked(), vec![Instance::new(99_003, [1, 0])]);
}

#[test]
fn a_program_ended_by_a_signal_is_told_as_ended_by_it() {
    let (mut child, mut lifeline) = supervisor(&["/usr/bin/perl", "-e", "kill 'KILL', $$"]);

    let told = told(&mut lifeline);

    assert_eq!(told.and_then(|status| status.signal()), Some(9));
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

#[test]
fn a_program_is_told_as_ended_while_what_it_left_still_runs() {
    let (mut child, mut lifeline) = supervisor(&["/bin/sh", "-c", "/bin/sleep 30 & exit 3"]);
    lifeline.write_all(&[RENEW]).unwrap();

    let told = told(&mut lifeline);

    assert_eq!(told.and_then(|status| status.code()), Some(3));
    assert!(killpg(Pid::from_raw(child.id() as i32), None).is_ok());
    drop(lifeline);
    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn the_supervisor_keeps_none_of_the_output_open_once_the_program_has_it() {
    use std::os::unix::process::CommandExt;
    let invocation = Invocation {
        lease: Duration::from_millis(800),
        proofs: proofs(),
        argv0: "sh".into(),
        program: "/bin/sh".into(),
        arguments: ["-c", "echo said; /bin/sleep 30 >/dev/null 2>&1 &"]
            .map(OsString::from)
            .to_vec(),
        handed: false,
    };
    let supervisor = invocation::Supervisor::of_these_tests();
    let (mut lifeline, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (mut output, writing) = atlas_core::spawn::pipe().unwrap();
    let mut command = Command::new(supervisor.program());
    command
        .args(supervisor.arguments(&invocation))
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(writing)))
        .process_group(0);
    let mut child = atlas_core::spawn::spawn(&mut command).unwrap();
    drop(command);
    lifeline.write_all(&[RENEW]).unwrap();

    // Read to the end: what the shell left closed its output, and the
    // supervisor, still supervising it, holds none.
    let mut said = String::new();
    output.read_to_string(&mut said).unwrap();

    assert!(said.ends_with("said\n"), "{said:?}");
    assert!(killpg(Pid::from_raw(child.id() as i32), None).is_ok());
    drop(lifeline);
    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn sigterm_reaches_the_program_with_its_default_action() {
    let (mut child, mut lifeline) = supervisor(&["/bin/sleep", "30"]);
    lifeline.write_all(&[RENEW]).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();

    let told = told(&mut lifeline);
    assert_eq!(told.and_then(|status| status.signal()), Some(15));
    drop(lifeline);
    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn sigterm_ends_work_that_left_the_group_even_when_it_ignores_it() {
    let (mut child, mut lifeline) = supervisor(&[
        "/usr/bin/perl",
        "-e",
        "use POSIX; POSIX::setsid(); $SIG{TERM} = 'IGNORE'; sleep 30",
    ]);
    std::thread::sleep(Duration::from_millis(500));
    lifeline.write_all(&[RENEW]).unwrap();
    let escaped = descendants(child.id() as i32);
    assert!(!escaped.is_empty());
    let renewing = std::thread::spawn(move || {
        for _ in 0..20 {
            if lifeline.write_all(&[RENEW]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });

    // Only the supervisor is asked; the lease stays renewed.
    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();

    assert!(ends_within(&mut child, Duration::from_secs(3)));
    for pid in escaped {
        assert!(
            atlas_core::process::info(pid)
                .unwrap()
                .is_none_or(|info| info.zombie)
        );
    }
    renewing.join().unwrap();
}

#[test]
fn a_supervisor_never_told_to_start_starts_nothing_and_ends_by_itself() {
    let folder = tempfile::tempdir().unwrap();
    let mark = folder.path().join("ran");
    let (mut child, _lifeline) = silent_supervisor(&["/usr/bin/touch", mark.to_str().unwrap()]);

    let status = child.wait().unwrap();

    assert_eq!(status.code(), Some(i32::from(ENDED)));
    assert!(!mark.exists(), "the program ran");
}

#[test]
fn a_supervisor_told_to_start_starts_its_program() {
    let folder = tempfile::tempdir().unwrap();
    let mark = folder.path().join("ran");
    let (mut child, mut lifeline) = silent_supervisor(&["/usr/bin/touch", mark.to_str().unwrap()]);

    lifeline.write_all(&[RENEW]).unwrap();

    assert_eq!(
        told(&mut lifeline).and_then(|status| status.code()),
        Some(0)
    );
    assert!(mark.exists());
    assert_eq!(child.wait().unwrap().code(), Some(i32::from(DONE)));
}

#[test]
fn a_supervisor_whose_lease_ends_ends_its_work_and_says_so_by_its_own_code() {
    let (mut child, _lifeline) = supervisor(&["/bin/sh", "-c", "/bin/sleep 30 & /bin/sleep 30"]);

    let status = child.wait().unwrap();

    assert_eq!(status.signal(), None);
    assert_eq!(status.code(), Some(i32::from(ENDED)));
}

/// A PID no process can have: macOS hands out none above 99,999.
const NO_PROCESS: i32 = 100_500;

#[test]
fn ending_with_only_the_supervisor_left_is_ended() {
    let kernel = MadeUp {
        group: vec![listed(own(), 77)],
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.ending(), ENDED);
}

#[test]
fn ending_with_a_member_that_will_not_go_cannot_say_it_ended() {
    let kernel = MadeUp {
        group: vec![listed(own(), 77), listed(NO_PROCESS, 77)],
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.ending(), UNSURE);
}

#[test]
fn ending_with_a_tracked_process_not_known_to_have_ended_cannot_say_it_ended() {
    let kernel = MadeUp {
        group: vec![listed(NO_PROCESS, 77)],
        ..MadeUp::default()
    };
    let mut work = Work::new(kernel, Pid::from_raw(77));
    assert_eq!(work.look(), Some(true));
    // It left the group; the kernel cannot say it ended.
    work.kernel.group = vec![listed(own(), 77)];

    assert_eq!(work.ending(), UNSURE);
}

#[test]
fn ending_never_signals_a_process_whose_identity_cannot_be_read() {
    let mut command = Command::new("/bin/sleep");
    command.arg("30");
    let mut child = atlas_core::spawn::spawn(&mut command).unwrap();
    let pid = child.id() as i32;
    let kernel = MadeUp {
        group: vec![listed(own(), 77), listed(pid, 77)],
        refusing: true,
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.ending(), UNSURE);

    assert_eq!(child.try_wait().unwrap(), None, "it was signalled");
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn ending_finds_a_program_that_left_the_group_before_any_look() {
    let kernel = MadeUp {
        group: vec![listed(own(), 77)],
        born: vec![Info {
            ppid: own(),
            ..listed(NO_PROCESS, 78)
        }],
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.ending(), UNSURE, "the program was never found");
}

#[test]
fn ending_keeps_a_member_that_left_the_group_between_rounds() {
    let kernel = MadeUp {
        group: vec![listed(own(), 77)],
        leaving: Mutex::new(Some(listed(NO_PROCESS, 77))),
        ..MadeUp::default()
    };
    let work = Work::new(kernel, Pid::from_raw(77));

    assert_eq!(work.ending(), UNSURE, "a member was forgotten once it left");
}

#[test]
fn a_pid_taken_by_another_process_never_brings_in_that_process_s_children() {
    let kernel = MadeUp {
        group: vec![listed(NO_PROCESS, 77)],
        ..MadeUp::default()
    };
    let mut work = Work::new(kernel, Pid::from_raw(77));
    assert_eq!(work.look(), Some(true));
    // It left the group, and another process took its PID, with a child.
    work.kernel.group = vec![listed(own(), 77)];
    work.kernel.born = vec![
        Info {
            instance: Instance::new(NO_PROCESS, [2, 0]),
            ..listed(NO_PROCESS, 90)
        },
        Info {
            ppid: NO_PROCESS,
            ..listed(NO_PROCESS + 1, 90)
        },
    ];

    work.look();

    assert!(
        !work
            .tracked()
            .contains(&Instance::new(NO_PROCESS + 1, [1, 0])),
        "an unrelated child was taken for work"
    );
}

#[test]
fn a_stop_gives_the_work_its_whole_grace_however_it_is_signalled_meanwhile() {
    let (mut child, mut lifeline) =
        supervisor(&["/usr/bin/perl", "-e", "$SIG{TERM} = 'IGNORE'; sleep 30"]);
    let group = Pid::from_raw(child.id() as i32);
    let mut started = [0; 8];
    lifeline.read_exact(&mut started).unwrap();
    assert_eq!(&started, invocation::STARTED);
    // Long enough for the program to set itself to ignore TERM.
    std::thread::sleep(Duration::from_millis(500));
    lifeline.write_all(&[RENEW]).unwrap();
    let [high, low] = 3_000_u16.to_be_bytes();
    lifeline.write_all(&[RENEW, STOP, high, low]).unwrap();
    let mut stopping = [0; 9];
    lifeline.read_exact(&mut stopping).unwrap();
    assert_eq!(&stopping, invocation::STOPPING);
    let mut renewing = lifeline.try_clone().unwrap();
    let renewals = std::thread::spawn(move || {
        for _ in 0..30 {
            if renewing.write_all(&[RENEW]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    // Its own TERM to its group reaches the supervisor too, and so may
    // another: neither cuts the grace to its own.
    kill(group, Signal::SIGTERM).unwrap();

    std::thread::sleep(Duration::from_millis(2_200));
    assert!(
        killpg(group, None).is_ok(),
        "ended before its grace was over"
    );
    assert!(ends_within(&mut child, Duration::from_secs(3)));
    drop(lifeline);
    renewals.join().unwrap();
}

#[test]
fn a_stop_is_cut_short_once_the_lifeline_goes() {
    let (mut child, mut lifeline) =
        supervisor(&["/usr/bin/perl", "-e", "$SIG{TERM} = 'IGNORE'; sleep 30"]);
    let mut started = [0; 8];
    lifeline.read_exact(&mut started).unwrap();
    let [high, low] = 60_000_u16.to_be_bytes();
    lifeline.write_all(&[RENEW, STOP, high, low]).unwrap();
    let mut stopping = [0; 9];
    lifeline.read_exact(&mut stopping).unwrap();

    drop(lifeline);

    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn a_stop_is_said_to_have_begun_even_when_the_program_ends_at_once_on_it() {
    let (mut child, mut lifeline) = supervisor(&["/bin/sleep", "30"]);
    let mut started = [0; 8];
    lifeline.read_exact(&mut started).unwrap();
    let [high, low] = 5_000_u16.to_be_bytes();

    lifeline.write_all(&[RENEW, STOP, high, low]).unwrap();

    let mut said = Vec::new();
    let _ = lifeline.read_to_end(&mut said);
    assert!(said.starts_with(invocation::STOPPING), "{said:?}");
    assert!(ends_within(&mut child, Duration::from_secs(2)));
}

#[test]
fn a_second_stop_never_shortens_the_first_one_s_grace() {
    let (mut child, mut lifeline) =
        supervisor(&["/usr/bin/perl", "-e", "$SIG{TERM} = 'IGNORE'; sleep 30"]);
    let group = Pid::from_raw(child.id() as i32);
    let mut started = [0; 8];
    lifeline.read_exact(&mut started).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let [long_high, long_low] = 3_000_u16.to_be_bytes();
    let [short_high, short_low] = 1_000_u16.to_be_bytes();

    lifeline
        .write_all(&[
            RENEW, STOP, long_high, long_low, STOP, short_high, short_low,
        ])
        .unwrap();

    let mut renewing = lifeline.try_clone().unwrap();
    let renewals = std::thread::spawn(move || {
        for _ in 0..30 {
            if renewing.write_all(&[RENEW]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    std::thread::sleep(Duration::from_millis(2_000));
    assert!(killpg(group, None).is_ok(), "the second stop's grace won");
    assert!(ends_within(&mut child, Duration::from_secs(3)));
    drop(lifeline);
    renewals.join().unwrap();
}

/// A supervisor of `words`, handed a packet when `handed`, its output read
/// here: its child, the daemon's end of its lifeline, and its output.
fn heard_supervisor(words: &[&str], handed: bool) -> (Child, UnixStream, std::io::PipeReader) {
    use std::os::unix::process::CommandExt;
    let invocation = Invocation {
        lease: Duration::from_millis(800),
        proofs: proofs(),
        argv0: words[0].into(),
        program: words[0].into(),
        arguments: words[1..].iter().map(OsString::from).collect(),
        handed,
    };
    let supervisor = invocation::Supervisor::of_these_tests();
    let (lifeline, theirs) = atlas_core::spawn::socket_pair().unwrap();
    let (output, writing) = atlas_core::spawn::pipe().unwrap();
    let mut command = Command::new(supervisor.program());
    command
        .args(supervisor.arguments(&invocation))
        .stdin(Stdio::from(OwnedFd::from(theirs)))
        .stdout(Stdio::null())
        .stderr(Stdio::from(OwnedFd::from(writing)))
        .process_group(0);
    let child = atlas_core::spawn::spawn(&mut command).unwrap();
    drop(command);
    (child, lifeline, output)
}

/// The frame that hands `packet` over the lifeline.
fn packet_frame(packet: &[u8]) -> Vec<u8> {
    let mut frame = vec![PACKET];
    frame.extend(u32::try_from(packet.len()).unwrap().to_be_bytes());
    frame.extend(packet);
    frame
}

#[test]
fn a_handed_program_reads_its_packet_on_its_standard_input_and_nothing_more() {
    // Larger than a socket's buffer: written while the program reads it.
    let packet: Vec<u8> = (0..200_000).map(|at| b'a' + (at % 26) as u8).collect();
    let (mut child, mut lifeline, mut output) = heard_supervisor(&["/bin/cat"], true);

    // Read as it comes: more than the output pipe holds.
    let reading = std::thread::spawn(move || {
        let mut said = Vec::new();
        output.read_to_end(&mut said).unwrap();
        said
    });
    lifeline.write_all(&packet_frame(&packet)).unwrap();
    lifeline.write_all(&[RENEW]).unwrap();

    assert_eq!(
        told(&mut lifeline).and_then(|status| status.code()),
        Some(0)
    );
    assert_eq!(reading.join().unwrap(), packet);
    assert_eq!(child.wait().unwrap().code(), Some(i32::from(DONE)));
}

#[test]
fn a_program_not_handed_a_packet_reads_nothing_on_its_standard_input() {
    let (mut child, mut lifeline, mut output) = heard_supervisor(&["/bin/cat"], false);

    lifeline.write_all(&[RENEW]).unwrap();

    assert_eq!(
        told(&mut lifeline).and_then(|status| status.code()),
        Some(0)
    );
    let mut said = Vec::new();
    output.read_to_end(&mut said).unwrap();
    assert!(said.is_empty(), "{said:?}");
    assert_eq!(child.wait().unwrap().code(), Some(i32::from(DONE)));
}

#[test]
fn a_handed_program_never_starts_without_its_packet_or_with_one_out_of_place() {
    let folder = tempfile::tempdir().unwrap();
    let mark = folder.path().join("ran");
    let touch = ["/usr/bin/touch", mark.to_str().unwrap()];
    let too_long = {
        let mut frame = vec![PACKET];
        frame.extend(u32::try_from(HANDED + 1).unwrap().to_be_bytes());
        frame
    };
    let cases: [(bool, Vec<u8>); 5] = [
        // Handed, its word to start with no packet before it.
        (true, vec![RENEW]),
        // Handed, its packet past the bound.
        (true, too_long),
        // Handed twice.
        (true, [packet_frame(b"{}"), packet_frame(b"{}")].concat()),
        // Not handed, yet sent one.
        (false, packet_frame(b"{}")),
        // Its packet cut short.
        (true, packet_frame(b"{}")[..4].to_vec()),
    ];

    for (handed, said) in cases {
        let (mut child, mut lifeline, _output) = heard_supervisor(&touch, handed);
        lifeline.write_all(&said).unwrap();
        let _ = lifeline.shutdown(std::net::Shutdown::Write);

        let status = child.wait().unwrap();

        assert_eq!(status.code(), Some(i32::from(ENDED)), "{said:?}");
        assert!(!mark.exists(), "the program ran: {said:?}");
    }
}

#[test]
fn a_packet_after_the_word_to_start_ends_the_work() {
    let (mut child, mut lifeline, _output) = heard_supervisor(
        &["/bin/sh", "-c", "/bin/cat >/dev/null; /bin/sleep 30"],
        true,
    );
    lifeline.write_all(&packet_frame(b"{}")).unwrap();
    lifeline.write_all(&[RENEW]).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    lifeline.write_all(&packet_frame(b"{}")).unwrap();

    assert!(ends_within(&mut child, Duration::from_secs(2)));
}
