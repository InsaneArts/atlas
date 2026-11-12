use super::*;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

fn idle(shared: f64, login: f64, hardware: Option<f64>) -> Idle {
    Idle {
        shared,
        login,
        hardware,
    }
}

#[test]
fn an_idle_time_that_follows_the_hardware_has_not_stopped() {
    assert_eq!(stopped(&idle(0.4, 0.4, Some(0.1))), Some(false));
}

#[test]
fn an_idle_time_trailing_by_secure_typing_half_second_has_not_stopped() {
    assert_eq!(stopped(&idle(0.9, 0.9, Some(0.3))), Some(false));
}

#[test]
fn an_idle_time_hours_behind_while_the_person_types_has_stopped() {
    assert_eq!(stopped(&idle(37_172.0, 1.0, Some(1.0))), Some(true));
}

#[test]
fn a_person_away_from_the_mac_says_nothing_of_the_idle_time() {
    assert_eq!(stopped(&idle(600.0, 600.0, Some(600.0))), None);
}

#[test]
fn hardware_activity_this_login_never_saw_says_nothing_of_the_idle_time() {
    // An app declaring activity resets the hardware's idle time alone.
    assert_eq!(stopped(&idle(600.0, 600.0, Some(1.0))), None);
}

#[test]
fn input_posted_into_this_login_says_nothing_of_the_idle_time() {
    // Posted input resets this login's idle time and not the hardware's.
    assert_eq!(stopped(&idle(600.0, 1.0, Some(600.0))), None);
}

#[test]
fn an_unknown_hardware_idle_time_says_nothing() {
    assert_eq!(stopped(&idle(37_172.0, 1.0, None)), None);
}

#[test]
fn the_first_restart_is_due_and_one_after_the_clock_was_set_well_back_too() {
    let now = SystemTime::now();
    assert!(due(None, now));
    assert!(due(Some(now + Duration::from_secs(600)), now));
    assert!(
        !due(Some(now + Duration::from_secs(1)), now),
        "a moment back"
    );
}

#[test]
fn a_restart_within_a_minute_of_the_last_is_not_due_and_one_a_minute_after_is() {
    let start = SystemTime::now();
    assert!(!due(Some(start), start + Duration::from_secs(59)));
    assert!(due(Some(start), start + RESET_EVERY));
}

/// Restart through the allowance at `path` at `now`, counting in `resets`.
fn restart_at(path: &Path, now: SystemTime, resets: &Cell<u32>) -> bool {
    restart_when_due(
        path,
        || now,
        &mut || true,
        || {
            resets.set(resets.get() + 1);
            true
        },
    )
}

#[test]
fn every_process_of_the_account_shares_one_restart_a_minute() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let start = UNIX_EPOCH + Duration::from_millis(1_000_900);
    let resets = Cell::new(0);
    assert!(restart_at(&path, start, &resets));
    assert!(!restart_at(&path, start + Duration::from_secs(30), &resets));
    // Fractions of a second count: 59.1 s after is not yet a minute.
    let almost = UNIX_EPOCH + Duration::from_secs(1_060);
    assert!(!restart_at(&path, almost, &resets));
    // A caller whose clock reads a moment before the last restart, as one
    // that read it before waiting its turn would, makes none.
    assert!(!restart_at(&path, start - Duration::from_secs(1), &resets));
    assert!(restart_at(&path, start + RESET_EVERY, &resets));
    assert_eq!(resets.get(), 2);
}

#[test]
fn a_restart_while_another_process_holds_the_allowance_is_not_made() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let held = files::open_lock(&path.join(LOCK)).unwrap();
    files::lock_within(&held, Lock::Exclusive, Duration::ZERO).unwrap();
    let resets = Cell::new(0);
    assert!(!restart_at(&path, SystemTime::now(), &resets));
    assert_eq!(resets.get(), 0);
}

#[test]
fn a_restart_no_longer_wanted_once_the_allowance_is_held_is_neither_made_nor_noted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let now = SystemTime::now();
    let resets = Cell::new(0);
    assert!(!restart_when_due(
        &path,
        || now,
        &mut || false,
        || {
            resets.set(1);
            true
        }
    ));
    assert_eq!(resets.get(), 0);
    assert!(restart_at(&path, now, &resets), "nothing was noted");
}

#[test]
fn each_idle_time_is_written_by_its_state() {
    assert_eq!(json!(IdleTime::Live), json!({"state": "live"}));
    assert_eq!(json!(IdleTime::Unknown), json!({"state": "unknown"}));
    assert_eq!(json!(IdleTime::Stuck), json!({"state": "stuck"}));
    assert_eq!(
        json!(IdleTime::Held {
            app: Some("Terminal".to_owned())
        }),
        json!({"state": "held", "app": "Terminal"})
    );
    assert_eq!(
        json!(IdleTime::Held { app: None }),
        json!({"state": "held", "app": null})
    );
}

