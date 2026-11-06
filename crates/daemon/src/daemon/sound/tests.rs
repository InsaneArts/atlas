use super::*;
use atlas_core::error::{Code, Error};
use atlas_native::native::audio::Unbuilt;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn the_mute_starts_only_once_the_person_allowed_it() {
    assert_eq!(plan(true, true, Access::Granted), Plan::Mute);
    assert_eq!(
        plan(true, true, Access::Undetermined),
        Plan::Unavailable(Reason::Unasked)
    );
    assert_eq!(
        plan(true, true, Access::Denied),
        Plan::Unavailable(Reason::Denied)
    );
}

#[test]
fn an_older_macos_cannot_mute_and_sound_on_needs_nothing() {
    assert_eq!(
        plan(true, false, Access::Granted),
        Plan::Unavailable(Reason::TooOld)
    );
    for access in [Access::Granted, Access::Denied, Access::Undetermined] {
        assert_eq!(plan(false, true, access), Plan::Off);
    }
}

#[test]
fn atlas_asks_only_for_a_wanted_mute_never_answered_and_not_before_a_restart() {
    assert!(asks(true, true, Access::Undetermined, false));
    assert!(!asks(false, true, Access::Undetermined, false));
    assert!(!asks(true, false, Access::Undetermined, false));
    assert!(!asks(true, true, Access::Denied, false));
    assert!(!asks(true, true, Access::Granted, false));
    assert!(!asks(true, true, Access::Undetermined, true));
}

#[test]
fn a_report_says_why_the_helper_cannot_be_muted() {
    let report = |wanted, state| Report { wanted, state }.json();
    assert_eq!(
        report(true, State::Muted),
        json!({"helperSound": "muted", "reason": null, "wanted": true})
    );
    assert_eq!(
        report(false, State::On),
        json!({"helperSound": "on", "reason": null, "wanted": false})
    );
    let unavailable = report(true, State::Unavailable(Reason::Unasked));
    assert_eq!(unavailable["helperSound"], "unavailable");
    assert!(
        unavailable["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Switch to the helper"))
    );
}

#[test]
fn the_helper_stays_silent_unless_sound_on_was_kept() {
    let folder = tempfile::tempdir().unwrap();
    let note = folder.path().join(".atlas").join(NOTE);
    assert!(wanted(&note));

    remember(&note, false).unwrap();
    assert!(!wanted(&note));

    remember(&note, true).unwrap();
    assert!(wanted(&note));

    std::fs::write(&note, "loud\n").unwrap();
    assert!(wanted(&note));
}

#[test]
fn a_daemon_without_the_mute_answers_from_its_report() {
    let sound = Sound::detached();

    assert_eq!(
        sound.set(false),
        Set::Done(Report {
            wanted: true,
            state: State::Unavailable(Reason::Failed)
        })
    );
    assert!(!sound.ask(false, || true));
}

#[test]
fn atlas_reads_whether_the_helper_is_on_screen_last_right_before_asking() {
    let steps = RefCell::new(Vec::new());
    let asked = ask_now(
        true,
        true,
        false,
        || {
            steps.borrow_mut().push("access");
            Access::Undetermined
        },
        || {
            steps.borrow_mut().push("on screen");
            true
        },
        || {
            steps.borrow_mut().push("request");
            true
        },
    );

    assert!(asked);
    assert_eq!(*steps.borrow(), ["access", "on screen", "request"]);
}

#[test]
fn a_helper_that_left_the_screen_is_never_asked() {
    let asked = ask_now(
        true,
        true,
        false,
        || Access::Undetermined,
        || false,
        || panic!("asked off screen"),
    );

    assert!(!asked);
}

#[test]
fn an_answered_or_unwanted_permission_is_never_asked_for() {
    for (muted, access) in [
        (true, Access::Denied),
        (true, Access::Granted),
        (false, Access::Undetermined),
    ] {
        let asked = ask_now(muted, true, false, || access, || true, || panic!("asked"));
        assert!(!asked);
    }
}

