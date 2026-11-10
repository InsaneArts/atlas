use super::*;

/// The facts of an input sending `actions` on a display of 1512 by 982 points.
fn input(actions: Value) -> Map<String, Value> {
    let actions: Vec<Action> = serde_json::from_value(actions).unwrap();
    let epoch = InputEpoch {
        uid: 502,
        security_session_id: 1,
        display_id: 1,
        width: 1512.0,
        height: 982.0,
        process_id: 1,
    };
    of_input(&actions, &epoch)
}

fn row(fields: Value) -> Row {
    let mut row = json!({"operation": "input"});
    row.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    Row::parse(&row).unwrap()
}

#[test]
fn an_inputs_facts_say_what_it_did_where_on_which_display() {
    let click =
        input(json!([{"type": "click", "x": 10.5, "y": 20.0, "button": "right", "count": 2}]));
    assert_eq!(
        Value::Object(click),
        json!({"action": "click", "point": {"x": 10.5, "y": 20.0}, "clicks": 2,
            "button": "right", "display": {"width": 1512.0, "height": 982.0}})
    );

    let drag = input(json!([{"type": "drag", "fromX": 1, "fromY": 2, "toX": 3, "toY": 4}]));
    assert_eq!(drag["point"], json!({"x": 1.0, "y": 2.0}));
    assert_eq!(drag["to"], json!({"x": 3.0, "y": 4.0}));

    let scrolls = input(json!([
        {"type": "scroll", "x": 5, "y": 6, "dy": 3},
        {"type": "scroll", "x": 5, "y": 6, "dy": 4}
    ]));
    assert_eq!(
        scrolls["scroll"],
        json!({"dx": 0, "dy": 7}),
        "one scroll by all"
    );

    let typed =
        input(json!([{"type": "type", "text": "Hello, "}, {"type": "type", "text": "world"}]));
    assert_eq!(typed["text"], "Hello, world");

    let referenced =
        input(json!([{"type": "reference", "ref": "8123abc.12", "button": "right", "count": 2}]));
    assert_eq!(
        Value::Object(referenced),
        json!({"action": "click", "clicks": 2, "button": "right",
            "display": {"width": 1512.0, "height": 982.0}}),
        "a referenced click is a click, at its element"
    );

    let pressed = input(json!([{"type": "press", "ref": "8123abc.12"}]));
    assert_eq!(pressed["action"], "press");
    assert!(
        pressed.get("point").is_none(),
        "pressed by name, never moved"
    );
}

/// What a command typed: `text`, into fields of `into` in turn.
fn typed(text: Option<&str>, into: &[Option<&str>]) -> Typed {
    Typed {
        text: text.map(str::to_owned),
        into: into.iter().map(|role| role.map(str::to_owned)).collect(),
    }
}

#[test]
fn typed_text_is_kept_whole_or_to_its_limit_and_never_for_a_password() {
    let typing = input(json!([{"type": "type", "text": "part"}]));
    let worked = row(json!({"ok": true, "verb": "type"}));
    let field = Some("AXTextField");
    let release = || typed(Some("Release 0.1.1"), &[field, field]);

    let whole = finished(typing.clone(), &worked, &json!({}), release(), None, 0);
    assert_eq!(
        (&whole["text"], &whole["length"]),
        (&json!("Release 0.1.1"), &json!(13))
    );

    let limited = finished(typing.clone(), &worked, &json!({}), release(), Some(7), 0);
    assert_eq!(
        (&limited["text"], &limited["length"]),
        (&json!("Release"), &json!(13))
    );

    let batch = finished(
        typing.clone(),
        &worked,
        &json!({}),
        typed(None, &[field]),
        None,
        0,
    );
    assert_eq!(
        batch["text"], "part",
        "the input's own text before all is typed"
    );

    let password = Some("AXSecureTextField");
    for into in [vec![password], vec![field, password], vec![None, password]] {
        let secret = finished(
            typing.clone(),
            &worked,
            &json!({}),
            typed(Some("hunter2"), &into),
            None,
            0,
        );
        assert_eq!(secret["secure"], true, "{into:?}");
        assert!(secret.get("text").is_none() && secret.get("length").is_none());
        assert!(!secret.to_string().contains("hunter2"));
    }

    for into in [
        vec![],
        vec![None],
        vec![field, None],
        vec![None, field],
        vec![field, Some("AXTextArea")],
    ] {
        let unread = finished(
            typing.clone(),
            &worked,
            &json!({}),
            typed(Some("hunter2"), &into),
            None,
            0,
        );
        assert!(
            unread.get("text").is_none()
                && unread.get("length").is_none()
                && unread.get("secure").is_none(),
            "text any input put in a field the helper could not tell may be a password: {into:?}"
        );
    }
}

#[test]
fn no_text_is_kept_that_may_have_moved_on_to_another_field() {
    let field = Some("AXTextField");
    let worked = row(json!({"ok": true, "verb": "type"}));
    for text in [
        "alice\thunter2",
        "alice\nhunter2",
        "alice\rhunter2",
        "alice\u{1b}x",
        "a\u{2028}b",
    ] {
        let typing = input(json!([{"type": "type", "text": text}]));
        let facts = finished(
            typing,
            &worked,
            &json!({}),
            typed(Some(text), &[field]),
            None,
            0,
        );
        assert!(
            facts.get("text").is_none() && facts.get("length").is_none(),
            "{text:?}: {facts}"
        );
        assert!(!facts.to_string().contains("hunter2"), "{facts}");
    }
}

