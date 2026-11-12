use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::atomic::AtomicU32,
};

const HELPER: u32 = 503;
const PERSON: u32 = 501;
/// The helper's login session, and another login's.
const LOGIN: i32 = 108_870;
const ELSEWHERE: i32 = 7;
const PROXY: &str = "/usr/libexec/xpcproxy";
const OTHER: &str = "/System/Library/PrivateFrameworks/HearingCore.framework/heard";

/// One made-up process.
struct Process {
    info: Info,
    program: PathBuf,
    /// Which use of its PID this is.
    version: u32,
    session: i32,
    born: Duration,
}

impl Process {
    fn token(&self) -> Token {
        Token::made_up(
            self.info.instance.pid,
            self.version,
            self.info.uid,
            self.session,
        )
    }
}

/// A made-up Mac: its clock, its processes, and what was asked of it.
struct Fake {
    hidden: Cell<bool>,
    /// How many more times the helper reads as hidden before it comes on
    /// screen; none while `hidden` alone says.
    hidden_for: Cell<Option<u32>>,
    began: Instant,
    elapsed: Cell<Duration>,
    processes: RefCell<BTreeMap<i32, Process>>,
    uses: Cell<u32>,
    /// The signals that reached a process.
    signals: RefCell<Vec<(i32, Signal)>>,
    /// The signals the kernel refused: their token's process was gone.
    refused: Cell<u32>,
    watched: RefCell<BTreeSet<i32>>,
    /// Whether a signalled process does as it is told.
    obeys: Cell<bool>,
    /// Whether the kernel tells launchd's starts.
    tells_starts: Cell<bool>,
    /// Whether the kernel describes processes just now.
    readable: Cell<bool>,
    /// Whether the kernel lists processes just now.
    lists: Cell<bool>,
    /// How long asking where the helper is takes.
    hidden_takes: Cell<Duration>,
    /// How long sending a signal takes.
    signal_takes: Cell<Duration>,
    /// Whether the next stop, once sent, is followed by a panic.
    panics_after_stop: Cell<bool>,
    /// Whether the next process to be forgotten brings a panic instead.
    panics_forgetting: Cell<bool>,
    /// What the coming waits bring, none for one the kernel refuses; the
    /// wait after the last tells the keeper to stop.
    waits: RefCell<VecDeque<Option<Vec<Event>>>>,
    stop: AtomicBool,
}

impl Fake {
    fn new(hidden: bool) -> Self {
        Self {
            hidden: Cell::new(hidden),
            hidden_for: Cell::new(None),
            began: Instant::now(),
            elapsed: Cell::new(Duration::ZERO),
            processes: RefCell::default(),
            uses: Cell::new(0),
            signals: RefCell::default(),
            refused: Cell::new(0),
            watched: RefCell::default(),
            obeys: Cell::new(true),
            tells_starts: Cell::new(true),
            readable: Cell::new(true),
            lists: Cell::new(true),
            hidden_takes: Cell::new(Duration::ZERO),
            signal_takes: Cell::new(Duration::ZERO),
            panics_after_stop: Cell::new(false),
            panics_forgetting: Cell::new(false),
            waits: RefCell::default(),
            stop: AtomicBool::new(false),
        }
    }

    /// launchd starts `pid` now: root's, and its own go-between still.
    fn start(&self, pid: i32) -> Instance {
        let elapsed = self.elapsed.get();
        let instance = Instance::new(
            pid,
            [
                1_794_300_000 + elapsed.as_secs(),
                u64::from(elapsed.subsec_micros()),
            ],
        );
        self.uses.set(self.uses.get() + 1);
        let process = Process {
            info: Info {
                instance,
                ppid: LAUNCHD,
                pgid: pid,
                uid: 0,
                ruid: 0,
                svuid: 0,
                zombie: false,
                stopped: false,
                name: "xpcproxy".into(),
            },
            program: PathBuf::from(PROXY),
            version: self.uses.get(),
            session: LOGIN,
            born: elapsed,
        };
        self.processes.borrow_mut().insert(pid, process);
        instance
    }

    /// `pid` becomes `program`, as the account `uid`.
    fn become_program(&self, pid: i32, program: &str, uid: u32) {
        let mut processes = self.processes.borrow_mut();
        let process = processes.get_mut(&pid).unwrap();
        (process.info.uid, process.info.ruid, process.info.svuid) = (uid, uid, uid);
        process.program = PathBuf::from(program);
    }

    /// launchd starts the helper's agent as `pid`, and it has become it.
    fn start_agent(&self, pid: i32) -> Instance {
        let instance = self.start(pid);
        self.become_program(pid, PROGRAM, HELPER);
        instance
    }

    fn change(&self, pid: i32, change: impl FnOnce(&mut Process)) {
        change(self.processes.borrow_mut().get_mut(&pid).unwrap());
    }

    fn end(&self, pid: i32) {
        self.processes.borrow_mut().remove(&pid);
        self.watched.borrow_mut().remove(&pid);
    }

    fn pass(&self, time: Duration) {
        self.elapsed.set(self.elapsed.get() + time);
    }

    fn stopped(&self, pid: i32) -> bool {
        self.processes.borrow()[&pid].info.stopped
    }

    fn signals(&self) -> Vec<(i32, Signal)> {
        self.signals.borrow().clone()
    }

    fn watched(&self) -> Vec<i32> {
        self.watched.borrow().iter().copied().collect()
    }
}

impl System for &Fake {
    fn hidden(&self) -> bool {
        self.pass(self.hidden_takes.get());
        match self.hidden_for.get() {
            None => self.hidden.get(),
            Some(0) => false,
            Some(more) => {
                self.hidden_for.set(Some(more - 1));
                true
            }
        }
    }

