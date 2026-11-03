use super::*;
use atlas_core::frame_pipe::model::{Hex32, Hex64};

fn id(value: u64) -> Positive {
    Positive::new(value).unwrap()
}

fn picture(image: u64) -> Ref {
    Ref {
        service_instance: Hex32::parse(&"a".repeat(32)).unwrap(),
        observer_id: Hex32::parse(&"b".repeat(32)).unwrap(),
        observer_generation: id(1),
        stream_generation: id(1),
        geometry_generation: id(1),
        source_sequence: id(image),
        image_sequence: id(image),
        frame_key: Hex64::random(),
        view_generation: id(1),
    }
}

fn subscribed() -> Subscription {
    let mut subscription = Subscription::default();
    subscription.subscribe().unwrap();
    subscription
}

/// Serve and deliver `credit` with `reference` at `now`.
fn deliver(subscription: &mut Subscription, credit: u64, reference: &Ref, now: Instant) {
    subscription.credit(id(credit), now).unwrap();
    assert_eq!(
        subscription.select(reference.clone(), now),
        Some(id(credit))
    );
    subscription.delivered(id(credit), now);
}

#[test]
fn only_a_committed_picture_becomes_the_base() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    deliver(&mut subscription, 1, &first, now);
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Decoded(first.clone()), now),
        Ok(Step::Done)
    );
    assert_eq!(subscription.base(), None, "decoding is provisional");
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Commit(first.clone()), now),
        Ok(Step::Done)
    );
    assert_eq!(subscription.base(), Some(&first));

    let second = picture(2);
    deliver(&mut subscription, 2, &second, now);
    subscription
        .acknowledge(id(2), Ack::Decoded(second.clone()), now)
        .unwrap();
    subscription.acknowledge(id(2), Ack::Drop, now).unwrap();
    assert_eq!(subscription.base(), Some(&first), "a drop keeps the base");

    deliver(&mut subscription, 3, &second, now);
    subscription.acknowledge(id(3), Ack::Resync, now).unwrap();
    assert_eq!(subscription.base(), None, "a resync forgets it");
}

#[test]
fn one_credit_at_a_time_and_each_newer_than_the_last() {
    let mut subscription = Subscription::default();
    let now = Instant::now();
    assert_eq!(
        subscription.credit(id(1), now),
        Err(Reason::InvalidSchema),
        "nothing before subscribing"
    );
    subscription.subscribe().unwrap();
    assert_eq!(subscription.subscribe(), Err(Reason::InvalidSchema));
    subscription.credit(id(5), now).unwrap();
    assert_eq!(subscription.credit(id(6), now), Err(Reason::QueueOverflow));
    assert!(subscription.retire(id(5)));
    assert!(!subscription.retire(id(5)));
    assert_eq!(subscription.credit(id(5), now), Err(Reason::InvalidSchema));
    assert_eq!(subscription.credit(id(4), now), Err(Reason::InvalidSchema));
    assert!(subscription.credit(id(6), now).is_ok());
}

#[test]
fn an_unserved_credit_expires_and_a_new_one_may_follow() {
    let mut subscription = subscribed();
    let now = Instant::now();
    subscription.credit(id(1), now).unwrap();
    let almost = now + CREDIT_LIFE - Duration::from_millis(1);
    assert_eq!(subscription.lapse(almost), Ok(None));
    assert_eq!(
        subscription.select(picture(1), now + CREDIT_LIFE),
        None,
        "an expired credit is never served"
    );
    assert_eq!(subscription.lapse(now + CREDIT_LIFE), Ok(Some(id(1))));
    assert_eq!(subscription.lapse(now + CREDIT_LIFE), Ok(None));
    assert!(subscription.credit(id(2), now + CREDIT_LIFE).is_ok());
}

#[test]
fn a_delivery_left_uncommitted_ends_the_pair() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    deliver(&mut subscription, 1, &first, now);
    assert_eq!(subscription.lapse(now + CREDIT_LIFE), Ok(None), "served");
    subscription
        .acknowledge(id(1), Ack::Decoded(first), now)
        .unwrap();
    assert_eq!(
        subscription.lapse(now + COMMIT_LIFE - Duration::from_millis(1)),
        Ok(None)
    );
    assert_eq!(
        subscription.lapse(now + COMMIT_LIFE),
        Err(Reason::ClientStale)
    );
}

#[test]
fn decoded_may_arrive_before_the_frame_lane_finishes() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    subscription.credit(id(1), now).unwrap();
    subscription.select(first.clone(), now).unwrap();
    let later = now + Duration::from_millis(100);
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Decoded(first.clone()), later),
        Ok(Step::Done)
    );
    subscription.delivered(id(1), later);
    assert_eq!(
        subscription.lapse(later + COMMIT_LIFE),
        Err(Reason::ClientStale),
        "its commit life runs from the decode"
    );
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Commit(first.clone()), later),
        Ok(Step::Done)
    );
    assert_eq!(subscription.base(), Some(&first));
}

#[test]
fn repeats_answer_without_effect_and_conflicts_end_the_pair() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    deliver(&mut subscription, 1, &first, now);
    let decoded = Ack::Decoded(first.clone());
    assert_eq!(
        subscription.acknowledge(id(1), decoded.clone(), now),
        Ok(Step::Done)
    );
    assert_eq!(
        subscription.acknowledge(id(1), decoded.clone(), now),
        Ok(Step::Repeat)
    );
    subscription
        .acknowledge(id(1), Ack::Commit(first.clone()), now)
        .unwrap();
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Commit(first.clone()), now),
        Ok(Step::Repeat)
    );
    assert_eq!(
        subscription.acknowledge(id(1), decoded, now),
        Ok(Step::Repeat),
        "a late identical decode"
    );
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Drop, now),
        Err(Reason::WrongBase),
        "a settled credit cannot be settled another way"
    );
    assert_eq!(
        subscription.acknowledge(id(9), Ack::Drop, now),
        Err(Reason::WrongBase),
        "nor an unknown one"
    );
}

#[test]
fn acknowledgements_name_the_picture_sent_in_order() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    subscription.credit(id(1), now).unwrap();
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Drop, now),
        Err(Reason::InvalidSchema),
        "nothing was sent yet"
    );
    subscription.select(first.clone(), now).unwrap();
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Commit(first.clone()), now),
        Err(Reason::InvalidSchema),
        "a commit follows a decode"
    );
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Decoded(picture(2)), now),
        Err(Reason::WrongBase)
    );
    subscription
        .acknowledge(id(1), Ack::Decoded(first), now)
        .unwrap();
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Commit(picture(2)), now),
        Err(Reason::WrongBase)
    );
    assert_eq!(subscription.base(), None);
}

#[test]
fn only_the_latest_acknowledgements_are_remembered() {
    let mut subscription = subscribed();
    let now = Instant::now();
    for credit in 1..=(REMEMBERED as u64 + 1) {
        deliver(&mut subscription, credit, &picture(credit), now);
        subscription
            .acknowledge(id(credit), Ack::Drop, now)
            .unwrap();
    }
    assert_eq!(subscription.acknowledged.len(), REMEMBERED);
    assert_eq!(
        subscription.acknowledge(id(1), Ack::Drop, now),
        Err(Reason::WrongBase),
        "a forgotten repeat is not answered"
    );
    assert_eq!(
        subscription.acknowledge(id(2), Ack::Drop, now),
        Ok(Step::Repeat)
    );
}
