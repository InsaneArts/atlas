use super::*;
use std::sync::Arc;

/// A feed kept in a folder of its own.
async fn opened() -> (Arc<Feed>, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let feed = Arc::new(Feed::default());
    let (opening, events) = (feed.clone(), folder.path().join("events"));
    tokio::task::spawn_blocking(move || opening.open(&events))
        .await
        .unwrap()
        .unwrap();
    (feed, folder)
}

/// Write an event of `kind` the way the producer does, off the runtime.
async fn append(feed: &Arc<Feed>, kind: Kind) -> Result<()> {
    let feed = feed.clone();
    tokio::task::spawn_blocking(move || feed.append("d", kind))
        .await
        .unwrap()
}

/// The feed's identity and head.
fn position(feed: &Feed) -> (String, u64) {
    match feed.position.borrow().clone() {
        Position::At(identity, head) => (identity, head),
        other => panic!("not available: {other:?}"),
    }
}

#[tokio::test]
async fn a_reader_naming_no_feed_is_told_where_it_stands() {
    let (feed, _folder) = opened().await;
    let (identity, head) = position(&feed);

    let (code, body) = feed.answer(None, 0).await;

    assert_eq!(code, 200);
    assert_eq!(body["reset"], json!({"feed": identity, "head": head}));
}

#[tokio::test(start_paused = true)]
async fn a_reader_with_nothing_new_waits_for_the_next_event() {
    let (feed, _folder) = opened().await;
    let (identity, head) = position(&feed);

    let (answered, ()) = tokio::join!(feed.answer(Some(&identity), head), async {
        tokio::time::sleep(Duration::from_secs(3)).await;
        append(&feed, Kind::Asks).await.unwrap();
    });

    assert_eq!(answered.0, 200);
    let events = answered.1["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "asks");
    assert_eq!(answered.1["head"], head + 1);
}

#[tokio::test(start_paused = true)]
async fn a_wait_ends_with_nothing_after_a_while() {
    let (feed, _folder) = opened().await;
    let (identity, head) = position(&feed);
    let start = tokio::time::Instant::now();

    let (code, body) = feed.answer(Some(&identity), head).await;

    assert_eq!(code, 200);
    assert_eq!(body["events"], json!([]));
    assert_eq!(body["feed"], identity);
    assert_eq!(start.elapsed(), WAIT);
}

#[tokio::test(start_paused = true)]
async fn readers_past_the_waits_are_told_to_poll() {
    let (feed, _folder) = opened().await;
    let (identity, head) = position(&feed);
    let held: Vec<_> = (0..WAITS)
        .map(|_| feed.waits.try_acquire().unwrap())
        .collect();

    let (code, body) = feed.answer(Some(&identity), head).await;

    assert_eq!(code, 200);
    assert_eq!(body["poll"], true);
    assert_eq!(body["events"], json!([]));
    drop(held);
}

#[tokio::test(start_paused = true)]
async fn a_feed_that_cannot_be_written_answers_its_waiting_readers_at_once() {
    let (feed, folder) = opened().await;
    let (identity, head) = position(&feed);
    let start = tokio::time::Instant::now();

    let (answered, ()) = tokio::join!(feed.answer(Some(&identity), head), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        // The feed's folder is gone: the next segment cannot be made.
        std::fs::remove_dir_all(folder.path().join("events")).unwrap();
        assert!(append(&feed, Kind::Status).await.is_err());
    });

    assert_eq!(answered, (503, json!({"ok": false, "unavailable": true})));
    assert!(start.elapsed() < WAIT);
    assert!(!feed.available());
}

#[tokio::test(start_paused = true)]
async fn a_service_ending_answers_its_waiting_readers() {
    let (feed, _folder) = opened().await;
    let (identity, head) = position(&feed);

    let (answered, ()) = tokio::join!(feed.answer(Some(&identity), head), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        feed.close();
    });

    assert_eq!(answered.0, 503);
}

#[tokio::test]
async fn a_closed_feed_is_never_begun_again() {
    let (feed, folder) = opened().await;
    feed.close();
    let (beginning, events) = (feed.clone(), folder.path().join("events"));

    let begun = tokio::task::spawn_blocking(move || beginning.begin(&events))
        .await
        .unwrap();

    assert!(begun.is_err());
    assert!(append(&feed, Kind::Status).await.is_err());
    assert_eq!(feed.answer(None, 0).await.0, 503);
}
