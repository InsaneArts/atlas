//! The input actions a batch may hold, and the bounds every batch is checked
//! against before anything is sent: coordinates on the display, known keys
//! and modifiers, and a total that finishes within two seconds.
use crate::{
    error::{Error, Result},
    session::Display,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Action {
    Pointer {
        phase: PointerPhase,
        x: f64,
        y: f64,
        #[serde(default)]
        button: Button,
        #[serde(default)]
        modifiers: Vec<String>,
        #[serde(default = "one")]
        count: u8,
    },
    Move {
        x: f64,
        y: f64,
    },
    Click {
        x: f64,
        y: f64,
        #[serde(default)]
        button: Button,
        #[serde(default = "one")]
        count: u8,
        #[serde(default)]
        modifiers: Vec<String>,
    },
    Drag {
        #[serde(rename = "fromX")]
        from_x: f64,
        #[serde(rename = "fromY")]
        from_y: f64,
        #[serde(rename = "toX")]
        to_x: f64,
        #[serde(rename = "toY")]
        to_y: f64,
        #[serde(default)]
        button: Button,
    },
    Scroll {
        x: f64,
        y: f64,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    },
    Type {
        text: String,
    },
    Key {
        key: String,
    },
    Wait {
        ms: u32,
    },
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    #[default]
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerPhase {
    Down,
    Move,
    Up,
}

fn one() -> u8 {
    1
}

/// CoreGraphics modifier masks (`CGEventFlags`), the flags a chord sends.
pub(crate) const SHIFT: u64 = 1 << 17;
pub(crate) const CONTROL: u64 = 1 << 18;
pub(crate) const OPTION: u64 = 1 << 19;
pub(crate) const COMMAND: u64 = 1 << 20;
pub(crate) const FUNCTION: u64 = 1 << 23;

/// The modifier mask a modifier name sets, if it names one.
fn modifier(value: &str) -> Option<u64> {
    Some(match value {
        "cmd" | "command" => COMMAND,
        "ctrl" | "control" => CONTROL,
        "alt" | "opt" | "option" => OPTION,
        "shift" => SHIFT,
        "fn" => FUNCTION,
        _ => return None,
    })
}

/// Five distinct modifier keys exist; longer lists only inflate requests.
fn modifiers_valid(modifiers: &[String]) -> bool {
    modifiers.len() <= 5 && modifiers.iter().all(|m| modifier(m).is_some())
}

/// The flags a validated modifier list sends.
pub(crate) fn modifier_flags(modifiers: &[String]) -> u64 {
    modifiers
        .iter()
        .fold(0, |flags, m| flags | modifier(m).unwrap_or(0))
}

/// The macOS virtual keycode of a lower-case key name.
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

/// A key or chord ("return", "cmd+shift+z", "cmd++") as the keycode and
/// modifier flags it sends, or None when it names no supported key.
pub(crate) fn parse_key(value: &str) -> Option<(u16, u64)> {
    if value.is_empty() || value.len() > 80 {
        return None;
    }

    let lower = value.to_lowercase();
    // "+" and "cmd++" are the plus key: shift and the = key. A chord ending
    // in a bare "+" ("cmd+") names no key at all.
    let bare_plus = lower == "+";

    let (chord, plus) = match lower.strip_suffix("++") {
        Some(rest) => (rest, true),
        None => (lower.as_str(), bare_plus),
    };

    let mut parts: Vec<_> = if bare_plus {
        Vec::new()
    } else {
        chord.split('+').collect()
    };

    let key = if plus {
        "="
    } else {
        parts.pop().unwrap_or_default()
    };

    let code = keycode(key)?;
    parts
        .into_iter()
        .try_fold(if plus { SHIFT } else { 0 }, |flags, part| {
            Some(flags | modifier(part)?)
        })
        .map(|flags| (code, flags))
}

pub fn valid_key(value: &str) -> bool {
    parse_key(value).is_some()
}

/// How long the engine pauses between the clicks of a multi-click.
pub(crate) const CLICK_GAP: Duration = Duration::from_millis(60);
/// How long a drag holds the button before it moves.
pub(crate) const DRAG_HOLD: Duration = Duration::from_millis(80);
/// The points a drag moves through, and the pause after each.
pub(crate) const DRAG_STEPS: u32 = 24;
pub(crate) const DRAG_STEP: Duration = Duration::from_millis(16);
/// How far, at most, the first step of a drag goes on each axis: what is
/// dragged follows the pointer from there.
pub(crate) const DRAG_ANCHOR: f64 = 1.0;
/// The pause after the first step that starts a drag.
pub(crate) const DRAG_START: Duration = Duration::from_millis(10);
/// The most characters one `type` action types.
pub(crate) const TEXT_MAX: usize = 256;
/// The pause after each typed character.
pub(crate) const TYPE_GAP: Duration = Duration::from_millis(2);
/// The pause between moving the pointer to a point and scrolling there.
pub(crate) const SCROLL_SETTLE: Duration = Duration::from_millis(50);

/// The batch budget and each action's share of it, in milliseconds. Each
/// share covers the engine's pauses for that action plus posting time.
const BATCH_MS: u32 = 2000;
const POINTER_MS: u32 = 1;
/// Per click: [`CLICK_GAP`] and its events.
const CLICK_MS: u32 = 70;
/// [`DRAG_HOLD`], [`DRAG_START`] and [`DRAG_STEPS`] × [`DRAG_STEP`], and
/// their events.
const DRAG_MS: u32 = 650;
/// [`SCROLL_SETTLE`] and its events.
const SCROLL_MS: u32 = 60;
/// Per character: [`TYPE_GAP`], its key-down and key-up events, the
/// session checks and the journal write before them: close to 7 ms in all.
const TYPE_MS: u32 = 7;
// The longest text one action types fits one batch.
const _: () = assert!(TYPE_MS * TEXT_MAX as u32 <= BATCH_MS);
const KEY_MS: u32 = 10;

pub fn validate(value: &Value, display: &Display) -> Result<()> {
    let actions = Vec::<Action>::deserialize(value)?;
    if actions.is_empty() || actions.len() > 32 {
        return Err(Error::new("input", "Send between 1 and 32 actions"));
    }

    let point = |x: f64, y: f64| {
        x.is_finite()
            && y.is_finite()
            && x >= 0.0
            && y >= 0.0
            && x < display.width
            && y < display.height
    };

    const OFF_DISPLAY: &str = "its point is off the display";
    const CLICKS: &str = "a click is 1 to 5 clicks";
    const MODIFIERS: &str = "a modifier is one of cmd, ctrl, alt, shift and fn";

    // A pointer press and a click are checked alike.
    let press = |x: f64, y: f64, count: u8, modifiers: &[String]| {
        (!point(x, y))
            .then_some(OFF_DISPLAY)
            .or((!(1..=5).contains(&count)).then_some(CLICKS))
            .or((!modifiers_valid(modifiers)).then_some(MODIFIERS))
    };

    let mut cost: u32 = 0;

    for (index, action) in actions.into_iter().enumerate() {
        // What is wrong with the action, if anything, and its share of the
        // batch budget.
        let (problem, duration) = match action {
            Action::Pointer {
                x,
                y,
                count,
                modifiers,
                ..
            } => (press(x, y, count, &modifiers), POINTER_MS),
            Action::Move { x, y } => ((!point(x, y)).then_some(OFF_DISPLAY), 0),
            Action::Click {
                x,
                y,
                count,
                modifiers,
                ..
            } => (press(x, y, count, &modifiers), u32::from(count) * CLICK_MS),
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                ..
            } => (
                (!(point(from_x, from_y) && point(to_x, to_y))).then_some(OFF_DISPLAY),
                DRAG_MS,
            ),
            Action::Scroll { x, y, dx, dy } => (
                (!point(x, y))
                    .then_some(OFF_DISPLAY)
                    .or(
                        (!((-100..=100).contains(&dx) && (-100..=100).contains(&dy)))
                            .then_some("a scroll is at most 100 lines each way"),
                    ),
                SCROLL_MS,
            ),
            Action::Type { text } => {
                let characters = text.chars().count();
                (
                    (!(1..=TEXT_MAX).contains(&characters))
                        .then_some("text is 1 to 256 characters"),
                    u32::try_from(characters)
                        .unwrap_or(u32::MAX)
                        .saturating_mul(TYPE_MS),
                )
            }
            Action::Key { key } => (
                (!valid_key(&key)).then_some("the key is not a known name (see atlas key --help)"),
                KEY_MS,
            ),
            Action::Wait { ms } => ((ms > 250).then_some("a wait is at most 250 ms"), ms),
        };

        if let Some(problem) = problem {
            return Err(Error::new(
                "input",
                format!(
                    "Action {} was refused and nothing was sent: {problem}",
                    index + 1
                ),
            ));
        }

        cost = cost.saturating_add(duration);

        if cost > BATCH_MS {
            return Err(Error::new(
                "input",
                "The batch would take longer than two seconds; send it in parts",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

        pub fn valid_key(value: &str) -> bool {
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

        pub fn modifier_flags(modifiers: &[String]) -> u64 {
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
        pub fn engine(key: &str) -> Option<(u16, u64)> {
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
}
