use super::*;

/// A folder of its own, private as the feed's must be.
fn private() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().unwrap();
    fs::set_permissions(folder.path(), fs::Permissions::from_mode(0o700)).unwrap();
    folder
}

fn opened(folder: &Path) -> EventLog {
    EventLog::open(folder).unwrap()
}

fn seqs(since: Since) -> Vec<u64> {
    match since {
        Since::Events { events, .. } => events.iter().map(|event| event.seq).collect(),
        Since::Reset { .. } => panic!("a reset"),
    }
}

#[test]
fn events_are_numbered_in_order_and_read_after_the_last_one_seen() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    log.append("d", Kind::Asks).unwrap();
    log.append("d", Kind::Status).unwrap();
    let feed = log.position().0.to_owned();

    assert_eq!(seqs(log.since(&feed, 0)), [1, 2, 3]);
    assert_eq!(seqs(log.since(&feed, 2)), [3]);
    assert_eq!(seqs(log.since(&feed, 3)), Vec::<u64>::new());
}

#[test]
fn a_reader_of_another_feed_or_beyond_the_head_is_told_to_start_again() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    let feed = log.position().0.to_owned();

    let reset = Since::Reset {
        feed: feed.clone(),
        head: 1,
    };
    assert_eq!(log.since("another", 0), reset);
    assert_eq!(log.since(&feed, 9), reset);
}

#[test]
fn the_feed_goes_on_from_disk_after_the_service_starts_again() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    log.append("d", Kind::Sessions).unwrap();
    let feed = log.position().0.to_owned();
    drop(log);

    let mut again = opened(folder.path());
    assert_eq!(again.position(), (feed.as_str(), 2));
    assert_eq!(again.append("d", Kind::Asks).unwrap().seq, 3);
    assert_eq!(seqs(again.since(&feed, 1)), [2, 3]);
}

#[test]
fn a_torn_last_record_is_cut_off_and_the_feed_goes_on() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    let feed = log.position().0.to_owned();
    drop(log);
    let (_, path) = segments(folder.path()).unwrap().pop().unwrap();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(br#"{"feed":"x","seq":2,"at""#).unwrap();

    let mut again = opened(folder.path());
    assert_eq!(again.position(), (feed.as_str(), 1));
    assert_eq!(again.append("d", Kind::Asks).unwrap().seq, 2);
}

#[test]
fn a_feed_that_cannot_be_shown_continuous_is_begun_again() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    log.append("d", Kind::Activity).unwrap();
    let feed = log.position().0.to_owned();
    drop(log);
    let (_, path) = segments(folder.path()).unwrap().pop().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let damaged = text.replacen("\"seq\":1", "\"seq\":5", 1);
    fs::write(&path, damaged).unwrap();

    let again = opened(folder.path());
    assert_ne!(again.position().0, feed);
    assert_eq!(again.position().1, 0);
    assert!(matches!(again.since(&feed, 2), Since::Reset { .. }));
}

#[test]
fn damage_before_the_last_record_is_never_taken_for_a_torn_tail() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    log.append("d", Kind::Activity).unwrap();
    let feed = log.position().0.to_owned();
    drop(log);
    let (_, path) = segments(folder.path()).unwrap().pop().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("not a record\n{text}")).unwrap();

    assert_ne!(opened(folder.path()).position().0, feed);
}

#[test]
fn segments_turn_over_and_two_are_kept() {
    let folder = private();
    let mut log = opened(folder.path());
    for _ in 0..(SEGMENT * 3 + 1) {
        log.append("d", Kind::Status).unwrap();
    }
    let kept = segments(folder.path()).unwrap();
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].0, SEGMENT * 2 + 1);
    let feed = log.position().0.to_owned();
    drop(log);

    let again = opened(folder.path());
    assert_eq!(again.position(), (feed.as_str(), SEGMENT * 3 + 1));
    assert!(
        matches!(again.since(&feed, 0), Since::Reset { .. }),
        "older than kept"
    );
    assert_eq!(seqs(again.since(&feed, SEGMENT * 3)), [SEGMENT * 3 + 1]);
}

#[test]
fn an_answer_holds_at_most_one_batch() {
    let folder = private();
    let mut log = opened(folder.path());
    for _ in 0..(BATCH + 10) {
        log.append("d", Kind::Activity).unwrap();
    }
    let feed = log.position().0.to_owned();
    assert_eq!(seqs(log.since(&feed, 0)).len(), BATCH);
}

#[test]
fn an_event_says_only_what_to_read_again() {
    let folder = private();
    let mut log = opened(folder.path());
    let event = log.append("d", Kind::Asks).unwrap();
    let written = serde_json::to_value(&event).unwrap();
    let mut fields: Vec<&str> = written
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["at", "desktop", "feed", "kind", "seq"]);
}

#[test]
fn a_whole_last_record_that_does_not_read_begins_the_feed_again() {
    let folder = private();
    let mut log = opened(folder.path());
    log.append("d", Kind::Activity).unwrap();
    let feed = log.position().0.to_owned();
    drop(log);
    let (_, path) = segments(folder.path()).unwrap().pop().unwrap();
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"feed\":\"x\",\"seq\":2}\n").unwrap();

    assert_ne!(opened(folder.path()).position().0, feed);
}

#[test]
fn a_reader_is_told_the_same_before_the_service_starts_again_as_after() {
    let folder = private();
    let mut log = opened(folder.path());
    for _ in 0..(SEGMENT * 2 + 1) {
        log.append("d", Kind::Status).unwrap();
    }
    let feed = log.position().0.to_owned();
    let before = log.since(&feed, 1);
    drop(log);

    assert_eq!(opened(folder.path()).since(&feed, 1), before);
    assert!(
        matches!(before, Since::Reset { .. }),
        "event 2 is no longer kept"
    );
}

#[test]
fn a_feed_whose_folder_went_away_begins_again_in_a_new_one() {
    let folder = private();
    let events = folder.path().join("events");
    drop(opened(&events));
    fs::remove_dir_all(&events).unwrap();

    let mut log = EventLog::begin(&events).unwrap();
    assert_eq!(log.append("d", Kind::Activity).unwrap().seq, 1);
}
