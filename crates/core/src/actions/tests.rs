use super::*;

#[test]
fn decoded_batches_keep_optional_wire_members_exactly_as_supplied() {
    use serde_json::json;
    let display = Display {
        width: 800.0,
        height: 600.0,
        scale: 1.0,
    };
    for action in [
        json!({"type":"click","x":10,"y":20}),
        json!({"type":"click","x":10,"y":20,"button":"left","count":1,"modifiers":[]}),
        json!({"type":"pointer","phase":"down","x":10,"y":20}),
        json!({"type":"pointer","phase":"move","x":10,"y":20,"button":"right","count":2,"modifiers":["shift"]}),
        json!({"type":"move","x":10,"y":20}),
        json!({"type":"drag","fromX":10,"fromY":20,"toX":30,"toY":40}),
        json!({"type":"scroll","x":10,"y":20}),
        json!({"type":"scroll","x":10,"y":20,"dx":0,"dy":1}),
        json!({"type":"type","text":"a"}),
        json!({"type":"key","key":"return"}),
        json!({"type":"wait","ms":0}),
    ] {
        let wire = json!([action]);
        let batch = Batch::decode(&wire).unwrap();
        batch.check(&display).unwrap();
        assert_eq!(batch.actions().len(), 1);
        assert_eq!(batch.wire(), &wire);
    }
    let wire = json!([{"type":"move","x":700,"y":20}]);
    let batch = Batch::decode(&wire).unwrap();
    batch.check(&display).unwrap();
    let error = batch
        .check(&Display {
            width: 600.0,
            ..display
        })
        .unwrap_err();
    assert_eq!(
        error.error,
        "Action 1 was refused and nothing was sent: its point is off the display"
    );
}

#[test]
fn an_expectation_must_be_a_process_id() {
    use serde_json::json;

    assert_eq!(Expect::from_request(&Value::Null).unwrap(), None);
    assert_eq!(Expect::from_request(&json!({})["expect"]).unwrap(), None);
    let five = Expect::from_request(&json!({"pid": 5})).unwrap().unwrap();
    assert_eq!(five.pid(), 5);
    assert_eq!(serde_json::to_value(&five).unwrap(), json!({"pid": 5}));

    for refused in [
        json!({"pid": 0}),
        json!({"pid": -1}),
        json!({"pid": "5"}),
        json!({"pid": 2147483648_i64}),
        json!({}),
        json!({"pid": 5, "app": "TextEdit"}),
        json!(5),
    ] {
        let error = Expect::from_request(&refused).unwrap_err();
        assert_eq!(error.code, "input", "{refused}");
    }

    assert!(Expect::new(812).is_ok());
    assert!(Expect::new(0).is_err());
    assert!(Expect::new(1 << 31).is_err());
}

#[test]
fn a_process_start_time_is_optional_but_never_silently_discarded() {
    use serde_json::json;

    for value in [json!({"pid":5}), json!({"pid":5,"start":null})] {
        assert!(
            Expect::from_request(&value)
                .unwrap()
                .unwrap()
                .instance()
                .is_none()
        );
    }
    let value = json!({"pid":5,"start":[123,456],"window":42,"field":"Email"});
    let expect = Expect::from_request(&value).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&expect).unwrap(), value);
    assert_eq!(expect.app_only().instance(), expect.instance());
    for start in [
        json!([0, 1]),
        json!([123, 1_000_000]),
        json!([-1, 0]),
        json!([123]),
        json!("123"),
    ] {
        assert_eq!(
            Expect::from_request(&json!({"pid":5,"start":start}))
                .unwrap_err()
                .code,
            Code::Input
        );
    }
}

