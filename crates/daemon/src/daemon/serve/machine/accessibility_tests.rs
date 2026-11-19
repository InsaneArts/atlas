use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering::SeqCst},
};

/// A guest as a test makes it: what it did, in order, and what it reads.
#[derive(Default)]
struct Fake {
    did: Mutex<Vec<&'static str>>,
    recording: AtomicBool,
    trusted: AtomicBool,
}

impl Fake {
    fn recording() -> Self {
        let fake = Self::default();
        fake.recording.store(true, SeqCst);
        fake
    }

    fn did(&self) -> Vec<&'static str> {
        self.did.lock().unwrap().clone()
    }
}

impl Trust for Fake {
    fn recording(&self) -> bool {
        self.recording.load(SeqCst)
    }

    fn trusted(&self) -> bool {
        self.trusted.load(SeqCst)
    }

    fn request(&self) {
        self.did.lock().unwrap().push("request");
    }

    fn open_settings(&self) {
        self.did.lock().unwrap().push("settings");
    }
}

/// A record in a folder of a test's own.
fn record() -> (tempfile::TempDir, Record) {
    let folder = tempfile::tempdir().unwrap();
    let record = Record::at(folder.path().join("accessibility.json"));
    (folder, record)
}

/// Watch `guest` for `long`, or until it ends: whether it ended.
async fn watched_for(guest: &Fake, record: &Record, long: Duration) -> bool {
    tokio::time::timeout(long, watch(guest, record))
        .await
        .is_ok()
}

#[tokio::test(start_paused = true)]
async fn a_fresh_guest_is_asked_for_accessibility_only_after_the_restart_that_takes_screen_recording()
 {
    let (_folder, record) = record();
    let before = Fake::default();

    let ended = watched_for(&before, &record, Duration::from_secs(600)).await;

    assert!(!ended, "it waits for Screen Recording");
    assert!(before.did().is_empty(), "{:?}", before.did());
    assert!(!record.asked());

    // Screen Recording granted, its daemon started again to take it.
    let after = Fake::recording();
    let asked = watched_for(&after, &record, Duration::from_secs(1)).await;
    let prompted = after.did();
    let fresh = Fake::recording();
    let opened = watched_for(&fresh, &Record::at("/nonexistent/a.json"), LOOK).await;

    assert!(!asked, "it waits for the person after asking");
    assert_eq!(prompted, ["request"], "macOS's prompt at once");
    assert!(record.asked(), "kept asked before it asked");
    assert!(opened && fresh.did().is_empty(), "nothing asked unkept");
}

#[tokio::test(start_paused = true)]
async fn a_guest_is_asked_for_accessibility_once_and_its_page_opened_once() {
    let (_folder, record) = record();
    let first = Fake::recording();

    let ended = watched_for(&first, &record, LOOK * (SETTINGS_AFTER + 1)).await;
    // A later daemon of the same guest, still not allowed it.
    let later = Fake::recording();
    let again = watched_for(&later, &record, LOOK).await;

    assert!(ended, "it ends once its page is opened");
    assert_eq!(first.did(), ["request", "settings"]);
    assert!(again, "it ends at once");
    assert!(
        later.did().is_empty(),
        "never asked again: {:?}",
        later.did()
    );
}

#[tokio::test(start_paused = true)]
async fn a_guest_allowed_accessibility_is_never_asked() {
    let (_folder, record) = record();
    let guest = Fake::recording();
    guest.trusted.store(true, SeqCst);
    let before = Fake::default();
    before.trusted.store(true, SeqCst);
    let asking = Fake::recording();

    let ended = watched_for(&guest, &record, Duration::from_secs(1)).await;
    let waited = watched_for(&before, &record, Duration::from_secs(1)).await;
    let (_other, allowing) = self::record();
    let watching = watch(&asking, &allowing);
    tokio::pin!(watching);
    let first = tokio::time::timeout(LOOK, &mut watching).await;
    asking.trusted.store(true, SeqCst);
    let allowed = tokio::time::timeout(LOOK * 2, &mut watching).await;

    assert!(ended && waited);
    assert!(guest.did().is_empty() && before.did().is_empty());
    assert!(!record.asked(), "nothing kept for a guest never asked");
    assert!(first.is_err() && allowed.is_ok(), "it ends once allowed");
    assert_eq!(asking.did(), ["request"], "no page once allowed");
}
