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
    let desktop = viewer.client().desktop().id().as_str().to_owned();

    producer
        .observe(&viewer, &desktop, reading(now, "ready", false))
        .await;
    assert_eq!(kinds_after(&viewer, start).await, ["status", "control"]);

    let first = head(&viewer);
    producer
        .observe(&viewer, &desktop, reading(now, "ready", false))
        .await;
    assert_eq!(head(&viewer), first, "the same reading says nothing");

    producer
        .observe(&viewer, &desktop, reading(now + TICK, "ready", true))
        .await;
    assert_eq!(kinds_after(&viewer, first).await, ["control"]);

    let second = head(&viewer);
    producer
        .observe(&viewer, &desktop, reading(now, "working", false))
        .await;
    assert_eq!(head(&viewer), second, "an older look says nothing");
}

#[tokio::test]
async fn a_read_of_a_replaced_helper_or_an_older_look_is_not_handed_on() {
    let (viewer, _dir) = viewer();
    let served = viewer.client();
    let now = Instant::now();
    let desktop = served.desktop().id().as_str().to_owned();
    let newest = |viewer: &Viewer| viewer.observed.borrow().get(&desktop).map(|seen| seen.at);

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

/// What the feed of `viewer` told after `after`: each event's desktop and
/// kind, in order.
async fn told_after(viewer: &Viewer, after: u64) -> Vec<(String, String)> {
    let (feed, _) = viewer.feed.head().unwrap();
    let (_, body) = viewer.feed.answer(Some(&feed), after).await;
    body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| {
            (
                event["desktop"].as_str().unwrap().to_owned(),
                event["kind"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_desktop_not_viewed_tells_the_feed_of_its_questions_status_and_activity_and_a_restart_goes_on()
 {
    use crate::viewer::{
        http::bind,
        two_desktops::{Two, read},
    };
    let two = Two::new().await;
    let viewer = &two.viewer;
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));
    let mut producer = Producer::default();
    producer.tick(viewer).await;
    let folder = super::super::station::service_folder(&two.home).join(FOLDER);
    assert!(folder.is_dir(), "the feed is the station's");
    let told = told_after(viewer, 0).await;
    for kind in [
        "activity", "sessions", "asks", "control", "status", "desktops",
    ] {
        assert!(
            told.contains(&(main.clone(), kind.to_owned())),
            "{kind}: {told:?}"
        );
    }
    for kind in ["activity", "sessions", "asks", "control"] {
        assert!(
            told.contains(&(work.clone(), kind.to_owned())),
            "{kind}: {told:?}"
        );
    }

    let start = head(viewer);
    let records = two.work.records().to_owned();
    std::fs::create_dir_all(&records).unwrap();
    std::fs::write(
        records.join(atlas_client::activity_log::FILE),
        "{\"at\":\"1\",\"operation\":\"input\",\"ok\":true}\n",
    )
    .unwrap();
    producer.tick(viewer).await;
    assert_eq!(
        told_after(viewer, start).await,
        [(work.clone(), "activity".to_owned())]
    );

    let asked = head(viewer);
    let asks = two.work.controller.asks().folder().to_owned();
    std::fs::create_dir_all(&asks).unwrap();
    std::fs::write(asks.join("question.json"), "{}").unwrap();
    producer.tick(viewer).await;
    assert_eq!(
        told_after(viewer, asked).await,
        [(work.clone(), "asks".to_owned())]
    );

    let read_at = head(viewer);
    let (status, said) = read(viewer, &format!("status.json?desktop={work}"), true).await;
    assert_eq!(status, 200, "{said}");
    let reading = viewer.observed.borrow().get(&work).cloned().unwrap();
    producer.observe(viewer, &work, reading).await;
    assert_eq!(
        told_after(viewer, read_at).await,
        [
            (work.clone(), "status".to_owned()),
            (work.clone(), "control".to_owned())
        ]
    );

    // The service started again goes on with the same feed, from where it
    // was, and tells every kind of each desktop once more.
    let (feed, before) = viewer.feed.head().unwrap();
    let (_, again) = bind(two.main.clone(), 0, Some(&two.settings))
        .await
        .unwrap();
    unpoisoned(&again.stand_ins).extend([two.main.clone(), two.work.clone()]);
    *unpoisoned(&again.station_home) = Some(two.home.clone());
    again.follow_settings().await;
    Producer::default().tick(&again).await;
    let (continued, after) = again.feed.head().unwrap();
    assert_eq!(continued, feed, "the reader's place holds");
    assert!(after > before);
    let told = told_after(&again, before).await;
    assert!(
        told.contains(&(work.clone(), "activity".to_owned())),
        "{told:?}"
    );
    assert!(told.contains(&(main, "activity".to_owned())), "{told:?}");
}