#[test]
fn an_expectation_may_name_a_window_number_and_a_field() {
    use serde_json::json;

    let named = json!({"pid": 5, "window": 4120, "field": "Email"});
    let expect = Expect::from_request(&named).unwrap().unwrap();
    assert_eq!(
        (expect.pid(), expect.window(), expect.field()),
        (5, Some(4120), Some("Email"))
    );
    assert_eq!(serde_json::to_value(&expect).unwrap(), named);
    assert_eq!(
        serde_json::to_value(expect.app_only()).unwrap(),
        json!({"pid": 5})
    );

    for (refused, member) in [
        (json!({"pid": 5, "window": 0}), "\"window\""),
        (json!({"pid": 5, "window": -3}), "\"window\""),
        (json!({"pid": 5, "window": "4120"}), "\"window\""),
        (json!({"pid": 5, "window": 4294967296_i64}), "\"window\""),
        (json!({"pid": 5, "field": ""}), "\"field\""),
        (json!({"pid": 5, "field": "  "}), "\"field\""),
        (
            json!({"pid": 5, "field": "x".repeat(FIELD_MAX + 1)}),
            "\"field\"",
        ),
        (json!({"pid": 5, "field": 7}), "\"field\""),
        (json!({"window": 4120}), "process ID"),
        (json!({"pid": 5, "app": "TextEdit"}), "app"),
    ] {
        let error = Expect::from_request(&refused).unwrap_err();
        assert_eq!(error.code, "input", "{refused}");
        assert!(error.error.contains(member), "{refused}: {}", error.error);
    }
    let longest = json!({"pid": 5, "field": "é".repeat(FIELD_MAX)});
    assert!(Expect::from_request(&longest).is_ok());
}

#[test]
fn keys_are_named_or_refused() {
    for key in [
        "cmd+s",
        "return",
        "cmd+shift+z",
        "f5",
        "cmd++",
        "ctrl+alt+delete",
        "=",
        "+",
    ] {
        assert!(valid_key(key), "{key}");
    }

    assert_eq!(parse_key("+"), parse_key("shift+="), "the plus key");

    for key in ["foo", "cmd+zz", "cmd+", "++", "cmd+bogus+s", "f13", ""] {
        assert!(!valid_key(key), "{key}");
    }
}

/// Every key name and alias, alone or under one or two modifiers and in
/// either case, sends its macOS keycode and modifier bits.
#[test]
fn every_key_and_modifier_sends_its_keycode_and_bits() {
    // A printable key's keycode is its place here; a space marks a code
    // that belongs to no printable key.
    let printable = "asdfhgzxcv bqweryt123465=97-80]ou[ip lj'k;\\,/nm.  `";
    let mut keys: Vec<(String, u16)> = printable
        .chars()
        .zip(0..)
        .filter(|(key, _)| *key != ' ')
        .map(|(key, code)| (key.to_string(), code))
        .collect();
    keys.extend(
        (1..=12)
            .map(|n| format!("f{n}"))
            .zip([122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111]),
    );
    keys.extend(
        "return enter tab space backspace delete escape esc forwarddelete home pageup end pagedown left right down up"
            .split(' ')
            .map(String::from)
            .zip([36, 36, 48, 49, 51, 51, 53, 53, 117, 115, 116, 119, 121, 123, 124, 125, 126]),
    );
    let modifiers: Vec<(&str, u64)> = "cmd command ctrl control alt opt option shift fn"
        .split(' ')
        .zip([20, 20, 18, 18, 19, 19, 19, 17, 23].map(|bit| 1 << bit))
        .collect();
    let shift = 1 << 17;
    let plus = 24;

    let mut sent = vec![
        ("+".to_owned(), (plus, shift)),
        // The Kelvin sign lower-cases to k.
        ("\u{212A}".into(), (40, 0)),
        (format!("{}a", "cmd+".repeat(19)), (0, 1 << 20)),
    ];
    for (key, code) in &keys {
        sent.push((key.clone(), (*code, 0)));
        for (first, flag) in &modifiers {
            sent.push((format!("{first}+{key}"), (*code, *flag)));
            for (second, other) in &modifiers {
                sent.push((format!("{first}+{second}+{key}"), (*code, flag | other)));
            }
        }
    }
    for (first, flag) in &modifiers {
        sent.push((format!("{first}++"), (plus, flag | shift)));
        for (second, other) in &modifiers {
            sent.push((format!("{first}+{second}++"), (plus, flag | other | shift)));
        }
    }
    assert!(sent.len() > 6_000, "{} chords", sent.len());
    for (chord, expected) in &sent {
        for chord in [chord.clone(), chord.to_uppercase()] {
            assert_eq!(parse_key(&chord), Some(*expected), "{chord:?}");
        }
    }

    let mut refused = vec![
        "++".to_owned(),
        "+++".into(),
        "x".repeat(81),
        format!("{}a", "cmd+".repeat(20)),
    ];
    for key in [
        "", " ", "foo", "zz", "bogus", "ä", "f0", "f01", "f13", "f25",
    ] {
        refused.push(key.into());
        refused.push(format!("cmd+{key}"));
        refused.push(format!("cmd+shift+{key}"));
    }
    for other in ["", "meta", "super"] {
        refused.push(format!("{other}++"));
        refused.push(format!("cmd+{other}++"));
        for (key, _) in &keys {
            refused.push(format!("{other}+{key}"));
            refused.push(format!("cmd+{other}+{key}"));
        }
    }
    for (key, _) in &keys {
        refused.push(format!("{key}+"));
        refused.push(format!("{key}++"));
        for (modifier, _) in &modifiers {
            refused.push(format!("{key}+{modifier}"));
        }
    }
    for chord in refused
        .iter()
        .flat_map(|chord| [chord.clone(), chord.to_uppercase()])
    {
        assert_eq!(parse_key(&chord), None, "{chord:?}");
    }

    for (list, flags) in [
        (&["cmd", "shift"][..], 1 << 20 | 1 << 17),
        (&["opt", "fn", "bogus"], 1 << 19 | 1 << 23),
        (&[], 0),
    ] {
        let list: Vec<String> = list.iter().map(|m| (*m).to_owned()).collect();
        assert_eq!(modifier_flags(&list), flags, "{list:?}");
    }
}