#[test]
fn a_failed_rebuild_is_tried_again_a_few_times_then_given_up() {
    assert_eq!(retry_after(1), Some(Duration::from_secs(1)));
    assert_eq!(
        retry_after(RETRIES),
        Some(Duration::from_secs(u64::from(RETRIES)))
    );
    assert_eq!(retry_after(RETRIES + 1), None);
    assert_eq!(retry_after(0), None);
}

/// What the fake Core Audio saw, in order.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Listened,
    Built(u32),
    TornDown(u32),
}

type Steps = Rc<RefCell<Vec<Step>>>;

/// Core Audio with the helper processes in `objects`, the permission
/// allowed; builds fail while `refuse` is set.
#[derive(Clone, Default)]
struct Fake {
    steps: Steps,
    built: Rc<Cell<u32>>,
    objects: Rc<RefCell<Vec<u32>>>,
    refuse: Rc<Cell<bool>>,
    /// Core Audio's restarts so far.
    generation: Rc<Cell<u64>>,
    /// Core Audio restarts while the mute reads the permission.
    restart_on_access: Rc<Cell<bool>>,
    /// The mute's objects cannot be checked, so it cannot be ended yet.
    unreadable: Rc<Cell<bool>>,
    /// Core Audio refuses to be listened to.
    deaf: Rc<Cell<bool>>,
}

impl Fake {
    /// Core Audio restarts: the generation its notice will carry.
    fn restart(&self) -> u64 {
        self.generation.set(self.generation.get() + 1);
        self.generation.get()
    }
}

impl Backend for Fake {
    type Mute = u32;

    fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn objects(&self) -> Result<Vec<u32>> {
        Ok(self.objects.borrow().clone())
    }

    fn access(&self) -> Access {
        if self.restart_on_access.get() {
            self.restart();
        }
        Access::Granted
    }

    fn still_ours(&self, _: &[u32]) -> Result<bool> {
        Ok(true)
    }

    fn start(&self, _: &mut u32, _: u64) -> Result<()> {
        Ok(())
    }

    fn build(&self, _: &[u32]) -> std::result::Result<u32, Box<Unbuilt<u32>>> {
        if self.refuse.get() {
            return Err(Box::new(Unbuilt {
                error: Error::new(Code::Daemon, "refused"),
                made: None,
            }));
        }
        let mute = self.built.get() + 1;
        self.built.set(mute);
        self.steps.borrow_mut().push(Step::Built(mute));
        Ok(mute)
    }

    fn tear_down(&self, mute: u32) -> Option<u32> {
        if self.unreadable.get() {
            return Some(mute);
        }
        self.steps.borrow_mut().push(Step::TornDown(mute));
        None
    }
}

impl Audio for Fake {
    type Backend = Fake;
    type Listener = ();

    fn backend(&self) -> Option<Fake> {
        Some(self.clone())
    }

    fn access(&self) -> Access {
        Access::Granted
    }

    fn listen(&self, _: Sender<Message>) -> Result<()> {
        if self.deaf.get() {
            return Err(Error::new(Code::Daemon, "deaf"));
        }
        self.steps.borrow_mut().push(Step::Listened);
        Ok(())
    }
}

/// A driver over the fake with one helper process, and what the fake saw.
fn driver() -> (Driver<Fake>, Steps) {
    let fake = Fake::default();
    fake.objects.replace(vec![7]);
    let steps = fake.steps.clone();
    let (sender, _receiver) = mpsc::channel();
    let driver = Driver::new(fake, None, &Sound::detached(), sender);
    driver.muted.store(true, Ordering::Relaxed);
    (driver, steps)
}

/// The person wants the helper silent, and it is.
const MUTED: Report = Report {
    wanted: true,
    state: State::Muted,
};

/// The person wants sound on, and it is.
const ON: Report = Report {
    wanted: false,
    state: State::On,
};

/// A running mute over the fake, and what the fake saw.
fn muted() -> (Driver<Fake>, Steps) {
    let (mut driver, steps) = driver();
    assert_eq!(driver.apply(), MUTED);
    (driver, steps)
}