    fn now(&self) -> Instant {
        self.began + self.elapsed.get()
    }

    fn children(&self, parent: i32) -> Option<Vec<i32>> {
        self.lists.get().then(|| {
            self.processes
                .borrow()
                .values()
                .filter(|process| process.info.ppid == parent)
                .map(|process| process.info.instance.pid)
                .collect()
        })
    }

    fn processes(&self, uid: u32) -> Option<Vec<Info>> {
        self.lists.get().then(|| {
            self.processes
                .borrow()
                .values()
                .filter(|process| process.info.uid == uid)
                .map(|process| process.info.clone())
                .collect()
        })
    }

    fn process(&self, pid: i32) -> Seen {
        if !self.readable.get() {
            return Seen::Unknown;
        }
        self.processes
            .borrow()
            .get(&pid)
            .map_or(Seen::Gone, |process| Seen::Found(process.info.clone()))
    }

    fn exact(&self, pid: i32) -> Option<Exact> {
        if !self.readable.get() {
            return None;
        }
        let processes = self.processes.borrow();
        let process = processes.get(&pid)?;
        Some(Exact {
            token: process.token(),
            program: process.program.clone(),
            age: self.elapsed.get().saturating_sub(process.born),
            read: self.now(),
        })
    }

    fn signal(&self, token: &Token, signal: Signal) -> bool {
        let mut processes = self.processes.borrow_mut();
        let Some(process) = processes
            .get_mut(&token.pid())
            .filter(|process| process.token() == *token)
        else {
            self.refused.set(self.refused.get() + 1);
            return false;
        };
        self.signals.borrow_mut().push((token.pid(), signal));
        if self.obeys.get() {
            process.info.stopped = signal == Signal::SIGSTOP;
        }
        self.elapsed
            .set(self.elapsed.get() + self.signal_takes.get());
        if signal == Signal::SIGSTOP && self.panics_after_stop.replace(false) {
            drop(processes);
            resume_unwind(Box::new("after the stop"));
        }
        true
    }

    fn settle(&self) {}

    fn watch_starts(&self, parent: i32) -> bool {
        self.tells_starts.get() && {
            self.watched.borrow_mut().insert(parent);
            true
        }
    }

    fn watch(&self, pid: i32) -> bool {
        self.processes.borrow().contains_key(&pid) && {
            self.watched.borrow_mut().insert(pid);
            true
        }
    }

    fn forget(&self, pid: i32) {
        if self.panics_forgetting.replace(false) {
            resume_unwind(Box::new("forgetting"));
        }
        self.watched.borrow_mut().remove(&pid);
    }

    fn wait(&self, longest: Duration) -> Option<Vec<Event>> {
        match self.waits.borrow_mut().pop_front() {
            // A notice ends the wait at once.
            Some(Some(events)) => Some(events),
            Some(None) => {
                self.pass(longest);
                None
            }
            None => {
                self.pass(longest);
                self.stop.store(true, Ordering::SeqCst);
                Some(Vec::new())
            }
        }
    }
}

fn keeper(fake: &Fake) -> Keeper<&Fake> {
    let mut keeper = Keeper::new(fake, HELPER, LOGIN, Arc::new(Mutex::new(Report::OFF)));
    keeper.look();
    keeper
}

fn report(keeper: &Keeper<&Fake>) -> Report {
    *keeper.report.lock().unwrap()
}

fn starts(keeper: &Keeper<&Fake>) -> Vec<Instance> {
    match &keeper.phase {
        Phase::Watching(starting) => starting.iter().map(|start| start.process).collect(),
        other => panic!("not watching: {other:?}"),
    }
}

fn kept(keeper: &Keeper<&Fake>) -> Option<Instance> {
    match &keeper.phase {
        Phase::Paused(kept) => Some(kept.process),
        _ => None,
    }
}

/// A keeper in the hidden helper, with the agent started as `pid`, told,
/// and paused.
fn paused(fake: &Fake, pid: i32) -> (Keeper<&Fake>, Instance) {
    let mut keeper = keeper(fake);
    let agent = fake.start(pid);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(pid, PROGRAM, HELPER);
    keeper.on(Event::Became(pid));
    assert_eq!(kept(&keeper), Some(agent));
    (keeper, agent)
}

#[test]
fn nothing_is_watched_or_paused_while_the_helper_is_on_screen() {
    let fake = Fake::new(false);
    let mut keeper = keeper(&fake);
    fake.start_agent(40);
    keeper.on(Event::Spawned(LAUNCHD));
    keeper.on(Event::Became(40));
    keeper.look();

    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(fake.signals(), []);
    assert_eq!(fake.watched(), Vec::<i32>::new());
    assert_eq!(report(&keeper).state, State::OnScreen);
}

#[test]
fn a_hidden_helper_waits_for_launchds_next_start() {
    let fake = Fake::new(true);
    let keeper = keeper(&fake);

    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
    assert_eq!(report(&keeper).state, State::Watching);
}

#[test]
fn the_agent_is_paused_the_moment_it_starts_and_nothing_else_is_watched() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let agent = fake.start(40);
    let other = fake.start(41);
    keeper.on(Event::Spawned(LAUNCHD));
    assert_eq!(starts(&keeper), [agent, other]);
    assert_eq!(fake.watched(), [LAUNCHD, 40, 41]);
    assert_eq!(fake.signals(), []);

    fake.pass(Duration::from_millis(9));
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert!(fake.stopped(40));
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(fake.watched(), [40]);
    assert_eq!(
        report(&keeper),
        Report {
            state: State::Paused,
            pauses: 1,
            misses: 0,
            ..Report::OFF
        }
    );
}