/// Each action's share of the batch budget covers the engine's pauses.
#[test]
fn costs_cover_the_engine_pauses() {
    let ms = |duration: Duration| u32::try_from(duration.as_millis()).unwrap();
    assert!(CLICK_MS > ms(CLICK_GAP));
    assert!(DRAG_MS > ms(DRAG_HOLD + DRAG_START + DRAG_STEP * DRAG_STEPS));
    assert!(TYPE_MS > ms(TYPE_GAP));
    assert!(SCROLL_MS > ms(SCROLL_SETTLE));
}

/// An absurd wait is refused, never added up.
#[test]
fn an_oversized_wait_is_refused_without_overflowing() {
    let display = Display {
        width: 100.0,
        height: 100.0,
        scale: 1.0,
    };

    let batch = serde_json::json!([
        {"type": "wait", "ms": 1},
        {"type": "wait", "ms": u32::MAX},
    ]);

    assert_eq!(
        Batch::decode(&batch)
            .and_then(|batch| batch.check(&display))
            .unwrap_err()
            .error,
        "Action 2 was refused and nothing was sent: a wait is at most 250 ms"
    );
}

/// A refused action says which of its fields is wrong.
#[test]
fn a_refused_action_names_its_problem() {
    let display = Display {
        width: 100.0,
        height: 100.0,
        scale: 1.0,
    };

    let said = |action: serde_json::Value| {
        Batch::decode(&serde_json::json!([action]))
            .and_then(|batch| batch.check(&display))
            .unwrap_err()
            .error
    };

    assert_eq!(
        said(serde_json::json!({"type": "key", "key": "bogus"})),
        "Action 1 was refused and nothing was sent: the key is not a known name (see atlas key --help)"
    );
    assert_eq!(
        said(serde_json::json!({"type": "scroll", "x": 1, "y": 1, "dy": 200})),
        "Action 1 was refused and nothing was sent: a scroll is at most 100 lines each way"
    );
    assert_eq!(
        said(serde_json::json!({"type": "click", "x": 500, "y": 1})),
        "Action 1 was refused and nothing was sent: its point is off the display"
    );
    assert_eq!(
        said(serde_json::json!({"type": "click", "x": 1, "y": 1, "count": 9})),
        "Action 1 was refused and nothing was sent: a click is 1 to 5 clicks"
    );
    assert_eq!(
        said(serde_json::json!({"type": "click", "x": 1, "y": 1, "modifiers": ["hyper"]})),
        "Action 1 was refused and nothing was sent: a modifier is one of cmd, ctrl, alt, shift and fn"
    );
    assert_eq!(
        said(serde_json::json!({"type": "type", "text": ""})),
        "Action 1 was refused and nothing was sent: text is 1 to 256 characters"
    );
}

