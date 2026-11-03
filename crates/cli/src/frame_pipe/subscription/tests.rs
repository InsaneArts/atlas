use super::*;
use atlas_core::frame_pipe::model::{Hex32, Hex64};

const MS: Duration = Duration::from_millis(1);

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

fn dropped() -> Ack {
    Ack::Drop(DropReason::Hidden, None)
}

#[test]
fn the_contract_limits_are_the_documented_ones() {
    assert_eq!(CREDIT_LIFE, 200 * MS);
    assert_eq!(COMMIT_LIFE, 500 * MS);
    assert_eq!(REMEMBERED, 64);
}

#[test]
fn only_a_committed_picture_becomes_the_base() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    deliver(&mut subscription, 1, &first, now);
    assert_eq!(
        subscription.acknowledge(1, id(1), Ack::Decoded(first.clone()), now),
        Ok(Step::Done)
    );
    assert_eq!(subscription.base(), None, "decoding is provisional");
    assert_eq!(
        subscription.acknowledge(2, id(1), Ack::Commit(first.clone()), now),
        Ok(Step::Done)
    );
    assert_eq!(subscription.base(), Some(&first));

    let second = picture(2);
    deliver(&mut subscription, 2, &second, now);
    subscription
        .acknowledge(3, id(2), Ack::Decoded(second.clone()), now)
        .unwrap();
    subscription
        .acknowledge(
            4,
            id(2),
            Ack::Drop(DropReason::Stale, Some(second.clone())),
            now,
        )
        .unwrap();
    assert_eq!(subscription.base(), Some(&first), "a drop keeps the base");

    deliver(&mut subscription, 3, &second, now);
    subscription
        .acknowledge(5, id(3), Ack::Resync(ResyncReason::WrongBase), now)
        .unwrap();
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
fn an_unserved_credit_expires_at_200_ms_and_a_new_one_may_follow() {
    let mut subscription = subscribed();
    let now = Instant::now();
    subscription.credit(id(1), now).unwrap();
    assert_eq!(subscription.lapse(now + 199 * MS), Ok(None));
    assert_eq!(
        subscription.select(picture(1), now + 200 * MS),
        None,
        "an expired credit is never served"
    );
    assert_eq!(subscription.lapse(now + 200 * MS), Ok(Some(id(1))));
    assert_eq!(subscription.lapse(now + 200 * MS), Ok(None));
    assert!(subscription.credit(id(2), now + 200 * MS).is_ok());
}

#[test]
fn a_delivery_left_uncommitted_for_500_ms_ends_the_pair() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    deliver(&mut subscription, 1, &first, now);
    assert_eq!(subscription.lapse(now + 200 * MS), Ok(None), "served");
    subscription
        .acknowledge(1, id(1), Ack::Decoded(first.clone()), now)
        .unwrap();
    assert_eq!(subscription.lapse(now + 499 * MS), Ok(None));
    assert_eq!(subscription.lapse(now + 500 * MS), Err(Reason::ClientStale));
    assert_eq!(
        subscription.acknowledge(2, id(1), Ack::Commit(first), now + 500 * MS),
        Err(Reason::ClientStale),
        "a late commit never becomes the base"
    );
    assert_eq!(subscription.base(), None);
}

#[test]
fn an_early_decode_leaves_the_commit_life_to_the_delivery() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    subscription.credit(id(1), now).unwrap();
    subscription.select(first.clone(), now).unwrap();
    assert_eq!(
        subscription.acknowledge(1, id(1), Ack::Decoded(first.clone()), now),
        Ok(Step::Done)
    );
    let delivered = now + 300 * MS;
    assert_eq!(
        subscription.lapse(delivered),
        Ok(None),
        "nothing runs before the frame lane finishes"
    );
    subscription.delivered(id(1), delivered);
    assert_eq!(subscription.lapse(delivered + 499 * MS), Ok(None));
    assert_eq!(
        subscription.acknowledge(2, id(1), Ack::Commit(first.clone()), delivered + 499 * MS),
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
        subscription.acknowledge(1, id(1), decoded.clone(), now),
        Ok(Step::Done)
    );
    assert_eq!(
        subscription.acknowledge(1, id(1), decoded.clone(), now),
        Ok(Step::Repeat)
    );
    assert_eq!(
        subscription.acknowledge(2, id(1), decoded.clone(), now),
        Err(Reason::InvalidSchema),
        "another request decoding again is no repeat"
    );
    subscription
        .acknowledge(3, id(1), Ack::Commit(first.clone()), now)
        .unwrap();
    assert_eq!(
        subscription.acknowledge(3, id(1), Ack::Commit(first.clone()), now),
        Ok(Step::Repeat)
    );
    assert_eq!(
        subscription.acknowledge(1, id(1), decoded, now),
        Ok(Step::Repeat),
        "a late identical decode"
    );
    assert_eq!(
        subscription.acknowledge(4, id(1), dropped(), now),
        Err(Reason::WrongBase),
        "a settled credit cannot be settled another way"
    );
    assert_eq!(
        subscription.acknowledge(5, id(9), dropped(), now),
        Err(Reason::WrongBase),
        "nor an unknown one"
    );

    deliver(&mut subscription, 2, &first, now);
    subscription.acknowledge(6, id(2), dropped(), now).unwrap();
    assert_eq!(
        subscription.acknowledge(6, id(2), Ack::Drop(DropReason::Stale, None), now),
        Err(Reason::WrongBase),
        "a changed reason is no repeat"
    );
}

#[test]
fn acknowledgements_name_the_picture_sent_in_order() {
    let mut subscription = subscribed();
    let now = Instant::now();
    let first = picture(1);
    subscription.credit(id(1), now).unwrap();
    assert_eq!(
        subscription.acknowledge(1, id(1), dropped(), now),
        Err(Reason::InvalidSchema),
        "nothing was sent yet"
    );
    subscription.select(first.clone(), now).unwrap();
    assert_eq!(
        subscription.acknowledge(2, id(1), Ack::Commit(first.clone()), now),
        Err(Reason::InvalidSchema),
        "a commit follows a decode"
    );
    assert_eq!(
        subscription.acknowledge(3, id(1), Ack::Decoded(picture(2)), now),
        Err(Reason::WrongBase)
    );
    assert_eq!(
        subscription.acknowledge(
            4,
            id(1),
            Ack::Drop(DropReason::Stale, Some(picture(2))),
            now
        ),
        Err(Reason::WrongBase)
    );
    subscription
        .acknowledge(5, id(1), Ack::Decoded(first), now)
        .unwrap();
    assert_eq!(
        subscription.acknowledge(6, id(1), Ack::Commit(picture(2)), now),
        Err(Reason::WrongBase)
    );
    assert_eq!(subscription.base(), None);
}

#[test]
fn only_the_latest_64_acknowledgements_are_remembered() {
    let mut subscription = subscribed();
    let now = Instant::now();
    for credit in 1..=65 {
        deliver(&mut subscription, credit, &picture(credit), now);
        subscription
            .acknowledge(credit, id(credit), dropped(), now)
            .unwrap();
    }
    assert_eq!(
        subscription.acknowledge(1, id(1), dropped(), now),
        Err(Reason::WrongBase),
        "a forgotten repeat is not answered"
    );
    assert_eq!(
        subscription.acknowledge(2, id(2), dropped(), now),
        Ok(Step::Repeat)
    );
}
