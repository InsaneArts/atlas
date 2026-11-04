//! Translate only terminal events the person supplies; nothing reads remote commands.
use super::Command;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde_json::{Value, json};

/// Plain letters are local only while watching. Reserved control chords
/// remain available after typing has been handed to the helper.
pub(super) fn shortcut(
    event: KeyEvent,
    controlling: bool,
    read_only: bool,
    frame: Option<&str>,
) -> Option<Command> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        return match event.code {
            KeyCode::Char('q' | 'Q') => Some(Command::Quit),
            KeyCode::Char('g' | 'G') => Some(Command::GiveBack),
            KeyCode::Char('c' | 'C') if !controlling => Some(Command::Quit),
            KeyCode::Esc if event.modifiers.contains(KeyModifiers::SHIFT) => {
                Some(Command::GiveBack)
            }
            _ => None,
        };
    }
    if controlling
        || event
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    match event.code {
        KeyCode::Char('q' | 'Q') => Some(Command::Quit),
        KeyCode::Char('t' | 'T') if !read_only => frame.map(|frame| Command::Take(frame.into())),
        KeyCode::Char('s' | 'S') if !read_only => Some(Command::Stop),
        _ => None,
    }
}

pub(super) fn modifiers(mods: KeyModifiers) -> Vec<&'static str> {
    [
        (KeyModifiers::CONTROL, "ctrl"),
        (KeyModifiers::ALT, "alt"),
        (KeyModifiers::SUPER, "cmd"),
        (KeyModifiers::SHIFT, "shift"),
    ]
    .into_iter()
    .filter_map(|(flag, name)| mods.contains(flag).then_some(name))
    .collect()
}

pub(super) fn key(event: KeyEvent) -> Option<Value> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let mods = event.modifiers;
    if let KeyCode::Char(character) = event.code
        && !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return Some(json!({"type":"type", "text":character.to_string()}));
    }
    let name = match event.code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
        KeyCode::Char(c) if "-=][';\\,/`.".contains(c) => c.to_string(),
        KeyCode::Enter => "return".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "tab".into(),
        KeyCode::Backspace => "delete".into(),
        KeyCode::Delete => "forwarddelete".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::F(n) if (1..=12).contains(&n) => format!("f{n}"),
        _ => return None,
    };
    let mut held = modifiers(mods);
    if event.code == KeyCode::BackTab && !held.contains(&"shift") {
        held.push("shift");
    }
    held.push(&name);
    Some(json!({"type":"key", "key":held.join("+")}))
}

pub(super) fn mouse(event: MouseEvent, point: (f64, f64)) -> Option<Value> {
    let (x, y) = point;
    let button = |b| match b {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    };
    Some(match event.kind {
        MouseEventKind::Down(b) | MouseEventKind::Up(b) | MouseEventKind::Drag(b) => json!({
            "type":"pointer", "x":x, "y":y, "button":button(b), "modifiers":modifiers(event.modifiers),
            "phase":match event.kind { MouseEventKind::Down(_) => "down", MouseEventKind::Up(_) => "up", _ => "move" }
        }),
        MouseEventKind::Moved => json!({"type":"move", "x":x, "y":y}),
        MouseEventKind::ScrollUp => json!({"type":"scroll", "x":x, "y":y, "dy":-1}),
        MouseEventKind::ScrollDown => json!({"type":"scroll", "x":x, "y":y, "dy":1}),
        MouseEventKind::ScrollLeft => json!({"type":"scroll", "x":x, "y":y, "dx":-1}),
        MouseEventKind::ScrollRight => json!({"type":"scroll", "x":x, "y":y, "dx":1}),
    })
}
