use super::*;
use serde_json::json;

fn hex(digits: usize, digit: char) -> String {
    std::iter::repeat_n(digit, digits).collect()
}

fn reference() -> serde_json::Value {
    json!({"serviceInstance": hex(32, 'a'), "observerID": hex(32, 'b'),
        "observerGeneration": 1, "streamGeneration": 2, "geometryGeneration": 3,
        "sourceSequence": 4, "imageSequence": 5, "frameKey": hex(64, 'c'),
        "viewGeneration": 6})
}

fn bytes(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

#[test]
fn a_reference_decodes_only_with_exactly_its_keys_and_shapes() {
    let decoded: Ref = decode(&bytes(&reference())).unwrap();
    assert_eq!(decoded.frame_key.as_str(), hex(64, 'c'));
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        reference(),
        "round trip"
    );

    let changed = |key: &str, value: serde_json::Value| {
        let mut wrong = reference();
        wrong[key] = value;
        decode::<Ref>(&bytes(&wrong))
    };
    for (key, value) in [
        ("observerID", json!(hex(32, 'B'))),
        ("observerID", json!(hex(31, 'b'))),
        ("frameKey", json!(hex(32, 'c'))),
        ("observerGeneration", json!(0)),
        ("observerGeneration", json!(-1)),
        ("observerGeneration", json!(1.5)),
        ("observerGeneration", json!("1")),
        ("imageSequence", serde_json::Value::Null),
        ("extra", json!(1)),
    ] {
        assert_eq!(
            changed(key, value.clone()),
            Err(Refusal::Invalid),
            "{key}: {value}"
        );
    }
    let mut missing = reference();
    missing.as_object_mut().unwrap().remove("viewGeneration");
    assert_eq!(decode::<Ref>(&bytes(&missing)), Err(Refusal::Invalid));
    let duplicated = format!(
        r#"{{"frameKey":"{}",{}"#,
        hex(64, 'd'),
        &serde_json::to_string(&reference()).unwrap()[1..]
    );
    assert_eq!(
        decode::<Ref>(duplicated.as_bytes()),
        Err(Refusal::Invalid),
        "the guard runs first"
    );
}

