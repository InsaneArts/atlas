use super::*;

fn refused(text: &str) -> Refusal {
    guard(text.as_bytes()).unwrap_err()
}

#[test]
fn ordinary_metadata_passes() {
    for text in [
        "{}",
        r#" {"creditID": 1, "ref": {"frameKey": "ab"}, "list": [1, -2.5e3, true, null, "x"]} "#,
        r#"{"text": "line\nbreak é 😀 é"}"#,
    ] {
        assert!(guard(text.as_bytes()).is_ok(), "{text}");
    }
}

#[test]
fn duplicate_keys_are_refused_however_they_are_spelled() {
    for text in [
        r#"{"a": 1, "a": 2}"#,
        r#"{"a": 1, "a": 2}"#,
        r#"{"é": 1, "é": 2}"#,
        r#"{"x": {"a": 1, "a": 2}}"#,
    ] {
        assert_eq!(refused(text), Refusal::Invalid, "{text}");
    }
    assert!(
        guard(br#"{"a": {"a": 1}, "b": {"a": 2}}"#).is_ok(),
        "one key in each object"
    );
}

#[test]
fn malformed_text_and_escapes_are_refused() {
    for text in [
        "",
        "[]",
        "\"just a string\"",
        "{",
        r#"{"a" 1}"#,
        r#"{"a": 1,}"#,
        r#"{"a": 01}"#,
        r#"{"a": 1.}"#,
        r#"{"a": .5}"#,
        r#"{"a": +1}"#,
        r#"{"a": tru}"#,
        r#"{"a": "\x"}"#,
        r#"{"a": "\ud800"}"#,
        r#"{"a": "\udc00"}"#,
        r#"{"a": "\ud800A"}"#,
        "{\"a\": \"raw\u{1}control\"}",
        r#"{} {}"#,
        r#"{} x"#,
    ] {
        assert_eq!(refused(text), Refusal::Invalid, "{text:?}");
    }
    assert_eq!(
        guard(&[b'{', b'"', 0xff, b'"', b':', b'1', b'}']),
        Err(Refusal::Invalid)
    );
}

#[test]
fn every_bound_is_refused_one_past_its_limit() {
    let nested =
        |depth: usize| format!("{}{}", "{\"a\":".repeat(depth - 1) + "{", "}".repeat(depth));
    assert!(guard(nested(DEPTH_MAX).as_bytes()).is_ok());
    assert_eq!(refused(&nested(DEPTH_MAX + 1)), Refusal::Oversized);

    let keys = |count: usize| {
        let body: Vec<String> = (0..count).map(|n| format!("\"k{n}\":0")).collect();
        format!("{{{}}}", body.join(","))
    };
    assert!(guard(keys(KEYS_MAX).as_bytes()).is_ok());
    assert_eq!(refused(&keys(KEYS_MAX + 1)), Refusal::Oversized);

    let key = |len: usize| format!("{{\"{}\":0}}", "k".repeat(len));
    assert!(guard(key(KEY_MAX).as_bytes()).is_ok());
    assert_eq!(refused(&key(KEY_MAX + 1)), Refusal::Oversized);

    let string = |len: usize| format!("{{\"s\":\"{}\"}}", "é".repeat(len / 2));
    assert!(guard(string(STRING_MAX).as_bytes()).is_ok());
    assert_eq!(
        refused(&string(STRING_MAX + 2)),
        Refusal::Oversized,
        "decoded bytes count"
    );
}
