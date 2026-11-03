use super::*;

#[test]
fn every_look_is_its_service_and_count_and_reads_back_as_the_same_look() {
    for (incarnation, sequence, written) in [
        (1, 0, "8123aba"),
        (1, 25, "8123abz"),
        (1, 26, "8123abaa"),
        (1, 701, "8123abzz"),
        (1, 702, "8123abaaa"),
        (0, 2, "8123aac"),
        (INCARNATIONS - 1, 2, "8123zzc"),
        (1, u64::MAX, "8123abgkgwbylwrxtlpp"),
    ] {
        let look = Look {
            service: 8123,
            incarnation,
            sequence,
        };
        assert_eq!(look.to_string(), written);
        assert_eq!(Look::parse(written), Some(look), "{written}");
    }
    let last = Look {
        service: 8123,
        incarnation: 1,
        sequence: u64::MAX,
    };
    assert_eq!(last.after(), None, "the count never comes around again");
    assert_eq!(
        Look::parse("8123aba").unwrap().after(),
        Look::parse("8123abb")
    );
    assert_ne!(
        Look::parse("8123abc"),
        Look::parse("8123acc"),
        "another incarnation of the same process ID"
    );
}

#[test]
fn a_reference_is_its_look_and_index_written_exactly_one_way() {
    let reference = Reference::parse("8123abc12").unwrap();
    assert_eq!(
        reference.look,
        Look {
            service: 8123,
            incarnation: 1,
            sequence: 2
        }
    );
    assert_eq!(reference.index, 12);
    assert_eq!(reference.to_string(), "8123abc12");
    assert_eq!(Reference::parse("8123abc0").unwrap().index, 0);
    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        serde_json::json!("8123abc12")
    );
    assert_eq!(
        serde_json::from_value::<Reference>(serde_json::json!("8123abc12")).unwrap(),
        reference
    );
}

#[test]
fn anything_but_a_listed_reference_is_no_reference() {
    for text in [
        "",
        "8123abc",
        "8123c12",
        "8123ab12",
        "abc12",
        "0abc12",
        "08123abc12",
        "8123ABC12",
        "8123abc012",
        "8123abc+1",
        "+8123abc1",
        "8123abc1.5",
        "8123abc 1",
        " 8123abc1",
        "8123abc.e4",
        "8123abc1a2",
        "é123",
        "8123abc99999999999999999999999",
        "99999999999abc1",
        "8123abzzzzzzzzzzzzzzzzzzzzzzzzzzzz1",
    ] {
        assert_eq!(Reference::parse(text), None, "{text:?}");
    }
    for value in [
        serde_json::json!("8123abc"),
        serde_json::json!(12),
        serde_json::json!(null),
    ] {
        assert!(serde_json::from_value::<Reference>(value).is_err());
    }
}

/// A menu bar title is judged against the menu bar, the Window Server's
/// window at the point to click, and only while that is shown; an app's
/// own window at that layer is not the bar.
#[test]
fn the_menu_bar_is_the_window_servers() {
    let window = |number: i64, owner: &str, layer: i64, alpha: f64| Window {
        number,
        pid: 433,
        owner: Some(owner.into()),
        title: None,
        layer,
        alpha,
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 30.0,
        },
        system: None,
    };
    let bar =
        |owner: &str, alpha: f64| window(3, owner, super::super::windows::MENU_BAR_LAYER, alpha);
    let app = || window(9, "Opie", 0, 1.0);

    assert_eq!(
        menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 12.0),
        Some(3)
    );
    assert_eq!(
        menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 60.0),
        None
    );
    assert_eq!(
        menu_bar_window(&[bar("Window Server", 0.0), app()], 232.0, 12.0),
        None
    );
    assert_eq!(
        menu_bar_window(&[bar("Opie", 1.0), app()], 232.0, 12.0),
        None
    );
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