#[test]
fn an_app_keeping_secure_typing_on_holds_the_record_named_or_not() {
    for name in [Some("Terminal".to_owned()), None] {
        let holder = Holder {
            login_window: false,
            name: name.clone(),
        };
        assert_eq!(stopped_by(Some(holder)), IdleTime::Held { app: name });
    }
}

#[test]
fn the_login_window_keeping_secure_typing_on_is_no_app_to_quit() {
    let holder = Holder {
        login_window: true,
        name: Some("loginwindow".to_owned()),
    };
    assert_eq!(stopped_by(Some(holder)), IdleTime::Stuck);
}

#[test]
fn a_record_stopped_with_nothing_holding_secure_typing_is_stuck() {
    assert_eq!(stopped_by(None), IdleTime::Stuck);
}

#[test]
#[ignore = "needs this login on the screen and someone using the Mac"]
fn the_idle_time_in_a_login_in_use_is_live() {
    assert_eq!(IdleTime::now(Look::Only), IdleTime::Live);
}

/// A Mac that answers each read from a script, and counts what is done.
struct Scripted {
    logins: RefCell<VecDeque<Option<Login>>>,
    idles: RefCell<VecDeque<Idle>>,
    secure: RefCell<VecDeque<Option<SecureTyping>>>,
    holder: Holder,
    waits: Cell<u32>,
    resets: Cell<u32>,
    /// Run as the last scripted login is read.
    on_last_login: RefCell<Option<Box<dyn FnOnce()>>>,
}

/// Leave to restart, given.
fn given() -> Leave {
    let leave = Leave::default();
    leave.give();
    leave
}

/// The person's login, on the screen or not.
fn person(on_screen: bool) -> Option<Login> {
    Some(Login {
        uid: 501,
        on_screen,
    })
}

/// The person typing a moment ago while the record apps read is `shared`
/// seconds old.
fn typing(shared: f64) -> Idle {
    idle(shared, 0.5, Some(0.5))
}

impl Scripted {
    fn new(
        logins: impl IntoIterator<Item = Option<Login>>,
        idles: impl IntoIterator<Item = Idle>,
        secure: impl IntoIterator<Item = Option<SecureTyping>>,
    ) -> Self {
        Self {
            logins: RefCell::new(logins.into_iter().collect()),
            idles: RefCell::new(idles.into_iter().collect()),
            secure: RefCell::new(secure.into_iter().collect()),
            holder: Holder {
                login_window: false,
                name: Some("Terminal".to_owned()),
            },
            waits: Cell::new(0),
            resets: Cell::new(0),
            on_last_login: RefCell::new(None),
        }
    }
}

impl Mac for Scripted {
    fn login(&self) -> Option<Login> {
        let login = self.logins.borrow_mut().pop_front().expect("a login read");
        if self.logins.borrow().is_empty()
            && let Some(last) = self.on_last_login.borrow_mut().take()
        {
            last();
        }
        login
    }

    fn idle(&self) -> Idle {
        self.idles.borrow_mut().pop_front().expect("an idle read")
    }

    fn wait(&self, _: Duration) {
        self.waits.set(self.waits.get() + 1);
    }

    fn secure_typing(&self) -> Option<SecureTyping> {
        self.secure
            .borrow_mut()
            .pop_front()
            .expect("a secure typing read")
    }

    fn restart_when_due(&self, ready: &mut dyn FnMut() -> bool, leave: &Leave) -> bool {
        ready() && leave.while_held(|| self.resets.set(self.resets.get() + 1))
    }

    fn holder(&self, _: i64) -> Holder {
        self.holder.clone()
    }
}

