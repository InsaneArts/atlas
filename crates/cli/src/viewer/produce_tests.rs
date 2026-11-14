use super::*;
use crate::viewer::http::configured;
use serde_json::json;

/// A viewer of its own, its feed not opened yet.
fn viewer() -> (Arc<Viewer>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client_in(dir.path());
    std::fs::create_dir_all(client.output_dir().unwrap()).unwrap();
    (configured(client, "127.0.0.1:0".to_owned(), None), dir)
}

/// The kinds of the events after `after`, in order.
async fn kinds_after(viewer: &Viewer, after: u64) -> Vec<String> {
    let (feed, _) = viewer.feed.head().unwrap();
    let (_, body) = viewer.feed.answer(Some(&feed), after).await;
    body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["kind"].as_str().unwrap().to_owned())
        .collect()
}

fn head(viewer: &Viewer) -> u64 {
    viewer.feed.head().unwrap().1
}

fn reading(at: Instant, state: &str, active: bool) -> Observed {
    Observed {
        at,
        status: json!({"overview": {"state": state}}),
        control: json!({"active": active}),
    }
}

#[tokio::test]
async fn a_new_feed_tells_every_kind_once_so_readers_reload() {
    let (viewer, _dir) = viewer();
    let mut producer = Producer::default();

    producer.tick(&viewer).await;

    let mut told = kinds_after(&viewer, 0).await;
    told.sort();
    assert_eq!(
        told,
        [
            "activity", "asks", "control", "desktops", "sessions", "status"
        ]
    );
}

#[tokio::test]
async fn a_change_to_a_store_is_told_once() {
    let (viewer, _dir) = viewer();
    let mut producer = Producer::default();
    producer.tick(&viewer).await;
    let before = head(&viewer);
    producer.tick(&viewer).await;
    assert_eq!(head(&viewer), before, "nothing changed");

    let log = viewer
        .client()
        .output_dir()
        .unwrap()
        .join(atlas_client::activity_log::FILE);
    std::fs::write(&log, "{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n").unwrap();
    producer.tick(&viewer).await;
    producer.tick(&viewer).await;

    assert_eq!(kinds_after(&viewer, before).await, ["activity"]);
}

#[tokio::test]
async fn a_change_the_feed_could_not_keep_is_told_once_it_can() {
    let (viewer, _dir) = viewer();
    let mut producer = Producer::default();
    producer.tick(&viewer).await;
    let output = viewer.client().output_dir().unwrap().to_owned();

    // The feed cannot be written: the change is kept for later.
    let unwritable = viewer.clone();
    tokio::task::spawn_blocking(move || {
        let _ = unwritable.feed.begin(Path::new("/dev/null/events"));
    })
    .await
    .unwrap();
    std::fs::write(output.join(atlas_client::activity_log::FILE), "{}\n").unwrap();
    producer.tick(&viewer).await;
    assert!(!viewer.feed.available());

    // The next slow look begins a feed again, and tells it.
    producer.slow_due = Some(Instant::now());
    producer.tick(&viewer).await;
    assert!(
        kinds_after(&viewer, 0)
            .await
            .contains(&"activity".to_owned())
    );
}

#[tokio::test]
async fn a_status_read_is_told_as_what_changed_and_an_older_look_says_nothing() {
    let (viewer, _dir) = viewer();
    let mut producer = Producer::default();
    producer.tick(&viewer).await;
    let start = head(&viewer);
    let now = Instant::now();

    producer
        .observe(&viewer, reading(now, "ready", false))
        .await;
    assert_eq!(kinds_after(&viewer, start).await, ["status", "control"]);

    let first = head(&viewer);
    producer
        .observe(&viewer, reading(now, "ready", false))
        .await;
    assert_eq!(head(&viewer), first, "the same reading says nothing");

    producer
        .observe(&viewer, reading(now + TICK, "ready", true))
        .await;
    assert_eq!(kinds_after(&viewer, first).await, ["control"]);

    let second = head(&viewer);
    producer
        .observe(&viewer, reading(now, "working", false))
        .await;
    assert_eq!(head(&viewer), second, "an older look says nothing");
}

#[tokio::test]
async fn a_read_of_a_replaced_helper_or_an_older_look_is_not_handed_on() {
    let (viewer, _dir) = viewer();
    let served = viewer.client();
    let now = Instant::now();
    let newest = |viewer: &Viewer| viewer.observed.borrow().as_ref().map(|seen| seen.at);

    viewer.tell_producer(&served, now, &json!({"overview": {}}), false);
    assert_eq!(newest(&viewer), Some(now));

    viewer.tell_producer(&served, now - TICK, &json!({"overview": {}}), true);
    assert_eq!(newest(&viewer), Some(now), "an older look");

    let replaced = Arc::new((*served).clone());
    viewer.tell_producer(&replaced, now + TICK, &json!({"overview": {}}), true);
    assert_eq!(newest(&viewer), Some(now), "a helper no longer served");
}

#[tokio::test]
async fn a_change_to_control_makes_the_next_read_look_at_the_helper_again() {
    let (viewer, _dir) = viewer();
    let mut producer = Producer::default();
    producer.tick(&viewer).await;
    let forgotten = || {
        viewer
            .looks_forgotten
            .load(std::sync::atomic::Ordering::SeqCst)
    };
    let before = forgotten();

    let client = viewer.client();
    let folder = client.controller.holder_path().parent().unwrap().to_owned();
    std::fs::create_dir_all(&folder).unwrap();
    let name = client.desktop().id().as_str().to_owned();
    std::fs::write(folder.join(format!("{name}.changed")), "{}").unwrap();
    producer.tick(&viewer).await;

    assert!(forgotten() > before);
}
