use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
};

/// A guest as a test makes it: what it did, in order, and what it reads.
#[derive(Default)]
struct Fake {
    did: Mutex<Vec<&'static str>>,
    permitted: AtomicBool,
    granted: AtomicBool,
    /// Its looks cannot tell.
    unsure: AtomicBool,
    /// Its looks never answer.
    late: AtomicBool,
    looks: AtomicUsize,
}

impl Fake {
    fn granting() -> Self {
        let fake = Self::default();
        fake.granted.store(true, SeqCst);
        fake
    }

    fn did(&self) -> Vec<&'static str> {
        self.did.lock().unwrap().clone()
    }

    fn note(&self, done: &'static str) {
        self.did.lock().unwrap().push(done);
    }
}

impl Recording for Fake {
    fn permitted(&self) -> bool {
        self.permitted.load(SeqCst)
    }

    fn seen(&self) -> impl Future<Output = Option<bool>> + Send {
        self.looks.fetch_add(1, SeqCst);
        let granted = self.granted.load(SeqCst);
        let unsure = self.unsure.load(SeqCst);
        let late = self.late.load(SeqCst);
        async move {
            if late {
                std::future::pending::<()>().await;
            }
            (!unsure).then_some(granted)
        }
    }

    fn request(&self) {
        self.note("request");
    }

    fn restart(&self) -> impl Future<Output = bool> + Send {
        self.note("restart");
        std::future::ready(true)
    }
}

/// A record in a folder of a test's own.
fn record() -> (tempfile::TempDir, Record) {
    let folder = tempfile::tempdir().unwrap();
    let record = Record::at(folder.path().join("recording.json"));
    (folder, record)
}

/// Watch `guest` for `long`, or until it ends.
async fn watched_for(guest: &Fake, record: &Record, long: Duration) {
    let _ = tokio::time::timeout(long, watch(guest, record)).await;
}

#[tokio::test(start_paused = true)]
async fn macos_is_asked_once_for_the_guest_once_capture_is_noted_held() {
    let (_folder, record) = record();
    let guest = Fake::default();

    watched_for(&guest, &record, Duration::from_secs(10)).await;
    let again = Fake::default();
    watched_for(&again, &record, Duration::from_secs(10)).await;

    assert_eq!(guest.did(), ["request"]);
    assert!(record.held());
    assert!(again.did().is_empty(), "asked again: {:?}", again.did());
}

#[tokio::test(start_paused = true)]
async fn a_hold_that_cannot_be_noted_asks_macos_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let record = Record::at(folder.path().join("missing").join("recording.json"));
    let guest = Fake::default();

    watched_for(&guest, &record, Duration::from_secs(10)).await;

    assert!(guest.did().is_empty(), "{:?}", guest.did());
}