#[test]
fn a_start_that_was_the_agent_before_it_could_be_watched_is_paused_too() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let agent = fake.start_agent(40);
    fake.pass(Duration::from_millis(12));
    keeper.on(Event::Spawned(LAUNCHD));

    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(kept(&keeper), Some(agent));
}

#[test]
fn a_start_that_becomes_its_go_between_first_is_paused_once_it_is_the_agent() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let agent = fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    // Told as launchd's start becomes its go-between, before the agent.
    keeper.on(Event::Became(40));
    assert_eq!(fake.signals(), []);
    assert_eq!(starts(&keeper), [agent]);

    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(kept(&keeper), Some(agent));
}

#[test]
fn a_start_seen_too_late_is_left_to_quit_and_the_next_is_awaited() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.pass(YOUNG + Duration::from_millis(1));
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
    assert_eq!(report(&keeper).misses, 1);

    fake.end(40);
    let next = fake.start(41);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(41, PROGRAM, HELPER);
    keeper.on(Event::Became(41));
    assert_eq!(fake.signals(), [(41, Signal::SIGSTOP)]);
    assert_eq!(kept(&keeper), Some(next));
}

#[test]
fn another_program_launchd_starts_is_never_signalled() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let other = fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, OTHER, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(starts(&keeper), [other]);
    assert_eq!(report(&keeper).misses, 0);

    fake.end(40);
    keeper.on(Event::Ended(40));
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
}

#[test]
fn the_persons_own_agent_is_never_signalled() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, PERSON);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(kept(&keeper), None);
}

#[test]
fn the_agent_of_another_login_is_never_signalled() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    fake.change(40, |process| process.session = ELSEWHERE);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(kept(&keeper), None);
}

#[test]
fn the_agent_started_by_anything_but_launchd_is_never_signalled() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);

    // One a shell started is no child of launchd, so it is never among
    // the starts looked at; were it told of, it is still not launchd's.
    fake.start_agent(41);
    fake.change(41, |process| process.info.ppid = 900);
    keeper.on(Event::Spawned(LAUNCHD));
    keeper.on(Event::Became(41));
    assert_eq!(fake.signals(), []);
    assert_eq!(keeper.identify(41, PROGRAM), Is::Other);
}

#[test]
fn a_helper_that_came_on_screen_before_the_notice_has_nothing_paused() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));

    // No look in between: the keeper last saw the helper hidden.
    fake.hidden.set(false);
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(fake.watched(), Vec::<i32>::new());
    assert_eq!(report(&keeper).misses, 0);
}

#[test]
fn a_stop_that_shows_as_the_helper_comes_on_screen_is_taken_back() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));

    // Hidden when asked before the stop, on screen when asked after.
    fake.hidden_for.set(Some(1));
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
    assert_eq!(kept(&keeper), None);
    assert_eq!(keeper.owed, []);
    assert_eq!(
        report(&keeper),
        Report {
            state: State::Watching,
            pauses: 0,
            misses: 0,
            ..Report::OFF
        }
    );

    keeper.look();
    assert_eq!(keeper.phase, Phase::OnScreen);
}

#[test]
fn the_agent_runs_again_when_the_helper_comes_on_screen_and_is_awaited_once_hidden() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    fake.hidden.set(false);
    keeper.look();
    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(fake.watched(), Vec::<i32>::new());

    fake.hidden.set(true);
    keeper.look();
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
}

#[test]
fn a_paused_agent_that_ended_is_awaited_again() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    fake.end(40);
    keeper.on(Event::Ended(40));
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);

    let next = fake.start(41);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(41, PROGRAM, HELPER);
    keeper.on(Event::Became(41));
    assert_eq!(kept(&keeper), Some(next));
    assert_eq!(report(&keeper).pauses, 2);
}

#[test]
fn an_agent_someone_else_continued_is_left_to_quit_and_awaited_again() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    fake.change(40, |process| process.info.stopped = false);
    keeper.look();
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
}

#[test]
fn a_paused_agent_the_kernel_does_not_describe_just_now_stays_kept() {
    let fake = Fake::new(true);
    let (mut keeper, agent) = paused(&fake, 40);

    fake.readable.set(false);
    keeper.look();
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(fake.watched(), [40]);

    fake.readable.set(true);
    keeper.look();
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
}

#[test]
fn an_agent_left_paused_by_an_earlier_daemon_is_kept_while_hidden() {
    let fake = Fake::new(true);
    let agent = fake.start_agent(40);
    fake.change(40, |process| process.info.stopped = true);
    fake.pass(Duration::from_secs(3600));

    let keeper = keeper(&fake);
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(fake.signals(), []);
    assert_eq!(fake.watched(), [40]);
    assert_eq!(
        report(&keeper),
        Report {
            state: State::Paused,
            pauses: 0,
            misses: 0,
            ..Report::OFF
        }
    );
}

#[test]
fn an_agent_left_paused_by_an_earlier_daemon_runs_again_on_screen() {
    let fake = Fake::new(false);
    fake.start_agent(40);
    fake.change(40, |process| process.info.stopped = true);

    let keeper = keeper(&fake);
    assert_eq!(fake.signals(), [(40, Signal::SIGCONT)]);
    assert!(!fake.stopped(40));
    assert_eq!(keeper.phase, Phase::OnScreen);
}

#[test]
fn only_a_paused_agent_of_this_login_is_taken_over() {
    let fake = Fake::new(true);
    // The person's own, another paused program, one in another login, and
    // the helper's agent running as it should.
    fake.start_agent(40);
    fake.become_program(40, PROGRAM, PERSON);
    fake.start(41);
    fake.become_program(41, OTHER, HELPER);
    fake.start_agent(42);
    fake.change(42, |process| process.session = ELSEWHERE);
    for pid in [40, 41, 42] {
        fake.change(pid, |process| process.info.stopped = true);
    }
    fake.start_agent(43);

    let keeper = keeper(&fake);
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.signals(), []);
}

