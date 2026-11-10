use super::*;

fn bounds(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}

fn display() -> Bounds {
    bounds(0.0, 0.0, 1000.0, 800.0)
}

/// A drawn button named `name` at `index`, enabled, in a window on screen.
fn button(index: usize, name: &str) -> Element {
    Element {
        index,
        role: Some("AXButton".to_owned()),
        title: Some(name.to_owned()),
        enabled: Some(true),
        window: Some(bounds(100.0, 100.0, 400.0, 300.0)),
        frame: Some(bounds(150.0, 150.0, 80.0, 24.0)),
        ..Element::default()
    }
}

/// A text field holding `value`, described as `name`.
fn field(index: usize, name: &str, value: &str) -> Element {
    Element {
        role: Some("AXTextField".to_owned()),
        title: None,
        description: Some(name.to_owned()),
        value: Some(value.to_owned()),
        ..button(index, name)
    }
}

fn named(name: &str) -> ElementFilter {
    ElementFilter {
        role: None,
        name: Some(name.to_owned()),
    }
}

fn judged(elements: &[Element], incomplete: bool, name: &str, state: &State) -> Judged {
    judge(elements, incomplete, &named(name), state, &display())
}

#[test]
fn an_element_exists_once_any_matches_and_names_the_one_a_click_would_pick() {
    assert_eq!(
        judged(&[button(0, "Cancel")], false, "Save", &State::Exists),
        Judged::Unmet(Why::None)
    );
    assert_eq!(
        judged(
            &[button(0, "Cancel"), button(1, "Save")],
            false,
            "Save",
            &State::Exists
        ),
        Judged::Met(Some(1))
    );
    assert_eq!(
        judged(
            &[button(1, "Save"), button(2, "Save")],
            false,
            "Save",
            &State::Exists
        ),
        Judged::Met(None),
        "two matches still exist, though neither is the one"
    );
    let collapsed = Element {
        frame: Some(bounds(150.0, 150.0, 0.0, 0.0)),
        ..button(3, "Save")
    };
    assert_eq!(
        judged(&[collapsed], true, "Save", &State::Exists),
        Judged::Met(None),
        "an element that is there but not drawn exists, even in an incomplete scan"
    );
}

#[test]
fn an_element_is_gone_only_when_a_complete_scan_shows_nothing_that_might_match() {
    assert_eq!(
        judged(&[button(0, "Cancel")], false, "Save", &State::Gone),
        Judged::Met(None)
    );
    assert_eq!(
        judged(&[button(0, "Save")], false, "Save", &State::Gone),
        Judged::Unmet(Why::Present)
    );
    assert_eq!(
        judged(&[button(0, "Cancel")], true, "Save", &State::Gone),
        Judged::Unmet(Why::Incomplete),
        "what an incomplete scan missed might still match"
    );
    let unread = Element {
        title: None,
        unreadable: vec!["AXTitle".to_owned()],
        ..button(0, "Save")
    };
    assert_eq!(
        judged(&[unread], false, "Save", &State::Gone),
        Judged::Unmet(Why::Unread)
    );
    assert_eq!(judged(&[], false, "Save", &State::Gone), Judged::Met(None));
}

#[test]
fn enabled_needs_the_one_element_to_say_it_is_enabled() {
    assert_eq!(
        judged(&[button(4, "Save")], false, "Save", &State::Enabled),
        Judged::Met(Some(4))
    );
    for (enabled, why) in [(Some(false), Why::Disabled), (None, Why::UnknownEnabled)] {
        let element = Element {
            enabled,
            ..button(4, "Save")
        };
        assert_eq!(
            judged(&[element], false, "Save", &State::Enabled),
            Judged::Unmet(why)
        );
    }
}

#[test]
fn a_state_of_the_one_element_waits_while_there_is_none_or_several_or_a_doubt() {
    for state in [State::Enabled, State::Visible, State::Value("x".to_owned())] {
        assert_eq!(
            judged(&[button(0, "Cancel")], false, "Save", &state),
            Judged::Unmet(Why::None)
        );
        assert_eq!(
            judged(
                &[button(0, "Save"), button(1, "Save")],
                false,
                "Save",
                &state
            ),
            Judged::Unmet(Why::Several)
        );
        let doubt = Element {
            title: Some("Sa".to_owned()),
            truncated: vec!["AXTitle".to_owned()],
            ..button(1, "Save")
        };
        assert_eq!(
            judged(&[button(0, "Save"), doubt], false, "Save", &state),
            Judged::Unmet(Why::Doubtful)
        );
    }
}