/// A referenced click is checked as a click before its element is found
/// again, comes first in its batch, and is charged the time finding it takes.
#[test]
fn a_referenced_click_is_checked_as_a_click_and_comes_first_in_its_batch() {
    let display = Display {
        width: 100.0,
        height: 100.0,
        scale: 1.0,
    };
    let check =
        |batch: &serde_json::Value| Batch::decode(batch).and_then(|batch| batch.check(&display));

    let one = serde_json::json!([{"type": "reference", "ref": "8123abc.12", "count": 2}]);
    check(&one).unwrap();
    let decoded = Batch::decode(&one).unwrap();
    assert!(matches!(
        decoded.actions(),
        [Action::Reference { reference, button: Button::Left, count: 2, modifiers }]
            if reference.to_string() == "8123abc.12" && modifiers.is_empty()
    ));
    assert_eq!(decoded.actions()[0].target(), Target::Nothing);
    assert!(!Kind::Reference.personal());

    for (action, problem) in [
        (
            serde_json::json!({"type": "reference", "ref": "8123abc.12", "count": 0}),
            "a click is 1 to 5 clicks",
        ),
        (
            serde_json::json!({"type": "reference", "ref": "8123abc.12", "modifiers": ["hyper"]}),
            "a modifier is one of cmd, ctrl, alt, shift and fn",
        ),
    ] {
        assert_eq!(
            check(&serde_json::json!([action])).unwrap_err().error,
            format!("Action 1 was refused and nothing was sent: {problem}")
        );
    }

    for later in [
        serde_json::json!([
            {"type": "reference", "ref": "8123abc.12"},
            {"type": "reference", "ref": "8123abc.13"},
        ]),
        serde_json::json!([
            {"type": "key", "key": "return"},
            {"type": "reference", "ref": "8123abc.12"},
        ]),
    ] {
        assert_eq!(
            check(&later).unwrap_err().error,
            "Action 2 was refused and nothing was sent: an action on a ref comes first in its batch"
        );
    }
    let slow = serde_json::json!([
        {"type": "reference", "ref": "8123abc.12"},
        {"type": "wait", "ms": 250},
        {"type": "wait", "ms": 250},
        {"type": "wait", "ms": 250},
        {"type": "wait", "ms": 250},
    ]);
    assert_eq!(
        check(&slow).unwrap_err().error,
        "The batch would take longer than two seconds; send it in parts"
    );

    for malformed in [
        serde_json::json!([{"type": "reference", "ref": "8123abc.012"}]),
        serde_json::json!([{"type": "reference"}]),
        serde_json::json!([{"type": "reference", "ref": "8123abc.1", "x": 1}]),
    ] {
        assert_eq!(check(&malformed).unwrap_err().code, Code::InvalidJson);
    }
}