#[test]
fn the_agent_runs_again_when_the_keeper_goes() {
    let fake = Fake::new(true);
    let (keeper, _) = paused(&fake, 40);
    let told = keeper.report.clone();

    drop(keeper);
    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
    assert_eq!(fake.watched(), Vec::<i32>::new());
    assert_eq!(told.lock().unwrap().state, State::Off);
}

#[test]
fn a_keeper_that_ends_says_whether_the_agent_was_seen_to_run_again() {
    let fake = Fake::new(true);
    let (keeper, _) = paused(&fake, 40);
    assert!(keeper.end());
    assert!(!fake.stopped(40));

    let (keeper, _) = paused(&fake, 41);
    fake.obeys.set(false);
    assert!(!keeper.end());
    assert!(fake.stopped(41));
    // Tried at its end, the allowed times over, and once more as it went.
    let tries = fake
        .signals()
        .iter()
        .filter(|sent| **sent == (41, Signal::SIGCONT))
        .count();
    assert_eq!(tries, 2 + LAST_TRIES as usize);
}

#[test]
fn another_process_with_the_agents_pid_is_never_reached() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    // The agent is gone and its PID is another paused process's.
    fake.end(40);
    fake.pass(Duration::from_secs(5));
    fake.start(40);
    fake.become_program(40, OTHER, HELPER);
    fake.change(40, |process| process.info.stopped = true);

    fake.hidden.set(false);
    keeper.look();
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(fake.refused.get(), 1);
    assert!(fake.stopped(40));
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(keeper.owed, []);
}

#[test]
fn a_stop_that_does_not_show_is_taken_back_and_counted_as_missed() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.obeys.set(false);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, HELPER);
    keeper.on(Event::Became(40));

    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert_eq!(starts(&keeper), []);
    assert_eq!(keeper.owed, []);
    assert_eq!(
        report(&keeper),
        Report {
            state: State::Watching,
            pauses: 0,
            misses: 1,
            ..Report::OFF
        }
    );
}

#[test]
fn an_agent_that_will_not_continue_stays_owed_and_is_continued_at_each_look() {
    let fake = Fake::new(true);
    let (mut keeper, agent) = paused(&fake, 40);

    fake.obeys.set(false);
    fake.hidden.set(false);
    keeper.look();
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(
        keeper
            .owed
            .iter()
            .map(|kept| kept.process)
            .collect::<Vec<_>>(),
        [agent]
    );
    assert!(fake.stopped(40));

    fake.obeys.set(true);
    keeper.look();
    assert!(!fake.stopped(40));
    assert_eq!(keeper.owed, []);
}

#[test]
fn a_continue_the_kernel_does_not_show_stays_owed_until_it_is_seen() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    fake.readable.set(false);
    fake.hidden.set(false);
    keeper.look();
    assert_eq!(keeper.owed.len(), 1);

    fake.readable.set(true);
    keeper.look();
    assert_eq!(keeper.owed, []);
    assert!(!fake.stopped(40));
}

#[test]
fn a_start_the_kernel_does_not_describe_is_not_paused() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let agent = fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, HELPER);
    fake.readable.set(false);
    keeper.on(Event::Became(40));
    keeper.look();

    assert_eq!(fake.signals(), []);
    assert_eq!(starts(&keeper), [agent]);
    assert_eq!(fake.watched(), [LAUNCHD, 40]);

    // Still watched, so it is paused once the kernel describes it again.
    fake.readable.set(true);
    keeper.on(Event::Became(40));
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(kept(&keeper), Some(agent));
}

#[test]
fn starts_that_never_become_the_agent_are_watched_no_longer() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    let slow = fake.start(40);
    let gone = fake.start(41);
    keeper.on(Event::Spawned(LAUNCHD));
    assert_eq!(starts(&keeper), [slow, gone]);

    fake.end(41);
    keeper.look();
    assert_eq!(starts(&keeper), [slow]);

    fake.pass(BECOMING + Duration::from_millis(1));
    keeper.look();
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
}

#[test]
fn starts_the_kernel_will_not_tell_are_asked_for_again_at_each_look() {
    let fake = Fake::new(true);
    fake.tells_starts.set(false);
    let mut keeper = keeper(&fake);
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(report(&keeper).state, State::Unavailable);

    fake.tells_starts.set(true);
    keeper.look();
    assert_eq!(starts(&keeper), []);
    assert_eq!(report(&keeper).state, State::Watching);
}

#[test]
fn a_helper_on_screen_is_never_said_to_be_unavailable() {
    let fake = Fake::new(true);
    fake.tells_starts.set(false);
    let mut keeper = keeper(&fake);
    assert_eq!(report(&keeper).state, State::Unavailable);

    fake.hidden.set(false);
    keeper.look();
    assert_eq!(report(&keeper).state, State::OnScreen);
}

#[test]
fn processes_launchd_already_had_are_not_taken_for_new_starts() {
    let fake = Fake::new(true);
    // The agent in the middle of a life when the keeper begins.
    fake.start_agent(40);
    let mut keeper = keeper(&fake);
    fake.start(41);
    keeper.on(Event::Spawned(LAUNCHD));

    assert_eq!(fake.signals(), []);
    assert_eq!(fake.watched(), [LAUNCHD, 41]);
}

#[test]
fn a_start_that_grew_old_while_the_helpers_place_was_asked_is_not_paused() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.pass(Duration::from_millis(9));
    fake.become_program(40, PROGRAM, HELPER);

    // Young when its age was read, too old once the helper's place is known.
    fake.hidden_takes.set(YOUNG);
    keeper.on(Event::Became(40));

    assert_eq!(fake.signals(), []);
    assert_eq!(starts(&keeper), []);
    assert_eq!(report(&keeper).misses, 1);
}

