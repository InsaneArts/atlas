use super::*;
use crate::frame_pipe::{model::decode, wire::Refusal};
use serde_json::{Value, json};

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

fn profile() -> Value {
    json!({"streamGeneration": 1, "geometryGeneration": 1, "width": 4, "height": 2,
        "rowStride": 12, "displayWidth": 4.0, "displayHeight": 2.0, "displayScale": 1.0,
        "backend": "cg"})
}

#[test]
fn a_credit_names_a_barrier_generation_and_revision_exactly_when_its_barrier_needs_them() {
    let credit = |barrier: &str, extra: Value| {
        let mut credit = json!({"creditID": 1, "viewGeneration": 1, "streamGeneration": 1,
            "geometryGeneration": 1, "barrier": barrier});
        for (key, value) in extra.as_object().unwrap() {
            credit[key] = value.clone();
        }
        decode::<Credit>(&bytes(&credit))
    };
    assert!(credit("ordinary", json!({})).is_ok());
    assert!(credit("subscribe", json!({"barrierGeneration": 2})).is_ok());
    assert!(
        credit(
            "after-input",
            json!({"barrierGeneration": 2, "inputRevision": 0})
        )
        .is_ok()
    );
    for (barrier, extra) in [
        ("ordinary", json!({"barrierGeneration": 2})),
        ("resume", json!({})),
        ("after-input", json!({"barrierGeneration": 2})),
        (
            "reconfigure",
            json!({"barrierGeneration": 2, "inputRevision": 1}),
        ),
        ("ordinary", json!({"inputRevision": null})),
    ] {
        assert_eq!(
            credit(barrier, extra.clone()),
            Err(Refusal::Invalid),
            "{barrier} {extra}"
        );
    }
}

#[test]
fn a_state_carries_a_profile_only_when_the_profile_changed() {
    let state = |phase: &str, profile: Option<Value>| {
        let mut state = json!({"phase": phase, "streamGeneration": 1, "geometryGeneration": 1,
            "inputReady": false, "controlOwner": "self"});
        if let Some(profile) = profile {
            state["profile"] = profile;
        }
        decode::<State>(&bytes(&state))
    };
    assert_eq!(
        state("live", None).unwrap().control_owner,
        ControlOwner::Own
    );
    assert!(state("profile-changed", Some(profile())).is_ok());
    assert_eq!(state("profile-changed", None), Err(Refusal::Invalid));
    assert_eq!(state("live", Some(profile())), Err(Refusal::Invalid));
    let mut skewed = profile();
    skewed["rowStride"] = json!(13);
    assert_eq!(
        state("profile-changed", Some(skewed)),
        Err(Refusal::Invalid)
    );
}

#[test]
fn closing_is_confirmed_or_it_is_not_closed() {
    assert!(decode::<Closed>(br#"{"cleanupConfirmed":true}"#).is_ok());
    assert_eq!(
        decode::<Closed>(br#"{"cleanupConfirmed":false}"#),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        decode::<Close>(br#"{"reason":"window-closed"}"#)
            .unwrap()
            .reason,
        CloseReason::WindowClosed
    );
}

#[test]
fn subscribing_names_its_demand_and_the_answer_a_whole_profile() {
    let subscribe = decode::<Subscribe>(&bytes(&json!({"reason": "first",
        "demand": {"desiredLongest": 1600, "backend": "cg"}, "viewGeneration": 1})))
    .unwrap();
    assert_eq!(subscribe.reason, SubscribeReason::First);
    assert!(
        decode::<Subscribed>(&bytes(
            &json!({"profile": profile(), "barrierGeneration": 1})
        ))
        .is_ok()
    );
    let mut skewed = profile();
    skewed["rowStride"] = json!(11);
    assert_eq!(
        decode::<Subscribed>(&bytes(&json!({"profile": skewed, "barrierGeneration": 1}))),
        Err(Refusal::Invalid)
    );
}

#[test]
fn acknowledgements_keep_their_exact_keys() {
    let reference = json!({"serviceInstance": "a".repeat(32), "observerID": "b".repeat(32),
        "observerGeneration": 1, "streamGeneration": 1, "geometryGeneration": 1,
        "sourceSequence": 1, "imageSequence": 1, "frameKey": "c".repeat(64),
        "viewGeneration": 1});
    assert!(decode::<Settle>(&bytes(&json!({"creditID": 1, "ref": reference}))).is_ok());
    assert!(
        decode::<Dropped>(&bytes(&json!({"creditID": 1, "reason": "hidden"})))
            .unwrap()
            .reference
            .is_none()
    );
    assert_eq!(
        decode::<Dropped>(&bytes(
            &json!({"creditID": 1, "reason": "hidden", "ref": null})
        )),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        decode::<Resynced>(&bytes(&json!({"creditID": 1, "reason": "stale"}))),
        Err(Refusal::Invalid),
        "stale is a drop reason, not a resync one"
    );
}