#[tokio::test(start_paused = true)]
async fn a_daemon_that_may_record_asks_nothing_and_stops_nothing() {
    let (_folder, record) = record();
    let guest = Fake::granting();
    guest.permitted.store(true, SeqCst);

    watch(&guest, &record).await;

    assert!(guest.did().is_empty());
    assert_eq!(guest.looks.load(SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_grant_restarts_once_and_a_daemon_started_again_still_denied_never_restarts_again() {
    let (_folder, record) = record();
    let first = Fake::granting();

    watched_for(&first, &record, Duration::from_secs(60)).await;
    let replacement = Fake::granting();
    watched_for(&replacement, &record, Duration::from_secs(3600)).await;

    assert_eq!(first.did(), ["request", "restart"]);
    assert!(replacement.did().is_empty(), "{:?}", replacement.did());
}

#[tokio::test(start_paused = true)]
async fn a_grant_taken_away_and_given_again_restarts_once_more() {
    let (_folder, record) = record();
    watched_for(&Fake::granting(), &record, Duration::from_secs(60)).await;
    let replacement = Fake::default();
    watched_for(&replacement, &record, Duration::from_secs(10)).await;

    replacement.granted.store(true, SeqCst);
    watched_for(&replacement, &record, Duration::from_secs(10)).await;

    assert_eq!(replacement.did(), ["restart"]);
}

#[tokio::test(start_paused = true)]
async fn a_restart_that_cannot_be_kept_is_never_made() {
    let folder = tempfile::tempdir().unwrap();
    let record = Record::at(folder.path().join("missing").join("recording.json"));
    let guest = Fake::granting();

    watched_for(&guest, &record, Duration::from_secs(600)).await;

    assert!(guest.did().is_empty(), "{:?}", guest.did());
}

#[tokio::test(start_paused = true)]
async fn a_grant_is_looked_for_every_two_seconds_for_a_minute_then_every_minute() {
    let (_folder, record) = record();
    let guest = Fake::default();
    let counting = async {
        tokio::time::sleep(EAGERLY).await;
        let eager = guest.looks.load(SeqCst);
        tokio::time::sleep(Duration::from_secs(600)).await;
        (eager, guest.looks.load(SeqCst) - eager)
    };

    let (eager, later) = tokio::select! {
        () = watch(&guest, &record) => panic!("it may never record"),
        counted = counting => counted,
    };

    assert!((29..=31).contains(&eager), "{eager} in the first minute");
    assert!((9..=11).contains(&later), "{later} in the next ten minutes");
}

/// A record noting capture held, as a daemon that could not record left it.
fn held() -> (tempfile::TempDir, Record) {
    let (folder, record) = record();
    assert!(record.keep(Kept {
        held: true,
        asked: true,
        ..Kept::default()
    }));
    (folder, record)
}

#[test]
fn a_daemon_that_may_record_where_capture_was_held_stops_agents_before_the_hold_is_forgotten() {
    let (_folder, record) = held();
    let stopped = AtomicBool::new(false);

    let admitted = record.reconcile(true, || {
        assert!(record.held(), "forgotten before the Stop was saved");
        stopped.store(true, SeqCst);
        true
    });

    assert!(admitted);
    assert!(stopped.load(SeqCst));
    assert!(!record.held());
    assert!(record.reconcile(true, || panic!("stopped twice for one grant")));
}

#[test]
fn a_daemon_still_denied_where_capture_was_held_stops_nothing_and_keeps_the_hold() {
    let (_folder, record) = held();

    assert!(record.reconcile(false, || panic!("stopped while denied")));
    assert!(record.held());
}

#[test]
fn a_stop_for_a_grant_that_cannot_be_saved_admits_nothing_and_keeps_the_hold() {
    let (_folder, record) = held();

    assert!(!record.reconcile(true, || false));
    assert!(record.held());
}

#[test]
fn a_guest_whose_capture_was_never_held_stops_nothing() {
    let (_folder, record) = record();

    assert!(record.reconcile(true, || panic!("stopped with nothing held")));
    assert!(!record.held());
}

#[tokio::test(start_paused = true)]
async fn a_look_that_cannot_tell_keeps_a_restart_made_for_a_grant_however_it_fails() {
    for fail in [
        &|guest: &Fake| guest.late.store(true, SeqCst),
        &|guest: &Fake| {
            guest.unsure.store(true, SeqCst);
        },
    ] as [&dyn Fn(&Fake); 2]
    {
        let (_folder, record) = record();
        watched_for(&Fake::granting(), &record, Duration::from_secs(60)).await;
        let failing = Fake::granting();
        fail(&failing);
        watched_for(&failing, &record, Duration::from_secs(600)).await;

        let granting = Fake::granting();
        watched_for(&granting, &record, Duration::from_secs(3600)).await;

        assert!(failing.looks.load(SeqCst) > 0);
        assert!(failing.did().is_empty(), "{:?}", failing.did());
        assert!(
            granting.did().is_empty(),
            "restarted again: {:?}",
            granting.did()
        );
    }
}
