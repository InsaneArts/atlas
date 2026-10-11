use crate::{
    error::{Error, Result},
    session::Display,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
fn modifier(value: &str) -> bool {
    matches!(
        value,
        "cmd" | "command" | "ctrl" | "control" | "alt" | "opt" | "option" | "shift" | "fn"
    )
}

// Five distinct modifier keys exist; longer lists only inflate requests.
fn modifiers_valid(modifiers: &[String]) -> bool {
    modifiers.len() <= 5 && modifiers.iter().all(|m| modifier(m))
}

pub fn valid_key(value: &str) -> bool {
    if value.is_empty() || value.len() > 80 {
        return false;
    }
    let lower = value.to_lowercase();
    // "cmd++" is the plus key, on the = key; a chord ending in a bare "+"
    // ("cmd+") names no key at all.
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

pub fn validate(value: &Value, display: &Display) -> Result<()> {
    let actions: Vec<Action> = serde_json::from_value(value.clone())?;
    let invalid = || {
        Error::new(
            "input",
            "Input exceeds the supported action, coordinate or two-second batch limits",
        )
    };
    if actions.is_empty() || actions.len() > 32 {
        return Err(invalid());
    }
    let point = |x: f64, y: f64| {
        x.is_finite()
            && y.is_finite()
            && x >= 0.0
            && y >= 0.0
            && x < display.width
            && y < display.height
    };
    let mut cost = 0;
    for action in actions {
        let (valid, duration) = match action {
            Action::Pointer {
                x,
                y,
                count,
                modifiers,
                ..
            } => (
                point(x, y) && (1..=5).contains(&count) && modifiers_valid(&modifiers),
                1,
            ),
            Action::Move { x, y } => (point(x, y), 0),
            Action::Click {
                x,
                y,
                count,
                modifiers,
                ..
            } => (
                point(x, y) && (1..=5).contains(&count) && modifiers_valid(&modifiers),
                u32::from(count) * 70,
            ),
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                ..
            } => (point(from_x, from_y) && point(to_x, to_y), 650),
            Action::Scroll { x, y, dx, dy } => (
                point(x, y) && (-100..=100).contains(&dx) && (-100..=100).contains(&dy),
                10,
            ),
            Action::Type { text } => (
                (1..=256).contains(&text.chars().count()),
                text.chars().count() as u32 * 3,
            ),
            Action::Key { key } => (valid_key(&key), 10),
            Action::Wait { ms } => (ms <= 250, ms),
        };
        cost += duration;
        if !valid || cost > 2000 {
            return Err(invalid());
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
        ] {
            assert!(valid_key(key), "{key}");
        }
        for key in ["foo", "cmd+zz", "cmd+", "+", "cmd+bogus+s", "f13", ""] {
            assert!(!valid_key(key), "{key}");
        }
    }
}
