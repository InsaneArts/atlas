use super::*;
use tokio::io::duplex;

#[tokio::test]
async fn a_relay_dropped_partway_gives_its_credit_back() {
    let credit = Credit::default();
    let outbox = Outbox::default();
    let (local, mut client) = duplex(1 << 20);
    // The lane takes nothing more once its buffer is full.
    let (to_lane, _lane_far) = duplex(1024);
    let (_lane_near, from_lane) = duplex(1024);
    client.write_all(&[7; CHUNK]).await.unwrap();

    let carried = carry(local, from_lane, to_lane, LaneId(4), &credit, &outbox);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), carried).await;

    assert_eq!(credit.left(), crate::credit::BULK as usize);
}

#[test]
fn a_list_is_takeable_only_within_its_bound_without_port_zero_or_a_repeat() {
    let at = |port| Endpoint {
        family: Family::V4,
        port,
        incarnation: 1,
    };

    assert!(takeable(&[at(80), at(81)]));
    assert!(!takeable(&[at(0)]));
    assert!(!takeable(&[at(80), at(80)]));
    assert!(!takeable(&(1..=33).map(at).collect::<Vec<_>>()));
}
