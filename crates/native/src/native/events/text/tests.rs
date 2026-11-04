use super::*;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventType,
};
use std::cell::Cell;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn translated(text: &str) -> Translation {
    Translation {
        units: units(text),
        dead_key_state: 0,
    }
}
fn layout(code: u16, modifiers: u32) -> Option<Translation> {
    let text = match (code, modifiers) {
        (0, 0) => "a",
        (0, 2) => "A",
        (11, 0) => "b",
        (11, 2) => "B",
        (18, 0) => "1",
        (18, 2) => "!",
        (24, 0) => "=",
        (24, 2) => "+",
        (49, 0) => " ",
        (7, 8) => "≈",
        (7, 10) => "˛",
        _ => return None,
    };
    Some(translated(text))
}

#[test]
fn letters_case_digits_and_punctuation_match_physical_codes_and_flags() {
    let plan = Plan(Some(Arc::new(reverse_map(layout))));
    for (text, expected) in [
        ("a", (0, 0)),
        ("A", (0, SHIFT)),
        ("b", (11, 0)),
        ("B", (11, SHIFT)),
        ("1", (18, 0)),
        ("!", (18, SHIFT)),
        ("=", (24, 0)),
        ("+", (24, SHIFT)),
        (" ", (49, 0)),
    ] {
        assert_eq!(plan.key(&units(text)), expected, "{text}");
    }
}

#[test]
fn the_current_layout_decides_positions_and_the_least_modified_match_wins() {
    let mapping = reverse_map(|code, modifiers| match (code, modifiers) {
        (45, 0) | (11, 2) | (7, 8) => Some(translated("b")),
        (45, 2) => Some(translated("B")),
        _ => None,
    });
    let plan = Plan(Some(Arc::new(mapping)));
    assert_eq!(plan.key(&units("b")), (45, 0));
    assert_eq!(plan.key(&units("B")), (45, SHIFT));
}

#[test]
fn dead_keys_controls_invalid_utf16_and_modifier_positions_are_not_text_chords() {
    let mapping = reverse_map(|code, _| {
        Some(match code {
            14 => Translation {
                units: units("´"),
                dead_key_state: 1,
            },
            11 => translated("\n"),
            12 => Translation {
                units: vec![0xd800],
                dead_key_state: 0,
            },
            55 => translated("x"),
            57 => translated("y"),
            _ => return None,
        })
    });
    assert!(mapping.is_empty());
}

#[test]
fn unmapped_unicode_preserves_the_original_payload_and_fallback_key() {
    let plan = Plan(Some(Arc::new(reverse_map(layout))));
    for text in ["🙂", "ე", "e\u{301}", "\n"] {
        let original = units(text);
        assert_eq!(plan.key(&original), (0, 0));
        assert_eq!(original, units(text));
    }
    assert_eq!(Plan::default().key(&units("b")), (0, 0));
}

#[test]
fn option_only_characters_keep_unicode_fallback_instead_of_synthesizing_meta() {
    let plan = Plan(Some(Arc::new(reverse_map(layout))));
    for text in ["≈", "˛"] {
        let original = units(text);
        assert_eq!(plan.key(&original), (0, 0));
        assert_eq!(original, units(text));
    }
}

#[test]
fn layout_bytes_and_keyboard_type_control_the_bounded_mapping_cache() {
    let mut cache = Cache::default();
    let calls = Cell::new(0);
    let mut translate = |code, modifiers| {
        calls.set(calls.get() + 1);
        layout(code, modifiers)
    };
    let first = cache.mapping(vec![1, 2, 3], 40, &mut translate);
    let count = calls.get();
    assert!(count > 0);
    let same = cache.mapping(vec![1, 2, 3], 40, &mut translate);
    assert!(Arc::ptr_eq(&first, &same));
    assert_eq!(calls.get(), count);
    let changed = cache.mapping(vec![1, 2, 4], 40, &mut translate);
    assert!(!Arc::ptr_eq(&first, &changed));
    assert!(calls.get() > count);
    let changed_type = cache.mapping(vec![1, 2, 4], 41, &mut translate);
    assert!(!Arc::ptr_eq(&changed, &changed_type));
}

#[test]
#[allow(
    unsafe_code,
    reason = "FFI: inspect synthetic unposted CGEvent UTF-16 payloads"
)]
fn planned_key_events_retain_their_actual_code_flags_and_unicode_without_posting() {
    let plan = Plan(Some(Arc::new(reverse_map(layout))));
    let source = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
    for text in ["b", "B", "!", "🙂"] {
        let original = units(text);
        let (code, flags) = plan.key(&original);
        for down in [true, false] {
            let event = CGEvent::new_keyboard_event(Some(&source), code, down).unwrap();
            // SAFETY: the original UTF-16 buffer remains live for this copy.
            unsafe {
                CGEvent::keyboard_set_unicode_string(
                    Some(&event),
                    original.len() as _,
                    original.as_ptr(),
                );
            }
            CGEvent::set_flags(Some(&event), CGEventFlags(if down { flags } else { 0 }));
            assert_eq!(
                CGEvent::integer_value_field(Some(&event), CGEventField::KeyboardEventKeycode),
                i64::from(code)
            );
            assert_eq!(
                CGEvent::r#type(Some(&event)),
                if down {
                    CGEventType::KeyDown
                } else {
                    CGEventType::KeyUp
                }
            );
            assert_eq!(CGEvent::flags(Some(&event)).0, if down { flags } else { 0 });
            let mut payload = [0_u16; 255];
            let mut length = 0;
            // SAFETY: length and payload point to initialized output storage
            // with the declared capacity; the synthetic event stays alive.
            unsafe {
                CGEvent::keyboard_get_unicode_string(
                    Some(&event),
                    payload.len() as _,
                    &mut length,
                    payload.as_mut_ptr(),
                );
            }
            assert_eq!(&payload[..length as usize], original);
        }
    }
}
