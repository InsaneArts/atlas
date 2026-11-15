use super::*;
use atlas_session::{authority::Retired, clock::Continuous, generation::Generation};

fn holding() -> Holding {
    let authority = Authority::new(Generation(1), Arc::new(Continuous), Box::new(|_, _| {}));
    Holding {
        lanes: Arc::new(Lanes::new(authority.clone())),
        authority,
        lane: LaneId(7),
    }
}

#[test]
fn an_update_is_handed_over_only_while_its_lane_and_session_are_live() {
    assert!(holding().live());

    let cancelled = holding();
    cancelled.lanes.cancel(cancelled.lane);
    assert!(!cancelled.live(), "handed over from a cancelled lane");

    let ended = holding();
    ended.authority.retire(Retired::Revoked);
    assert!(!ended.live(), "handed over once its session ended");
}