#[test]
fn a_commands_outcome_reads_as_it_is_marked() {
    let click = input(json!([{"type": "click", "x": 1, "y": 1}]));
    for (fields, outcome) in [
        (json!({"ok": true}), "worked"),
        (
            json!({"ok": false, "code": "focus-changed", "reason": "app"}),
            "notSent",
        ),
        (
            json!({"ok": false, "code": "focus-changed", "partial": true}),
            "failed",
        ),
        (json!({"ok": false, "code": "uncertain"}), "uncertain"),
        (json!({"ok": false, "code": "elements"}), "failed"),
    ] {
        let facts = finished(
            click.clone(),
            &row(fields.clone()),
            &json!({}),
            Typed::default(),
            None,
            0,
        );
        assert_eq!(facts["outcome"], outcome, "{fields}");
    }
}

#[test]
fn a_shortcuts_keys_read_as_keycaps_and_its_times_in_milliseconds() {
    let keys = input(json!([{"type": "key", "key": "cmd+s"}]));
    let pressed = row(json!({"ok": true, "verb": "key", "label": "⌘S"}));
    let facts = finished(
        keys,
        &pressed,
        &json!({}),
        Typed::default(),
        None,
        5_000_000,
    );
    assert_eq!(facts["keys"], json!([["⌘", "S"]]));
    assert_eq!(facts["label"], "⌘S");
    assert_eq!(facts["at"], 5);
    assert!(facts["endedAt"].as_u64().is_some());
}

#[test]
fn keys_that_type_text_are_never_kept() {
    let typed = input(json!([{"type": "key", "key": "s"}, {"type": "key", "key": "shift+e"}]));
    assert!(!typed.contains_key("keys"), "{typed:?}");
    // A row names no key that types text, so neither do the facts.
    let pressed = row(json!({"ok": true, "verb": "key"}));
    let facts = finished(typed, &pressed, &json!({}), Typed::default(), None, 0);
    assert_eq!(facts["action"], "key");
    assert!(facts.get("keys").is_none(), "{facts}");
}

fn element(role: &str, title: &str, frame: Option<Bounds>) -> Element {
    Element {
        role: Some(role.into()),
        title: Some(title.into()),
        frame,
        ..Element::default()
    }
}

fn frame() -> Bounds {
    Bounds {
        x: 20.0,
        y: 30.0,
        width: 80.0,
        height: 24.0,
    }
}

#[test]
fn a_found_element_is_kept_by_its_frame_and_role_with_a_name_only_for_a_control() {
    let button = found(&element("AXButton", "Save", Some(frame())));
    assert_eq!(
        Value::Object(button),
        json!({"element": {"frame": {"x": 20.0, "y": 30.0, "width": 80.0, "height": 24.0},
            "role": "AXButton", "name": "Save"}})
    );

    let text = found(&element("AXStaticText", "Dear Sam", Some(frame())));
    assert_eq!(text["element"]["role"], "AXStaticText");
    assert!(
        text["element"].get("name").is_none(),
        "never a document's text"
    );
    assert!(!Value::Object(text).to_string().contains("Dear Sam"));

    let unframed = found(&element("AXButton", "Save", None));
    assert!(unframed.is_empty(), "no frame says nothing");
}

#[test]
fn an_element_the_input_service_reached_is_kept_unless_the_command_found_one() {
    let click = input(json!([{"type": "reference", "ref": "8123abc.12"}]));
    let worked = row(json!({"ok": true}));
    let reached = json!({"ok": true, "element": {"frame": {"x": 1, "y": 2, "width": 3, "height": 4},
        "role": "AXTextArea", "name": "Dear Sam"}});
    let facts = finished(click.clone(), &worked, &reached, Typed::default(), None, 0);
    assert_eq!(
        facts["element"],
        json!({"frame": {"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0}, "role": "AXTextArea"})
    );
    assert!(!facts.to_string().contains("Dear Sam"));

    let pressed = json!({"ok": true, "element": {"frame": {"x": 1, "y": 2, "width": 3, "height": 4},
        "role": "AXCheckBox", "name": "Wrap"}});
    let facts = finished(click.clone(), &worked, &pressed, Typed::default(), None, 0);
    assert_eq!(facts["element"]["name"], "Wrap");

    let mut named = click.clone();
    named.extend(found(&element("AXButton", "Save", Some(frame()))));
    let facts = finished(named, &worked, &pressed, Typed::default(), None, 0);
    assert_eq!(
        facts["element"]["name"], "Save",
        "the command's own is kept"
    );

    for reply in [
        json!({"ok": true}),
        json!({"ok": true, "element": {"role": "AXButton"}}),
        json!({"ok": true, "element": {"frame": {"x": "1"}}}),
    ] {
        let facts = finished(click.clone(), &worked, &reply, Typed::default(), None, 0);
        assert!(facts.get("element").is_none(), "{reply}");
    }
}
