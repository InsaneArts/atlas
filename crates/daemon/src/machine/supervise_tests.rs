use super::*;
use atlas_session::clock::Manual;
use std::{
    io::Write,
    process::{Child, Stdio},
    sync::mpsc,
    time::Instant,
};

/// Set in the child the tests below start: what it supervises.
const SUPERVISING: &str = "ATLAS_TEST_SUPERVISING";

/// What separates a supervised program's words in [`SUPERVISING`].
const SEPARATE: char = '\u{1f}';

/// The supervisor these tests run in a child of their own: the test binary
/// again, in a group of its own, supervising what `SUPERVISING` names.
#[test]
fn a_supervisor_runs_what_it_is_given_when_started_for_these_tests() {
    let Ok(program) = std::env::var(SUPERVISING) else {
        return;
    };
    let mut words = program.split(SEPARATE).map(str::to_owned);
    let program = words.next().unwrap();
    let arguments: Vec<String> = words.collect();
    let status = supervise(
        &program,
        &arguments,
        std::io::stdin(),
        Duration::from_millis(800),
    );
    std::process::exit(status.ok().and_then(|status| status.code()).unwrap_or(70));
}

/// A supervisor of the program `words` name, its lifeline held here.
fn supervisor(words: &[&str]) -> Child {
    let program = words.join(&SEPARATE.to_string());
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "machine::supervise::tests::a_supervisor_runs_what_it_is_given_when_started_for_these_tests",
            "--test-threads=1",
        ])
        .env(SUPERVISING, program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .process_group(0);
    atlas_core::spawn::spawn(&mut command).unwrap()
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
    let mut child = supervisor(&["/bin/sleep", "30"]);
    std::thread::sleep(Duration::from_millis(300));

    drop(child.stdin.take());

    assert!(ends_within(&mut child, Duration::from_millis(500)));
}

#[test]
fn a_lease_gone_unrenewed_ends_the_group() {
    let mut child = supervisor(&["/bin/sleep", "30"]);

    // The lifeline stays open; nothing renews the lease.
    assert!(ends_within(&mut child, Duration::from_secs(5)));
    drop(child.stdin.take());
}

#[test]
fn renewals_keep_the_work_running_until_they_stop() {
    let mut child = supervisor(&["/bin/sleep", "30"]);
    let mut lifeline = child.stdin.take().unwrap();
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
fn the_work_ending_by_itself_ends_its_supervisor_with_its_status() {
    let mut child = supervisor(&["/usr/bin/false"]);
    let mut lifeline = child.stdin.take().unwrap();
    let _ = lifeline.write_all(&[RENEW]);

    let status = child.wait().unwrap();

    assert_eq!(status.code(), Some(1));
}

#[test]
fn work_left_behind_by_a_program_that_ended_is_supervised_until_it_ends_too() {
    // Its background process outlives the shell, in the shell's group.
    let mut child = supervisor(&["/bin/sh", "-c", "/bin/sleep 30 &"]);
    std::thread::sleep(Duration::from_millis(500));
    let mut lifeline = child.stdin.take().unwrap();
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
    let mut child = supervisor(&[
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

    drop(child.stdin.take());

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
    let mut child = supervisor(&[
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

    drop(child.stdin.take());

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
}

impl Kernel for MadeUp {
    fn group(&self, _pgid: i32) -> atlas_core::error::Result<Vec<Info>> {
        Ok(self.group.clone())
    }

    fn children(&self, _pid: i32) -> atlas_core::error::Result<Vec<i32>> {
        let held = self.held.lock().unwrap().take();
        if let Some(held) = held {
            let _ = held.recv();
        }
        if self.refusing {
            return Err(std::io::Error::other("refused").into());
        }
        Ok(Vec::new())
    }

    fn info(&self, _pid: i32) -> atlas_core::error::Result<Option<Info>> {
        if self.refusing {
            return Err(std::io::Error::other("refused").into());
        }
        Ok(None)
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

    assert_eq!(work.targets(), vec![Pid::from_raw(99_001)]);
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
    let targets = work.targets();

    assert!(started.elapsed() < Duration::from_millis(50));
    assert_eq!(targets, vec![Pid::from_raw(99_002)]);
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
    assert_eq!(work.targets(), vec![Pid::from_raw(99_003)]);
}