#[test]
fn a_stop_that_was_sent_too_late_is_taken_back() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.pass(Duration::from_millis(9));
    fake.become_program(40, PROGRAM, HELPER);

    // Young as the stop is sent, too old by the time it has been.
    fake.signal_takes.set(YOUNG);
    keeper.on(Event::Became(40));

    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
    assert_eq!(kept(&keeper), None);
    assert_eq!(keeper.owed, []);
    assert_eq!(report(&keeper).misses, 1);
}

#[test]
fn an_agent_left_paused_that_could_not_be_read_at_first_is_taken_over_later() {
    let fake = Fake::new(true);
    let agent = fake.start_agent(40);
    fake.change(40, |process| process.info.stopped = true);

    // Listed, but not described: it may be the agent, so it is asked again.
    fake.readable.set(false);
    let mut keeper = keeper(&fake);
    assert_eq!(kept(&keeper), None);
    assert!(!keeper.looked_for_one_left);

    fake.readable.set(true);
    keeper.look();
    assert_eq!(kept(&keeper), Some(agent));
    assert!(keeper.looked_for_one_left);
    assert_eq!(fake.signals(), []);
}

#[test]
fn processes_the_kernel_does_not_list_are_asked_for_again() {
    let fake = Fake::new(false);
    fake.start_agent(40);
    fake.change(40, |process| process.info.stopped = true);

    fake.lists.set(false);
    let mut keeper = keeper(&fake);
    assert_eq!(fake.signals(), []);
    assert!(fake.stopped(40));

    // On screen, the one an earlier daemon left paused runs again.
    fake.lists.set(true);
    keeper.look();
    assert_eq!(fake.signals(), [(40, Signal::SIGCONT)]);
    assert!(!fake.stopped(40));
}

#[test]
fn starts_the_kernel_does_not_list_are_left_to_the_next_notice() {
    let fake = Fake::new(true);
    fake.lists.set(false);
    let mut keeper = keeper(&fake);
    // launchd's children are unknown, so a new one could not be told.
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(report(&keeper).state, State::Unavailable);
    assert_eq!(fake.watched(), Vec::<i32>::new());

    fake.lists.set(true);
    keeper.look();
    assert_eq!(starts(&keeper), []);

    fake.lists.set(false);
    let agent = fake.start_agent(40);
    keeper.on(Event::Spawned(LAUNCHD));
    assert_eq!(fake.signals(), []);

    fake.lists.set(true);
    keeper.on(Event::Spawned(LAUNCHD));
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(kept(&keeper), Some(agent));
}

#[test]
fn a_panic_right_after_the_stop_still_lets_the_agent_run_again() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, HELPER);

    fake.panics_after_stop.set(true);
    let unwound = catch_unwind(AssertUnwindSafe(|| keeper.on(Event::Became(40))));
    assert!(unwound.is_err());
    assert!(fake.stopped(40));
    assert_eq!(kept(&keeper), None);

    drop(keeper);
    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
}

#[test]
fn a_panic_once_the_stop_has_shown_still_lets_the_agent_run_again() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);
    fake.start(40);
    keeper.on(Event::Spawned(LAUNCHD));
    fake.become_program(40, PROGRAM, HELPER);

    // The stop is sent and shows; keeping it then panics.
    fake.panics_forgetting.set(true);
    let unwound = catch_unwind(AssertUnwindSafe(|| keeper.on(Event::Became(40))));
    assert!(unwound.is_err());
    assert!(fake.stopped(40));

    drop(keeper);
    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
    assert!(!fake.stopped(40));
}

#[test]
fn a_panic_while_letting_the_agent_go_still_lets_it_run_again() {
    let fake = Fake::new(true);
    let (mut keeper, _) = paused(&fake, 40);

    fake.hidden.set(false);
    fake.panics_forgetting.set(true);
    let unwound = catch_unwind(AssertUnwindSafe(|| keeper.look()));
    assert!(unwound.is_err());
    assert!(fake.stopped(40));

    drop(keeper);
    assert!(!fake.stopped(40));
    assert_eq!(
        fake.signals(),
        [(40, Signal::SIGSTOP), (40, Signal::SIGCONT)]
    );
}

#[test]
fn notices_the_kernel_will_not_tell_are_asked_for_again_at_the_next_look() {
    let fake = Fake::new(true);
    let mut keeper = keeper(&fake);

    keeper.untold();
    assert_eq!(keeper.phase, Phase::OnScreen);
    assert_eq!(fake.watched(), Vec::<i32>::new());
    assert_eq!(report(&keeper).state, State::Unavailable);

    keeper.look();
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
    assert_eq!(report(&keeper).state, State::Watching);
}

#[test]
fn a_paused_agent_stays_kept_while_the_kernel_will_not_tell_its_notices() {
    let fake = Fake::new(true);
    let (mut keeper, agent) = paused(&fake, 40);

    keeper.untold();
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(report(&keeper).state, State::Paused);
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
}

#[test]
fn a_running_keeper_takes_notices_as_they_come_and_looks_each_second() {
    let fake = Fake::new(true);
    let mut keeper = Keeper::new(&fake, HELPER, LOGIN, Arc::new(Mutex::new(Report::OFF)));

    // A wait the kernel refuses, then the stop: by the look a second on,
    // launchd's starts are awaited again.
    fake.waits.borrow_mut().push_back(None);
    keeper.run(&fake.stop);
    assert_eq!(starts(&keeper), []);
    assert_eq!(fake.watched(), [LAUNCHD]);
    assert_eq!(report(&keeper).state, State::Watching);

    // A start told in a notice is paused.
    fake.stop.store(false, Ordering::SeqCst);
    let agent = fake.start_agent(40);
    fake.waits
        .borrow_mut()
        .push_back(Some(vec![Event::Spawned(LAUNCHD)]));
    keeper.run(&fake.stop);
    assert_eq!(kept(&keeper), Some(agent));
    assert_eq!(fake.signals(), [(40, Signal::SIGSTOP)]);
    assert_eq!(report(&keeper).state, State::Paused);
}