#[test]
fn a_record_a_keystroke_behind_after_a_minute_away_is_live_once_it_catches_up() {
    let mac = Scripted::new(
        [person(true), person(true)],
        [typing(60.0), typing(0.5)],
        [],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(look, IdleTime::Live);
    assert_eq!(mac.waits.get(), 1);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn a_stopped_record_no_app_holds_is_restarted_by_the_service() {
    let mac = Scripted::new(
        [person(true), person(true), person(true)],
        [typing(600.0), typing(601.0), typing(0.5)],
        [Some(SecureTyping::Off), Some(SecureTyping::Off)],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(look, IdleTime::Live);
    assert_eq!(mac.resets.get(), 1);
}

#[test]
fn a_look_alone_never_restarts_the_record() {
    let mac = Scripted::new(
        [person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::Off)],
    );
    assert_eq!(look(&mac, Look::Only), IdleTime::Stuck);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn an_unreadable_secure_typing_state_is_unknown_and_restarts_nothing() {
    let mac = Scripted::new(
        [person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [None],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(look, IdleTime::Unknown);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn a_login_that_left_the_screen_before_the_restart_restarts_nothing() {
    let mac = Scripted::new(
        [person(true), person(true), person(false)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::Off)],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(look, IdleTime::Unknown);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn a_record_an_app_holds_names_the_app_and_restarts_nothing() {
    let mac = Scripted::new(
        [person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::HeldBy(42))],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(
        look,
        IdleTime::Held {
            app: Some("Terminal".to_owned())
        }
    );
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn a_login_off_the_screen_says_nothing_of_the_record() {
    let mac = Scripted::new([person(false)], [], []);
    assert_eq!(look(&mac, Look::AndRestart(given())), IdleTime::Unknown);
}

#[test]
fn a_switch_during_the_second_look_says_nothing_of_the_record() {
    for second in [typing(0.5), typing(601.0)] {
        let mac = Scripted::new(
            [person(true), person(false)],
            [typing(600.0), second],
            [Some(SecureTyping::Off)],
        );
        let look = look(&mac, Look::AndRestart(given()));
        assert_eq!(look, IdleTime::Unknown);
        assert_eq!(mac.resets.get(), 0);
    }
}

#[test]
fn an_app_that_turns_secure_typing_on_just_before_the_restart_stops_it() {
    let mac = Scripted::new(
        [person(true), person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::Off), Some(SecureTyping::HeldBy(42))],
    );
    let look = look(&mac, Look::AndRestart(given()));
    assert_eq!(look, IdleTime::Unknown);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn the_minute_counts_from_the_end_of_a_slow_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let start = SystemTime::now();
    let reads = Cell::new(0);
    // The restart ends 50 s after its allowance was first read.
    let clock = || {
        reads.set(reads.get() + 1);
        if reads.get() > 2 {
            start + Duration::from_secs(50)
        } else {
            start
        }
    };
    assert!(restart_when_due(&path, clock, &mut || true, || true));
    let resets = Cell::new(0);
    assert!(!restart_at(&path, start + Duration::from_secs(70), &resets));
    assert!(restart_at(&path, start + Duration::from_secs(110), &resets));
}

#[test]
fn a_note_that_is_there_but_cannot_be_read_allows_no_restart() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(LAST), "not a time").unwrap();
    let resets = Cell::new(0);
    assert!(!restart_at(dir.path(), SystemTime::now(), &resets));
    assert_eq!(resets.get(), 0);
}

#[test]
fn leave_taken_back_before_the_restart_stops_it() {
    let mac = Scripted::new(
        [person(true), person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::Off), Some(SecureTyping::Off)],
    );
    let leave = given();
    leave.take_back();
    assert_eq!(look(&mac, Look::AndRestart(leave)), IdleTime::Stuck);
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn leave_taken_back_while_the_last_reads_are_made_stops_the_restart() {
    let leave = given();
    let mac = Scripted::new(
        [person(true), person(true), person(true)],
        [typing(600.0), typing(601.0)],
        [Some(SecureTyping::Off), Some(SecureTyping::Off)],
    );
    // The service stops while the last login read is made.
    let taking = leave.clone();
    *mac.on_last_login.borrow_mut() = Some(Box::new(move || taking.take_back()));
    look(&mac, Look::AndRestart(leave));
    assert_eq!(mac.resets.get(), 0);
}

#[test]
fn taking_leave_back_waits_for_a_restart_under_way() {
    let leave = given();
    let (started, running) = std::sync::mpsc::channel();
    let (finish, finished) = std::sync::mpsc::channel::<()>();
    let restarting = leave.clone();
    let worker = std::thread::spawn(move || {
        restarting.while_held(|| {
            started.send(()).unwrap();
            finished.recv().unwrap();
        })
    });
    running.recv().unwrap();
    let taking = leave.clone();
    let taker = std::thread::spawn(move || taking.take_back());
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !taker.is_finished(),
        "not taken back while the restart runs"
    );
    finish.send(()).unwrap();
    assert!(worker.join().unwrap());
    taker.join().unwrap();
    assert!(!leave.holds());
    assert!(!leave.while_held(|| unreachable!()));
}

#[test]
fn the_allowance_folder_is_made_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Library/Application Support/Atlas");
    let resets = Cell::new(0);
    assert!(restart_at(&folder, SystemTime::now(), &resets));
    assert_eq!(resets.get(), 1);
}

#[test]
fn a_person_gone_idle_after_the_restart_says_nothing_of_the_record() {
    for after in [idle(600.0, 600.0, Some(600.0)), idle(600.0, 0.5, None)] {
        let mac = Scripted::new(
            [person(true), person(true), person(true)],
            [typing(600.0), typing(601.0), after],
            [Some(SecureTyping::Off), Some(SecureTyping::Off)],
        );
        assert_eq!(look(&mac, Look::AndRestart(given())), IdleTime::Unknown);
        assert_eq!(mac.resets.get(), 1);
    }
}