#[test]
fn visible_needs_a_drawn_frame_inside_its_window_and_the_display() {
    assert_eq!(
        judged(&[button(0, "Save")], false, "Save", &State::Visible),
        Judged::Met(Some(0))
    );
    for (frame, window) in [
        (
            bounds(2000.0, 150.0, 80.0, 24.0),
            Some(bounds(1900.0, 100.0, 400.0, 300.0)),
        ),
        (
            bounds(600.0, 600.0, 80.0, 24.0),
            Some(bounds(100.0, 100.0, 400.0, 300.0)),
        ),
        (bounds(150.0, 150.0, 80.0, 24.0), None),
    ] {
        let element = Element {
            frame: Some(frame.clone()),
            window,
            ..button(0, "Save")
        };
        assert_eq!(
            judged(&[element], false, "Save", &State::Visible),
            Judged::Unmet(Why::Hidden),
            "{frame:?}"
        );
    }
}

#[test]
fn a_value_holds_only_when_read_whole_and_exactly_the_same() {
    let wanted = State::Value("Ada".to_owned());
    assert_eq!(
        judged(&[field(0, "Name", "Ada")], false, "Name", &wanted),
        Judged::Met(Some(0))
    );
    assert_eq!(
        judged(&[field(0, "Name", "ada")], false, "Name", &wanted),
        Judged::Unmet(Why::OtherValue),
        "a value is typed text, compared exactly"
    );
    let cut = Element {
        truncated: vec!["AXValue".to_owned()],
        ..field(0, "Name", "Ada")
    };
    assert_eq!(
        judged(&[cut], false, "Name", &wanted),
        Judged::Unmet(Why::CutValue)
    );
    let unread = Element {
        value: None,
        unreadable: vec!["AXValue".to_owned()],
        ..field(0, "Name", "Ada")
    };
    assert_eq!(
        judged(&[unread], false, "Name", &wanted),
        Judged::Unmet(Why::UnreadValue)
    );
    let empty = Element {
        value: None,
        ..field(0, "Name", "Ada")
    };
    assert_eq!(
        judged(&[empty], false, "Name", &wanted),
        Judged::Unmet(Why::OtherValue)
    );
}

#[test]
fn every_reason_a_wait_gives_keeps_its_own_word_and_saying() {
    for (why, word, says) in [
        (Why::None, "none", "no element matched"),
        (Why::Several, "several", "more than one element matched"),
        (
            Why::Doubtful,
            "doubtful",
            "another element's name could not be read whole",
        ),
        (
            Why::Present,
            "present",
            "a matching element was still there",
        ),
        (Why::Incomplete, "incomplete", "the scan was incomplete"),
        (
            Why::Unread,
            "unread",
            "an element's name could not be read whole",
        ),
        (Why::Disabled, "disabled", "the element was disabled"),
        (
            Why::UnknownEnabled,
            "unknown-enabled",
            "the element did not say whether it was enabled",
        ),
        (Why::Hidden, "hidden", "the element was not on screen"),
        (
            Why::OtherValue,
            "other-value",
            "the element held another value",
        ),
        (
            Why::CutValue,
            "cut-value",
            "the element's value was too long to compare",
        ),
        (
            Why::UnreadValue,
            "unread-value",
            "the element's value could not be read",
        ),
    ] {
        assert_eq!(serde_json::to_value(why).unwrap(), word, "{why:?}");
        assert_eq!(why.says(), says, "{why:?}");
    }
}

#[test]
fn a_missed_wait_says_in_a_sentence_what_did_not_happen() {
    for (state, word, missed) in [
        (
            State::Exists,
            "exists",
            "The element did not appear in time",
        ),
        (State::Gone, "gone", "The element did not go in time"),
        (
            State::Enabled,
            "enabled",
            "The element was not enabled in time",
        ),
        (
            State::Visible,
            "visible",
            "The element was not on screen in time",
        ),
        (
            State::Value("x".to_owned()),
            "value",
            "The element did not hold the value in time",
        ),
    ] {
        assert_eq!((state.word(), state.missed()), (word, missed));
    }
}

#[test]
fn an_incomplete_scan_never_proves_the_one_element_a_state_is_about() {
    for state in [
        State::Enabled,
        State::Visible,
        State::Value("Save".to_owned()),
    ] {
        assert_eq!(
            judged(&[button(0, "Save")], true, "Save", &state),
            Judged::Unmet(Why::Incomplete),
            "{state:?}"
        );
    }
}
