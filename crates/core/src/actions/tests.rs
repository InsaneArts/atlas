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

/// The key validation and engine tables this parser replaced, verbatim.
mod before {
    fn modifier(value: &str) -> bool {
        matches!(
            value,
            "cmd" | "command" | "ctrl" | "control" | "alt" | "opt" | "option" | "shift" | "fn"
        )
    }

    pub(super) fn valid_key(value: &str) -> bool {
        if value.is_empty() || value.len() > 80 {
            return false;
        }

        let lower = value.to_lowercase();
        let (chord, plus) = match lower.strip_suffix("++") {
            Some(rest) => (rest, true),
            None => (lower.as_str(), false),
        };

        let mut parts: Vec<_> = chord.split('+').collect();

        let key = if plus {
            "="
        } else {
            parts.pop().unwrap_or_default()
        };

        if plus && parts.iter().any(|part| part.is_empty()) {
            return false;
        }

        let printable =
            key.len() == 1 && "abcdefghijklmnopqrstuvwxyz0123456789=-][;'\\,/`.".contains(key);
        let function = key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n) && key == format!("f{n}"));

        (printable
            || function
            || matches!(
                key,
                "return"
                    | "enter"
                    | "tab"
                    | "space"
                    | "backspace"
                    | "delete"
                    | "escape"
                    | "esc"
                    | "forwarddelete"
                    | "home"
                    | "pageup"
                    | "end"
                    | "pagedown"
                    | "left"
                    | "right"
                    | "down"
                    | "up"
            ))
            && parts.into_iter().all(modifier)
    }

    pub(super) fn modifier_flags(modifiers: &[String]) -> u64 {
        modifiers.iter().fold(0, |flags, m| {
            flags
                | match m.as_str() {
                    "cmd" | "command" => 1 << 20,
                    "ctrl" | "control" => 1 << 18,
                    "shift" => 1 << 17,
                    "alt" | "opt" | "option" => 1 << 19,
                    "fn" => 1 << 23,
                    _ => 0,
                }
        })
    }

    fn keycode(key: &str) -> Option<u16> {
        Some(match key {
            "a" => 0,
            "s" => 1,
            "d" => 2,
            "f" => 3,
            "h" => 4,
            "g" => 5,
            "z" => 6,
            "x" => 7,
            "c" => 8,
            "v" => 9,
            "b" => 11,
            "q" => 12,
            "w" => 13,
            "e" => 14,
            "r" => 15,
            "y" => 16,
            "t" => 17,
            "1" => 18,
            "2" => 19,
            "3" => 20,
            "4" => 21,
            "6" => 22,
            "5" => 23,
            "=" => 24,
            "9" => 25,
            "7" => 26,
            "-" => 27,
            "8" => 28,
            "0" => 29,
            "]" => 30,
            "o" => 31,
            "u" => 32,
            "[" => 33,
            "i" => 34,
            "p" => 35,
            "return" | "enter" => 36,
            "l" => 37,
            "j" => 38,
            "'" => 39,
            "k" => 40,
            ";" => 41,
            "\\" => 42,
            "," => 43,
            "/" => 44,
            "n" => 45,
            "m" => 46,
            "." => 47,
            "tab" => 48,
            "space" => 49,
            "`" => 50,
            "backspace" | "delete" => 51,
            "escape" | "esc" => 53,
            "f1" => 122,
            "f2" => 120,
            "f3" => 99,
            "f4" => 118,
            "f5" => 96,
            "f6" => 97,
            "f7" => 98,
            "f8" => 100,
            "f9" => 101,
            "f10" => 109,
            "f11" => 103,
            "f12" => 111,
            "home" => 115,
            "pageup" => 116,
            "forwarddelete" => 117,
            "end" => 119,
            "pagedown" => 121,
            "left" => 123,
            "right" => 124,
            "down" => 125,
            "up" => 126,
            _ => return None,
        })
    }

    /// What the engine sent for a key action.
    pub(super) fn engine(key: &str) -> Option<(u16, u64)> {
        let lower = key.to_lowercase();
        let mut parts: Vec<_> = lower.split('+').map(str::to_owned).collect();
        let mut key = parts.pop().unwrap_or_default();
        if key.is_empty() {
            key = "=".into();
            parts.push("shift".into());
        }

        Some((keycode(&key)?, modifier_flags(&parts)))
    }
}

/// Every key name and chord form validates, and sends, exactly what the
/// separate validation and engine tables did.
#[test]
fn the_shared_parser_matches_the_old_tables() {
    let mut keys: Vec<String> = "abcdefghijklmnopqrstuvwxyz0123456789=-][;'\\,/`.+ "
        .chars()
        .map(String::from)
        .collect();
    keys.extend((0..=25).map(|n| format!("f{n}")));
    keys.extend(
        [
            "f01",
            "return",
            "enter",
            "tab",
            "space",
            "backspace",
            "delete",
            "escape",
            "esc",
            "forwarddelete",
            "home",
            "pageup",
            "end",
            "pagedown",
            "left",
            "right",
            "down",
            "up",
            "",
            "foo",
            "zz",
            "bogus",
            "ä",
            "\u{212A}",
        ]
        .map(String::from),
    );

    let modifiers = [
        "cmd", "command", "ctrl", "control", "alt", "opt", "option", "shift", "fn", "", "meta",
        "super",
    ];
    let mut chords = Vec::new();

    for key in &keys {
        chords.push(key.clone());
        chords.push(format!("{key}+"));
        chords.push(format!("{key}++"));

        for first in modifiers {
            chords.push(format!("{first}+{key}"));
            chords.push(format!("{first}++"));
            for second in modifiers {
                chords.push(format!("{first}+{second}+{key}"));
                chords.push(format!("{first}+{second}++"));
            }
        }
    }

    chords.push("x".repeat(81));
    chords.push(format!("{}+a", "cmd+".repeat(19)));

    let mut valid = 0;
    for chord in chords.iter().flat_map(|c| [c.clone(), c.to_uppercase()]) {
        let parsed = parse_key(&chord);

        // The one deliberate difference: "+" on its own is the plus key.
        if chord == "+" {
            assert_eq!(parsed, before::engine("shift+="));
            continue;
        }

        assert_eq!(before::valid_key(&chord), parsed.is_some(), "{chord:?}");

        if parsed.is_some() {
            valid += 1;
            assert_eq!(parsed, before::engine(&chord), "{chord:?}");
        }
    }

    assert!(valid > 10_000, "{valid} valid chords compared");

    for list in [&["cmd", "shift"][..], &["opt", "fn", "bogus"], &[]] {
        let list: Vec<String> = list.iter().map(|m| (*m).to_owned()).collect();
        assert_eq!(modifier_flags(&list), before::modifier_flags(&list));
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
        validate(&batch, &display).unwrap_err().error,
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
        validate(&serde_json::json!([action]), &display)
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