#[test]
fn a_handle_that_keeps_no_agent_says_so_and_does_nothing() {
    let agent = ConsoleAgent::detached();
    drop(agent.start());
    assert_eq!(agent.report(), Report::OFF);
    assert_eq!(
        agent.report().json(),
        json!({"state": "off", "pauses": 0, "misses": 0, "rapportd": {"paused": false, "pauses": 0}})
    );
    agent.stop();
}

/// A made-up keeper's thread and what its test is told of it.
struct Idle {
    /// How many keepers were started.
    starts: Arc<AtomicU32>,
    /// Told each time a keeper's thread runs.
    began: Receiver<()>,
    /// Lets a keeper that waits for it go on to hear its stop.
    go: mpsc::Sender<()>,
}

/// A handle whose keeper touches nothing: it says it is on screen, waits
/// to be told to stop, says it is off and answers `released`. With
/// `held`, it first waits for `Idle::go`, deaf to any stop.
fn idle(released: bool, held: bool, patience: Duration) -> (ConsoleAgent, Idle) {
    let starts = Arc::new(AtomicU32::new(0));
    let (beginning, began) = mpsc::channel();
    let (go, going) = mpsc::channel();
    let going = Mutex::new(Some(going));
    let counted = starts.clone();
    let starter: Starter = Box::new(move |report| {
        counted.fetch_add(1, Ordering::SeqCst);
        let (wake, woken) = mpsc::channel();
        let beginning = beginning.clone();
        let going = going.lock().unwrap().take();
        Running::spawn(
            // The thread has ended when nobody hears the wake.
            move || _ = wake.send(()),
            move |stopping| {
                report.lock().unwrap().state = State::OnScreen;
                beginning.send(()).unwrap();
                if held {
                    going.unwrap().recv().unwrap();
                }
                while !stopping.load(Ordering::SeqCst) {
                    woken.recv().unwrap();
                }
                report.lock().unwrap().state = State::Off;
                released
            },
        )
        .ok()
    });
    (
        ConsoleAgent::with(starter, patience),
        Idle { starts, began, go },
    )
}

const SOON: Duration = Duration::from_secs(5);

#[test]
fn a_started_keeper_ends_when_what_started_it_is_left() {
    let (agent, idle) = idle(true, false, LET_GO);
    {
        let _keeping = agent.start();
        idle.began.recv_timeout(SOON).unwrap();
        assert_eq!(agent.report().state, State::OnScreen);

        // A second start is the same keeper.
        std::mem::forget(agent.start());
        assert_eq!(idle.starts.load(Ordering::SeqCst), 1);
    }
    assert_eq!(agent.report().state, State::Off);
    assert!(agent.keeper.lock().unwrap().is_none());
    agent.stop();
}

#[test]
fn a_handle_that_is_dropped_ends_its_keeper() {
    let (agent, idle) = idle(true, false, LET_GO);
    let told = agent.report.clone();
    std::mem::forget(agent.start());
    idle.began.recv_timeout(SOON).unwrap();

    drop(agent);
    assert_eq!(told.lock().unwrap().state, State::Off);
}

#[test]
fn a_keeper_that_does_not_start_is_said_to_be_unavailable() {
    let agent = ConsoleAgent::with(Box::new(|_| None), LET_GO);
    drop(agent.start());

    assert_eq!(agent.report().state, State::Unavailable);
    assert!(agent.keeper.lock().unwrap().is_none());
}

#[test]
fn a_keeper_that_has_not_ended_in_time_is_waited_for_again() {
    let (agent, idle) = idle(true, true, Duration::from_millis(10));
    std::mem::forget(agent.start());
    idle.began.recv_timeout(SOON).unwrap();

    // Deaf to its stop for now: the stop gives up waiting, not the keeper.
    agent.stop();
    assert!(agent.keeper.lock().unwrap().is_some());
    assert_eq!(agent.report().state, State::OnScreen);
    std::mem::forget(agent.start());
    assert_eq!(idle.starts.load(Ordering::SeqCst), 1);

    idle.go.send(()).unwrap();
    let end = Instant::now() + SOON;
    while agent.keeper.lock().unwrap().is_some() {
        assert!(Instant::now() < end, "the keeper did not end");
        agent.stop();
    }
    assert_eq!(agent.report().state, State::Off);
}

thread_local! {
    /// Held by a made-up keeper's thread until the thread itself ends.
    static LAST: RefCell<Option<Last>> = const { RefCell::new(None) };
}

/// What a thread does last of all, after its keeper has answered: it says
/// so, then waits to be let through.
struct Last {
    leaving: mpsc::Sender<()>,
    through: Receiver<()>,
}

impl Drop for Last {
    fn drop(&mut self) {
        // A test that has given up hears and says nothing more.
        let _ = self.leaving.send(());
        let _ = self.through.recv();
    }
}