#[test]
fn an_optional_key_may_be_absent_but_never_null() {
    let demand: Demand = decode(br#"{"desiredLongest": 1600, "backend": "cg"}"#).unwrap();
    assert_eq!(demand.hard_ceiling, None);
    assert_eq!(
        serde_json::to_value(demand).unwrap(),
        json!({"desiredLongest": 1600, "backend": "cg"})
    );
    let ceiling: Demand =
        decode(br#"{"desiredLongest": 1600, "backend": "sck", "hardCeiling": 640}"#).unwrap();
    assert_eq!(ceiling.hard_ceiling.map(Longest::get), Some(640));
    for text in [
        &br#"{"desiredLongest": 1600, "backend": "cg", "hardCeiling": null}"#[..],
        br#"{"desiredLongest": 319, "backend": "cg"}"#,
        br#"{"desiredLongest": 4097, "backend": "cg"}"#,
        br#"{"desiredLongest": 1600, "backend": "vnc"}"#,
    ] {
        assert_eq!(
            decode::<Demand>(text),
            Err(Refusal::Invalid),
            "{}",
            String::from_utf8_lossy(text)
        );
    }
}

#[test]
fn a_profile_names_finite_geometry_and_its_exact_stride() {
    let profile: Profile = decode(&bytes(
        &json!({"streamGeneration": 1, "geometryGeneration": 1,
        "width": 4, "height": 2, "rowStride": 12, "displayWidth": 1512.0, "displayHeight": 982.0,
        "displayScale": 2.0, "backend": "cg"}),
    ))
    .unwrap();
    assert_eq!(profile.bytes(), 24);
    let mut skewed = serde_json::to_value(profile).unwrap();
    skewed["rowStride"] = json!(13);
    assert_eq!(decode::<Profile>(&bytes(&skewed)), Err(Refusal::Invalid));
    for (key, value) in [
        ("width", json!(0)),
        ("height", json!(4097)),
        ("displayScale", json!(0.0)),
    ] {
        let mut wrong = serde_json::to_value(profile).unwrap();
        wrong[key] = value;
        assert_eq!(
            decode::<Profile>(&bytes(&wrong)),
            Err(Refusal::Invalid),
            "{key}"
        );
    }
}

#[test]
fn errors_name_a_credit_exactly_when_they_retire_one() {
    let failure = json!({"code": "stale-frame", "stage": "source-age", "uncertain": false,
        "dispatched": false, "reason": "source-stale"});
    let retired: ErrorMetadata = decode(&bytes(
        &json!({"failure": failure, "disposition": "credit-retired", "creditID": 9}),
    ))
    .unwrap();
    assert_eq!(retired.credit_id.map(Positive::get), Some(9));
    let mut sent = failure.clone();
    sent["dispatched"] = json!(true);
    for wrong in [
        json!({"failure": failure, "disposition": "pair-fatal", "creditID": 9}),
        json!({"failure": failure, "disposition": "credit-retired"}),
        json!({"failure": sent, "disposition": "request-refused"}),
    ] {
        assert_eq!(
            decode::<ErrorMetadata>(&bytes(&wrong)),
            Err(Refusal::Invalid),
            "{wrong}"
        );
    }
    let mut native = failure.clone();
    native["reason"] = json!("/Users/someone/secret");
    assert_eq!(
        decode::<ErrorMetadata>(&bytes(
            &json!({"failure": native, "disposition": "pair-fatal"})
        )),
        Err(Refusal::Invalid),
        "no free text enters a failure"
    );
}

#[test]
fn identifiers_are_fresh_and_exactly_shaped() {
    let (first, second) = (Hex64::random(), Hex64::random());
    assert_ne!(first, second);
    assert!(Hex64::parse(first.as_str()).is_some());
    assert_eq!(Hex32::random().as_str().len(), 32);
    assert!(Hex32::parse(&hex(32, 'g')).is_none());
    let hello = Hello {
        protocol_major: 1,
        service_instance: Hex32::random(),
        bootstrap_capability: Hex64::random(),
        client_challenge: Hex32::random(),
        role: Role::Observer,
        client_kind: ClientKind::Terminal,
        requested_rights: vec![Right::Observe, Right::StopAgents],
        offered_updates: vec![UpdateKind::Full],
    };
    let wire = encode(&hello).unwrap();
    let text = String::from_utf8(wire.clone()).unwrap();
    assert!(text.contains("\"requestedRights\":[\"observe\",\"stop-agents\"]"));
    assert_eq!(decode::<Hello>(&wire).unwrap(), hello);
}

#[test]
fn openings_speak_this_protocol_and_keep_the_pair_tokens_fixed_life() {
    let hello = json!({"protocolMajor": 1, "serviceInstance": hex(32, 'a'),
        "bootstrapCapability": hex(64, 'b'), "clientChallenge": hex(32, 'c'),
        "role": "observer", "clientKind": "swift",
        "requestedRights": ["observe"], "offeredUpdates": ["full", "delta"]});
    assert!(decode::<Hello>(&bytes(&hello)).is_ok());
    for (key, value) in [
        ("protocolMajor", json!(2)),
        ("requestedRights", json!(["control"])),
        ("offeredUpdates", json!(["delta"])),
    ] {
        let mut wrong = hello.clone();
        wrong[key] = value;
        assert_eq!(
            decode::<Hello>(&bytes(&wrong)),
            Err(Refusal::Invalid),
            "{key}"
        );
    }
    let welcome = json!({"serviceInstance": hex(32, 'a'), "observerID": hex(32, 'd'),
        "observerGeneration": 1, "controlConnectionID": hex(32, 'e'),
        "clientChallenge": hex(32, 'c'), "grantedRights": ["observe"],
        "acceptedUpdates": ["full"], "pairToken": hex(64, 'f'),
        "pairExpiresInMs": PAIR_EXPIRES_MS, "cleanupToken": hex(64, '1')});
    assert!(decode::<Welcome>(&bytes(&welcome)).is_ok());
    for (key, value) in [
        ("pairExpiresInMs", json!(PAIR_EXPIRES_MS + 1)),
        ("acceptedUpdates", json!(["unchanged"])),
    ] {
        let mut wrong = welcome.clone();
        wrong[key] = value;
        assert_eq!(
            decode::<Welcome>(&bytes(&wrong)),
            Err(Refusal::Invalid),
            "{key}"
        );
    }
}