/// Turn the helper's sound on (`false`) or off, and what the mute said.
fn choose(driver: &mut Driver<Fake>, muted: bool) -> Option<Report> {
    let (answer, answered) = mpsc::channel();
    driver.handle(Some(Message::Want(muted, answer)));
    answered.recv().unwrap()
}

/// Let the rebuild due fall due now.
fn tick(driver: &mut Driver<Fake>) {
    if driver.rebuild.is_some() {
        driver.rebuild = Some(Instant::now());
    }
    driver.handle(None);
}

/// What the driver last said.
fn said(driver: &Driver<Fake>) -> Report {
    *driver.report.lock().unwrap()
}

/// Try ending earlier mutes as if their time came.
fn retry_cleanup(driver: &mut Driver<Fake>) {
    if let Some(engine) = &mut driver.engine {
        engine.sweep(Instant::now() + Duration::from_secs(120));
    }
}

#[test]
fn a_restart_during_the_permission_read_makes_no_tap_from_the_stale_list() {
    let (mut driver, steps) = driver();
    driver.audio.restart_on_access.set(true);

    assert_eq!(driver.apply().state, State::Unavailable(Reason::Behind));
    assert!(
        driver.rebuild.is_some(),
        "it lists the processes again soon"
    );
    assert_eq!(*steps.borrow(), [Step::Listened]);
}

#[test]
fn an_unmute_after_a_restart_not_yet_in_line_still_ends_the_old_mute() {
    let (mut driver, steps) = muted();
    driver.audio.restart();

    assert_eq!(choose(&mut driver, false), Some(ON));
    assert_eq!(
        *steps.borrow(),
        [Step::Listened, Step::Built(1), Step::TornDown(1)]
    );
}

#[test]
fn a_replacement_built_before_the_restart_notice_is_kept_and_sound_on_ends_it() {
    let (mut driver, steps) = muted();
    let generation = driver.audio.restart();
    driver.handle(Some(Message::Check));

    driver.handle(Some(Message::Changed(Change::Restarted, generation)));
    tick(&mut driver);
    assert_eq!(choose(&mut driver, false), Some(ON));

    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::TornDown(1),
            Step::Built(2),
            Step::Listened,
            Step::TornDown(2)
        ]
    );
}

#[test]
fn a_restart_notice_ends_the_old_mute_and_rebuilds_after_a_pause() {
    let (mut driver, steps) = muted();
    let generation = driver.audio.restart();

    driver.handle(Some(Message::Changed(Change::Restarted, generation)));
    assert!(driver.rebuild.is_some(), "only the rebuild waits");
    tick(&mut driver);
    assert_eq!(choose(&mut driver, false), Some(ON));

    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::TornDown(1),
            Step::Listened,
            Step::Built(2),
            Step::TornDown(2)
        ]
    );
}

#[test]
fn a_mute_that_misses_new_helper_apps_says_so_and_keeps_the_old_one() {
    let (mut driver, steps) = muted();
    driver.audio.objects.replace(vec![7, 8]);
    driver.audio.refuse.set(true);

    driver.handle(Some(Message::Changed(Change::Processes, 0)));
    tick(&mut driver);

    assert_eq!(said(&driver).state, State::Unavailable(Reason::Behind));
    assert!(driver.rebuild.is_some(), "it is tried again");
    assert_eq!(*steps.borrow(), [Step::Listened, Step::Built(1)]);

    driver.audio.refuse.set(false);
    tick(&mut driver);
    assert_eq!(said(&driver), MUTED);
}

#[test]
fn a_choice_that_could_not_be_saved_changes_nothing_and_says_so() {
    let (mut driver, steps) = muted();
    let folder = tempfile::tempdir().unwrap();
    let blocker = folder.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    driver.note = Some(blocker.join(NOTE));

    assert_eq!(choose(&mut driver, false), None);
    assert!(driver.wants());
    assert_eq!(*steps.borrow(), [Step::Listened, Step::Built(1)]);
}

