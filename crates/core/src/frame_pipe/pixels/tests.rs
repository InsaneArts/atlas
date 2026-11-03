use super::*;
use std::sync::Arc;

#[test]
fn pixels_keep_their_reservation_until_the_last_share_drops() {
    let budget = Budget::new(1024);
    let mut pixels = FramePixels::reserve(&budget, 4, 2).unwrap();
    assert_eq!(budget.reserved(), 24);
    pixels.as_mut_slice().fill(1);
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
fn a_picture_the_budget_cannot_hold_is_refused_before_it_is_allocated() {
    let budget = Budget::new(24);
    let held = FramePixels::reserve(&budget, 4, 2).unwrap();
    assert_eq!(
        FramePixels::reserve(&budget, 1, 1).unwrap_err(),
        Reason::BudgetExhausted
    );
    drop(held);
    assert_eq!(
        FramePixels::reserve(&budget, 3, 3).unwrap_err(),
        Reason::BudgetExhausted
    );
    assert_eq!(budget.reserved(), 0, "nothing is partly held");
}

#[test]
fn sides_stay_within_one_to_4096() {
    let budget = Budget::new(u64::MAX);
    for (width, height) in [(0, 2), (2, 0), (4097, 1), (1, 4097)] {
        assert_eq!(
            FramePixels::reserve(&budget, width, height).unwrap_err(),
            Reason::WrongGeometry
        );
    }
    assert_eq!(budget.reserved(), 0);
    let largest = FramePixels::reserve(&budget, 4096, 1).unwrap();
    assert_eq!(largest.as_slice().len(), 4096 * 3);
}
