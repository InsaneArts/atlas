use super::*;
use crate::frame_pipe::budget::Budget;
use std::sync::Arc;

#[test]
fn pixels_keep_their_reservation_until_the_last_share_drops() {
    let budget = Budget::new(1024);
    let pixels = FramePixels::new(
        Buffer::copy_from(&[1; 24]).unwrap(),
        4,
        2,
        budget.reserve(24).unwrap(),
    )
    .unwrap();
    let shared = Arc::new(pixels);
    let reader = shared.clone();
    drop(shared);
    assert_eq!(budget.reserved(), 24);
    assert_eq!(reader.as_slice(), &[1; 24]);
    assert_eq!((reader.width(), reader.height()), (4, 2));
    drop(reader);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn pixels_must_be_exactly_their_picture_and_fully_paid_for() {
    let budget = Budget::new(1024);
    let make = |len: usize, width: u32, height: u32, paid: u64| {
        FramePixels::new(
            Buffer::zeroed(len).unwrap(),
            width,
            height,
            budget.reserve(paid).unwrap(),
        )
    };
    assert!(make(24, 4, 2, 24).is_ok());
    assert_eq!(make(23, 4, 2, 24).unwrap_err(), Refusal::Invalid);
    assert_eq!(make(24, 4, 2, 23).unwrap_err(), Refusal::Invalid);
    assert_eq!(make(0, 0, 2, 0).unwrap_err(), Refusal::Invalid);
    assert_eq!(
        budget.reserved(),
        0,
        "refused pixels give their reservation back"
    );
}
