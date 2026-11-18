use super::*;
use std::time::Duration;

fn hold() -> Hold {
    Hold {
        change: Some(Token::new()),
        name: Some("work".to_owned()),
    }
}

#[tokio::test]
async fn a_hold_tells_every_open_lane_to_stop_and_admits_none_until_released() {
    let lanes = Lanes::default();
    let read = lanes.admit(Work::Read).unwrap();
    let picture = lanes.admit(Work::Picture).unwrap();
    let other = lanes.admit(Work::Picture).unwrap();
    let held = hold();

    let told = lanes.hold(held.clone());

    assert_eq!(
        told,
        BTreeMap::from([(Work::Read, 1), (Work::Picture, 2)]),
        "every open lane by its work"
    );
    for ticket in [&read, &picture, &other] {
        tokio::time::timeout(Duration::from_secs(5), ticket.stopped())
            .await
            .expect("told to stop");
    }
    assert_eq!(lanes.admit(Work::Change).err(), Some(held.clone()));
    assert_eq!(lanes.held(), Some(held));
    lanes.release();
    assert!(lanes.admit(Work::Change).is_ok(), "admitted once released");
}

#[tokio::test]
async fn a_lane_admitted_while_nothing_holds_is_never_told_to_stop() {
    let lanes = Lanes::default();
    let ticket = lanes.admit(Work::Stage).unwrap();

    let stopped = tokio::time::timeout(Duration::from_millis(200), ticket.stopped()).await;

    assert!(stopped.is_err(), "it carries on");
    assert_eq!(lanes.held(), None);
}

#[tokio::test]
async fn a_hold_is_drained_only_once_every_ticket_is_gone() {
    let lanes = std::sync::Arc::new(Lanes::default());
    let first = lanes.admit(Work::Read).unwrap();
    let second = lanes.admit(Work::Change).unwrap();
    lanes.hold(hold());

    let still = tokio::time::timeout(Duration::from_millis(200), lanes.drained()).await;
    drop(first);
    let one_left = tokio::time::timeout(Duration::from_millis(200), lanes.drained()).await;
    drop(second);
    let drained = tokio::time::timeout(Duration::from_secs(5), lanes.drained()).await;

    assert!(still.is_err(), "two lanes open");
    assert!(one_left.is_err(), "one lane open");
    assert!(drained.is_ok(), "none open");
    assert_eq!(lanes.open_now(), 0);
}

#[tokio::test]
async fn a_hold_with_no_lane_open_is_drained_at_once() {
    let lanes = Lanes::default();

    let told = lanes.hold(hold());

    assert!(told.is_empty());
    tokio::time::timeout(Duration::from_secs(1), lanes.drained())
        .await
        .expect("nothing to wait for");
}