/// A stop of a keeper whose thread, having answered `answer` (or panicked,
/// with none), is still on its way out: whether the stop waited for it.
fn stop_waits_for_the_thread(answer: Option<bool>) -> bool {
    let (leaving, left) = mpsc::channel();
    let (let_through, through) = mpsc::channel();
    let last = Mutex::new(Some(Last { leaving, through }));
    let starter: Starter = Box::new(move |_| {
        let last = last.lock().unwrap().take();
        Running::spawn(
            || {},
            move |_| {
                LAST.set(last);
                answer.unwrap_or_else(|| resume_unwind(Box::new("no answer")))
            },
        )
        .ok()
    });
    let agent = Arc::new(ConsoleAgent::with(starter, SOON));
    std::mem::forget(agent.start());

    let stopping = agent.clone();
    let (stopped, has_stopped) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        stopping.stop();
        stopped.send(()).unwrap();
    });
    // The thread has answered and is on its way out, held at the gate.
    left.recv_timeout(SOON).unwrap();
    let waited = has_stopped.recv_timeout(Duration::from_millis(50)).is_err();

    let_through.send(()).unwrap();
    has_stopped.recv_timeout(SOON).unwrap();
    stopper.join().unwrap();
    assert!(agent.keeper.lock().unwrap().is_none());
    waited
}

#[test]
fn a_stop_waits_until_the_keepers_thread_has_ended() {
    assert!(stop_waits_for_the_thread(Some(true)));
    assert!(stop_waits_for_the_thread(Some(false)));
}

#[test]
fn a_stop_waits_for_the_thread_of_a_keeper_that_panicked() {
    assert!(stop_waits_for_the_thread(None));
}

#[test]
fn a_refusal_is_news_once_until_it_is_given_again() {
    let refusal = Refusal::default();
    assert!(!refusal.news(false));
    assert!(refusal.news(true));
    assert!(!refusal.news(true));
    assert!(!refusal.news(false));
    assert!(refusal.news(true));

    assert_eq!(refusal.given("it", Ok::<_, &str>(7)), Some(7));
    assert_eq!(refusal.given("it", Err::<i32, _>("refused")), None);
}

/// The real adapter, in a login said to be on screen.
fn mac() -> Mac {
    Mac::new(|| true, Arc::new(Watch::new().unwrap()))
}

/// A child that sleeps, killed and reaped when dropped, whatever its test
/// came to.
fn sleeper() -> (atlas_core::test_support::TestChild, i32) {
    let mut sleep = std::process::Command::new("/bin/sleep");
    let child = atlas_core::test_support::TestChild::spawn(sleep.arg("30"));
    let pid = child.process.id() as i32;
    (child, pid)
}

#[test]
fn a_live_process_is_read_exactly_and_one_that_is_gone_is_no_news() {
    let mac = mac();
    let (child, pid) = sleeper();
    let exact = mac.exact(pid).unwrap();
    assert_eq!(exact.token.pid(), pid);
    assert_eq!(exact.program, Path::new("/bin/sleep"));
    assert!(exact.age < Duration::from_secs(30));
    assert!(matches!(mac.process(pid), Seen::Found(found) if found.instance.pid == pid));

    drop(child);
    assert_eq!(mac.process(pid), Seen::Gone);
    assert_eq!(mac.exact(pid), None);
    assert!(!mac.signal(&exact.token, Signal::SIGCONT));
    assert!(!mac.watch(pid));
    let refused = &mac.refused;
    for refusal in [
        &refused.read,
        &refused.token,
        &refused.program,
        &refused.age,
        &refused.signal,
        &refused.start,
    ] {
        assert!(!refusal.0.get());
    }
}

#[test]
fn what_the_kernel_refuses_is_news_until_it_gives_it_again() {
    let mac = mac();
    let me = std::process::id() as i32;

    // No process has a negative PID, so its starts cannot be told.
    assert!(!mac.watch_starts(-1));
    assert!(mac.refused.starts.0.get());
    assert!(!mac.refused.starts.news(true));

    assert!(mac.watch_starts(me));
    assert!(!mac.refused.starts.0.get());
    assert!(mac.children(me).is_some());
    assert!(mac.processes(nix::unistd::getuid().as_raw()).is_some());
    // Other tests start processes meanwhile, so the wait may tell of one.
    mac.forget(me);
    assert!(mac.wait(Duration::ZERO).is_some());

    assert!(!mac.watch_starts(-1));
    assert!(mac.refused.starts.0.get());
}

#[test]
fn only_a_process_known_to_have_ended_makes_a_refusal_no_news() {
    let me = process::info(std::process::id() as i32).unwrap().unwrap();
    let zombie = Info {
        zombie: true,
        ..me.clone()
    };
    assert!(has_ended(&Ok(None)));
    assert!(has_ended(&Ok(Some(zombie))));
    assert!(!has_ended(&Ok(Some(me))));
    // The kernel did not say: the refusal it follows is still told.
    let unread = Err(std::io::Error::other("unread").into());
    assert!(!has_ended(&unread));
}

#[test]
fn a_dropped_handle_asks_twice_and_then_waits_no_longer() {
    let (agent, idle) = idle(true, true, Duration::from_millis(10));
    let told = agent.report.clone();
    std::mem::forget(agent.start());
    idle.began.recv_timeout(SOON).unwrap();

    // Deaf to its stop: the handle goes, and the keeper is left to end.
    drop(agent);
    assert_eq!(told.lock().unwrap().state, State::OnScreen);

    idle.go.send(()).unwrap();
    let end = Instant::now() + SOON;
    while told.lock().unwrap().state != State::Off {
        assert!(Instant::now() < end, "the keeper did not end");
        std::thread::yield_now();
    }
}

#[test]
fn a_keeper_that_could_not_let_the_agent_go_is_not_waited_for_again() {
    let (agent, idle) = idle(false, false, LET_GO);
    std::mem::forget(agent.start());
    idle.began.recv_timeout(SOON).unwrap();

    agent.stop();
    assert!(agent.keeper.lock().unwrap().is_none());
}

