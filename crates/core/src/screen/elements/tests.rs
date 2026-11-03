use super::*;

#[test]
fn every_look_name_is_five_letters_that_read_back_as_the_same_look() {
    for number in [
        0,
        1,
        25,
        26,
        675,
        104_729,
        LOOK_NAMES - 1,
        LOOK_NAMES,
        u64::MAX,
    ] {
        let look = Look::new(number);
        let name = look.to_string();
        assert_eq!(name.len(), 5, "{name}");
        assert!(name.bytes().all(|byte| byte.is_ascii_lowercase()), "{name}");
        assert_eq!(Look::parse(&name), Some(look), "{name}");
    }
    assert_eq!(Look::new(0).to_string(), "aaaaa");
    assert_eq!(Look::new(LOOK_NAMES - 1).to_string(), "zzzzz");
    assert_eq!(Look::new(LOOK_NAMES), Look::new(0));
}

#[test]
fn a_reference_is_its_look_and_index_written_exactly_one_way() {
    let reference = Reference::parse("kfpaq12").unwrap();
    assert_eq!(reference.look, Look::parse("kfpaq").unwrap());
    assert_eq!(reference.index, 12);
    assert_eq!(reference.to_string(), "kfpaq12");
    assert_eq!(Reference::parse("kfpaq0").unwrap().index, 0);
    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        serde_json::json!("kfpaq12")
    );
    assert_eq!(
        serde_json::from_value::<Reference>(serde_json::json!("kfpaq12")).unwrap(),
        reference
    );
}

#[test]
fn anything_but_a_listed_reference_is_no_reference() {
    for text in [
        "",
        "kfpaq",
        "kfp12",
        "kfpaqa12",
        "KFPAQ12",
        "kfpaq012",
        "kfpaq+1",
        "kfpaq-1",
        "kfpaq1.5",
        "kfpaq 1",
        " kfpaq1",
        "kfpaq.e4",
        "é123",
        "kfpaq99999999999999999999999",
    ] {
        assert_eq!(Reference::parse(text), None, "{text:?}");
    }
    for value in [
        serde_json::json!("kfpaq"),
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
