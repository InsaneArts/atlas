use super::*;

#[test]
fn every_look_name_is_four_letters_that_read_back_as_the_same_look() {
    for number in [0, 1, 25, 26, 675, 104_729, LOOK_NAMES - 1, LOOK_NAMES, u64::MAX] {
        let look = Look::new(number);
        let name = look.to_string();
        assert_eq!(name.len(), 4, "{name}");
        assert!(name.bytes().all(|byte| byte.is_ascii_lowercase()), "{name}");
        assert_eq!(Look::parse(&name), Some(look), "{name}");
    }
    assert_eq!(Look::new(0).to_string(), "aaaa");
    assert_eq!(Look::new(LOOK_NAMES - 1).to_string(), "zzzz");
    assert_eq!(Look::new(LOOK_NAMES), Look::new(0));
}

#[test]
fn a_reference_is_its_look_and_index_written_exactly_one_way() {
    let reference = Reference::parse("kfpa12").unwrap();
    assert_eq!(reference.look, Look::parse("kfpa").unwrap());
    assert_eq!(reference.index, 12);
    assert_eq!(reference.to_string(), "kfpa12");
    assert_eq!(Reference::parse("kfpa0").unwrap().index, 0);
    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        serde_json::json!("kfpa12")
    );
    assert_eq!(
        serde_json::from_value::<Reference>(serde_json::json!("kfpa12")).unwrap(),
        reference
    );
}

#[test]
fn anything_but_a_listed_reference_is_no_reference() {
    for text in [
        "",
        "kfpa",
        "kfp12",
        "kfpaa12",
        "KFPA12",
        "kfpa012",
        "kfpa+1",
        "kfpa-1",
        "kfpa1.5",
        "kfpa 1",
        " kfpa1",
        "kfpa.e4",
        "é123",
        "kfpa99999999999999999999999",
    ] {
        assert_eq!(Reference::parse(text), None, "{text:?}");
    }
    for value in [
        serde_json::json!("kfpa"),
        serde_json::json!(12),
        serde_json::json!(null),
    ] {
        assert!(serde_json::from_value::<Reference>(value).is_err());
    }
}

#[test]
fn a_stale_reference_is_refused_with_nothing_sent_and_a_fresh_look_advised() {
    let refused = serde_json::to_value(stale("That look is no longer kept")).unwrap();
    assert_eq!(
        refused,
        serde_json::json!({"ok": false, "code": "elements", "uncertain": false,
            "error": "That look is no longer kept", "next": "Run atlas elements again."})
    );
}