#[test]
fn a_report_says_its_state_in_one_word() {
    let words = [
        (State::Off, "off"),
        (State::OnScreen, "onScreen"),
        (State::Watching, "watching"),
        (State::Paused, "paused"),
        (State::Unavailable, "unavailable"),
    ];
    for (state, word) in words {
        let report = Report {
            state,
            pauses: 2,
            misses: 1,
            ..Report::OFF
        };
        assert_eq!(
            report.json(),
            json!({"state": word, "pauses": 2, "misses": 1, "rapportd": {"paused": false, "pauses": 0}})
        );
    }
}

/// launchd starts `rapportd` as `pid` for the account `uid` in `session`,
/// and it has become it.
fn start_rapportd(fake: &Fake, pid: i32, uid: u32, session: i32) -> Instance {
    let instance = fake.start(pid);
    fake.become_program(pid, QUIET, uid);
    fake.change(pid, |process| {
        process.info.name = "rapportd".into();
        process.session = session;
    });
    instance
}

fn quiet(keeper: &Keeper<&Fake>) -> Option<Instance> {
    keeper.quiet.map(|kept| kept.process)
}

#[test]
fn the_hidden_helpers_rapportd_is_paused_whatever_its_age() {
    let fake = Fake::new(true);
    let rapportd = start_rapportd(&fake, 300, HELPER, LOGIN);
    fake.pass(Duration::from_secs(3600));
    let keeper = keeper(&fake);
    assert_eq!(quiet(&keeper), Some(rapportd));
    assert!(fake.stopped(300));
    assert_eq!(fake.signals(), [(300, Signal::SIGSTOP)]);
    let report = report(&keeper);
    assert!(report.quiet);
    assert_eq!(report.quiet_pauses, 1);
}

#[test]
fn the_helpers_rapportd_runs_again_on_screen_and_is_paused_again_once_hidden() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    let mut keeper = keeper(&fake);
    fake.hidden.set(false);
    keeper.look();
    assert_eq!(quiet(&keeper), None);
    assert!(!fake.stopped(300));
    fake.hidden.set(true);
    keeper.look();
    assert!(fake.stopped(300));
    assert_eq!(report(&keeper).quiet_pauses, 2);
}

#[test]
fn the_persons_own_rapportd_and_another_logins_are_never_signalled() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, PERSON, LOGIN);
    start_rapportd(&fake, 301, HELPER, ELSEWHERE);
    let keeper = keeper(&fake);
    assert_eq!(quiet(&keeper), None);
    assert!(
        fake.signals()
            .iter()
            .all(|(pid, _)| *pid != 300 && *pid != 301)
    );
}

#[test]
fn a_program_only_named_rapportd_is_never_signalled() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    fake.change(300, |process| process.program = PathBuf::from(OTHER));
    let keeper = keeper(&fake);
    assert_eq!(quiet(&keeper), None);
    assert_eq!(fake.signals(), []);
}

#[test]
fn a_rapportd_that_ended_or_was_continued_gives_way_to_the_one_running() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    let mut keeper = keeper(&fake);
    fake.end(300);
    let next = start_rapportd(&fake, 310, HELPER, LOGIN);
    keeper.look();
    assert_eq!(quiet(&keeper), Some(next));
    fake.change(310, |process| process.info.stopped = false);
    keeper.look();
    assert!(fake.stopped(310), "someone else's continue is undone");
    assert_eq!(report(&keeper).quiet_pauses, 3);
}

#[test]
fn a_rapportd_left_paused_by_an_earlier_daemon_is_held_and_runs_again_on_screen() {
    let fake = Fake::new(true);
    let rapportd = start_rapportd(&fake, 300, HELPER, LOGIN);
    fake.change(300, |process| process.info.stopped = true);
    let mut keeper = keeper(&fake);
    assert_eq!(quiet(&keeper), Some(rapportd));
    assert_eq!(fake.signals(), [], "held as it was");
    fake.hidden.set(false);
    keeper.look();
    assert!(!fake.stopped(300));
}

#[test]
fn the_helpers_rapportd_runs_again_when_the_keeper_goes() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    let keeper = keeper(&fake);
    assert!(fake.stopped(300));
    assert!(keeper.end());
    assert!(!fake.stopped(300));
}

#[test]
fn a_rapportd_the_kernel_does_not_list_is_looked_for_again() {
    let fake = Fake::new(true);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    fake.lists.set(false);
    let mut keeper = keeper(&fake);
    assert_eq!(quiet(&keeper), None);
    fake.lists.set(true);
    keeper.look();
    assert!(fake.stopped(300));
}

#[test]
fn a_rapportd_left_paused_by_an_earlier_daemon_runs_again_when_the_helper_is_on_screen() {
    let fake = Fake::new(false);
    start_rapportd(&fake, 300, HELPER, LOGIN);
    fake.change(300, |process| process.info.stopped = true);
    let keeper = keeper(&fake);
    assert!(!fake.stopped(300));
    assert_eq!(quiet(&keeper), None);
}

#[test]
fn a_rapportd_held_again_after_a_continue_that_did_not_show_is_owed_nothing() {
    let fake = Fake::new(true);
    let rapportd = start_rapportd(&fake, 300, HELPER, LOGIN);
    let mut keeper = keeper(&fake);
    fake.obeys.set(false);
    fake.hidden.set(false);
    keeper.look();
    assert!(fake.stopped(300), "the continue did not show");
    fake.hidden.set(true);
    keeper.look();
    assert_eq!(quiet(&keeper), Some(rapportd));
    assert!(keeper.owed.is_empty());
    fake.obeys.set(true);
    keeper.look();
    assert!(fake.stopped(300), "held, so no continue is sent");
}