#[test]
fn a_change_the_mute_never_confirmed_is_uncertain_not_the_old_state() {
    let (sender, receiver) = mpsc::channel();
    let sound = Sound {
        sender: Some(sender),
        ..Sound::detached()
    };
    let unanswered = std::thread::spawn(move || {
        // The thread takes the change and ends without confirming it.
        let _ = receiver.recv();
    });

    assert_eq!(sound.set(true), Set::Uncertain);
    unanswered.join().unwrap();
    assert_eq!(
        sound.set(false),
        Set::Uncertain,
        "a gone thread confirms nothing"
    );
}

#[test]
fn sound_on_with_an_earlier_mute_still_ending_says_so_and_keeps_trying() {
    let (mut driver, steps) = muted();
    driver.audio.unreadable.set(true);

    let report = choose(&mut driver, false).unwrap();
    assert!(!report.wanted, "the switch shows the choice");
    assert_eq!(report.state, State::Unavailable(Reason::Ending));
    assert!(driver.due().is_some(), "it is tried again");

    driver.audio.unreadable.set(false);
    retry_cleanup(&mut driver);
    driver.handle(None);

    assert_eq!(said(&driver), ON);
    assert_eq!(driver.due(), None);
    assert_eq!(
        *steps.borrow(),
        [Step::Listened, Step::Built(1), Step::TornDown(1)]
    );
}

#[test]
fn sound_off_after_an_unfinished_sound_on_listens_again_for_new_apps() {
    let (mut driver, steps) = muted();
    driver.audio.unreadable.set(true);
    choose(&mut driver, false);
    assert!(driver.listener.is_none());

    assert_eq!(choose(&mut driver, true), Some(MUTED));

    assert!(driver.listener.is_some());
    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::Listened,
            Step::Built(2)
        ]
    );
}

#[test]
fn an_earlier_mute_not_ended_by_a_good_rebuild_is_still_tried_again() {
    let (mut driver, steps) = muted();
    driver.audio.objects.replace(vec![7, 8]);
    driver.audio.unreadable.set(true);

    driver.handle(Some(Message::Changed(Change::Processes, 0)));
    tick(&mut driver);
    assert_eq!(said(&driver), MUTED);
    assert!(driver.due().is_some(), "the earlier mute is tried again");

    driver.audio.unreadable.set(false);
    retry_cleanup(&mut driver);

    assert_eq!(driver.due(), None);
    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::Built(2),
            Step::TornDown(1)
        ]
    );
}

#[test]
fn a_rebuild_without_listening_says_new_apps_may_be_heard_and_listens_again() {
    let (mut driver, steps) = muted();
    let generation = driver.audio.restart();
    driver.audio.deaf.set(true);

    driver.handle(Some(Message::Changed(Change::Restarted, generation)));
    tick(&mut driver);
    assert_eq!(said(&driver).state, State::Unavailable(Reason::Failed));
    assert!(driver.rebuild.is_some(), "listening is tried again");

    driver.audio.deaf.set(false);
    tick(&mut driver);

    assert_eq!(said(&driver), MUTED);
    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::TornDown(1),
            Step::Listened,
            Step::Built(2)
        ]
    );
}

#[test]
fn a_new_helper_app_is_muted_at_once_and_an_ended_one_waits_to_settle() {
    let (mut driver, steps) = muted();

    driver.audio.objects.replace(vec![7, 8]);
    driver.handle(Some(Message::Changed(Change::Processes, 0)));
    assert_eq!(said(&driver), MUTED);
    assert!(driver.rebuild.is_none(), "no settle for a new app");

    driver.audio.objects.replace(vec![7]);
    driver.handle(Some(Message::Changed(Change::Processes, 0)));
    assert!(driver.rebuild.is_some(), "an ended app waits to settle");

    assert_eq!(
        *steps.borrow(),
        [
            Step::Listened,
            Step::Built(1),
            Step::Built(2),
            Step::TornDown(1)
        ]
    );
}