/// Each action on a referenced element has a wire word of its own, names
/// no point, is never the person's to send, comes first in its batch and
/// is charged the whole time it may take.
#[test]
fn an_action_on_an_element_is_typed_by_its_word_and_checked_with_its_whole_batch() {
    use serde_json::json;
    let display = Display {
        width: 100.0,
        height: 100.0,
        scale: 1.0,
    };
    let check = |batch: &Value| Batch::decode(batch).and_then(|batch| batch.check(&display));
    let one = |action: Value| -> ElementAction {
        let batch = json!([action]);
        check(&batch).unwrap();
        let decoded = Batch::decode(&batch).unwrap();
        let [action] = decoded.actions() else {
            panic!("one action")
        };
        assert_eq!(action.target(), Target::Nothing);
        let (reference, done) = action.element_action().unwrap();
        assert_eq!(Some(reference), action.reference());
        assert_eq!(reference.to_string(), "8123abc12");
        done
    };

    let option = Reference::parse("8123abc14").unwrap();
    for (action, done) in [
        (
            json!({"type": "press", "ref": "8123abc12"}),
            ElementAction::Press,
        ),
        (
            json!({"type": "set-value", "ref": "8123abc12", "value": ""}),
            ElementAction::SetValue(String::new()),
        ),
        (
            json!({"type": "check", "ref": "8123abc12"}),
            ElementAction::Check(true),
        ),
        (
            json!({"type": "uncheck", "ref": "8123abc12"}),
            ElementAction::Check(false),
        ),
        (
            json!({"type": "pick", "ref": "8123abc12", "option": "8123abc14"}),
            ElementAction::Pick(option),
        ),
        (
            json!({"type": "expand", "ref": "8123abc12"}),
            ElementAction::Expand(true),
        ),
        (
            json!({"type": "collapse", "ref": "8123abc12"}),
            ElementAction::Expand(false),
        ),
        (
            json!({"type": "scroll-into-view", "ref": "8123abc12"}),
            ElementAction::ScrollIntoView,
        ),
    ] {
        let kind = || Kind::deserialize(&action).unwrap();
        assert!(!kind().personal(), "{action}");
        assert_eq!(kind().remembered_point(&action), None);
        assert_eq!(one(action.clone()), done);
    }
    let click = json!([{"type": "reference", "ref": "8123abc12"}]);
    assert!(
        Batch::decode(&click).unwrap().actions()[0]
            .element_action()
            .is_none()
    );
    assert!(ACT_MS + 4 * KEY_MS <= BATCH_MS);

    for (action, problem) in [
        (
            json!({"type": "set-value", "ref": "8123abc12", "value": "a".repeat(VALUE_MAX + 1)}),
            "a value is at most 4096 characters",
        ),
        (
            json!({"type": "pick", "ref": "8123abc12", "option": "8123abd14"}),
            "an option is another element of its control's listing",
        ),
        (
            json!({"type": "pick", "ref": "8123abc12", "option": "8123abc12"}),
            "an option is another element of its control's listing",
        ),
    ] {
        assert_eq!(
            check(&json!([action])).unwrap_err().error,
            format!("Action 1 was refused and nothing was sent: {problem}")
        );
    }
    assert_eq!(
        check(&json!([{"type": "key", "key": "tab"}, {"type": "check", "ref": "8123abc12"}]))
            .unwrap_err()
            .error,
        "Action 2 was refused and nothing was sent: an action on a ref comes first in its batch"
    );
    check(&json!([{"type": "check", "ref": "8123abc12"}, {"type": "key", "key": "tab"}])).unwrap();

    for malformed in [
        json!([{"type": "press", "ref": "Save"}]),
        json!([{"type": "press"}]),
        json!([{"type": "press", "ref": "8123abc12", "value": "x"}]),
        json!([{"type": "set-value", "ref": "8123abc12"}]),
        json!([{"type": "pick", "ref": "8123abc12"}]),
        json!([{"type": "setvalue", "ref": "8123abc12", "value": "x"}]),
    ] {
        assert_eq!(
            check(&malformed).unwrap_err().code,
            Code::InvalidJson,
            "{malformed}"
        );
    }
}

#[test]
fn a_verification_is_one_of_three_words() {
    for (verification, word) in [
        (Verification::Matched, "matched"),
        (Verification::Mismatch, "mismatch"),
        (Verification::NotChecked, "not-checked"),
    ] {
        assert_eq!(serde_json::to_value(verification).unwrap(), word);
        assert_eq!(
            Verification::deserialize(Value::from(word)).unwrap(),
            verification
        );
    }
}
