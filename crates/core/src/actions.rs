//! The input actions a batch may hold, the app a batch may be for, and the
//! bounds every batch is checked against before anything is sent:
//! coordinates on the display, known keys and modifiers, and a total that
//! finishes within two seconds. An action on a referenced element is done
//! through Accessibility alone, and its reply says whether it reads as asked.
use crate::{
    error::{Code, Error, Result},
    screen::elements::Reference,
    session::Display,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// One declaration owns wire names and personal-input permission, including
/// for malformed fields that must retain their later admission order.
macro_rules! actions {
    ($($kind:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? } => $personal:expr),* $(,)?) => {
        #[derive(Debug, Deserialize)]
        #[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
        pub enum Action {
            $($kind { $($(#[$attr])* $field: $ty),* }),*
        }

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "kebab-case")]
        pub enum Kind { $($kind),* }

        impl Kind {
            pub fn personal(self) -> bool {
                match self { $(Self::$kind => $personal),* }
            }
        }
    };
}

actions! {
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
    } => true,
    Move {
        x: f64,
        y: f64,
    } => true,
    Click {
        x: f64,
        y: f64,
        #[serde(default)]
        button: Button,
        #[serde(default = "one")]
        count: u8,
        #[serde(default)]
        modifiers: Vec<String>,
    } => false,
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
    } => false,
    Scroll {
        x: f64,
        y: f64,
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    } => true,
    Type {
        text: String,
    } => true,
    Key {
        key: String,
    } => true,
    Wait {
        ms: u32,
    } => false,
    // A click on the element a reference names, found again first.
    Reference {
        #[serde(rename = "ref")]
        reference: Reference,
        #[serde(default)]
        button: Button,
        #[serde(default = "one")]
        count: u8,
        #[serde(default)]
        modifiers: Vec<String>,
    } => false,
    // The actions done to a referenced element through Accessibility.
    Press {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
    SetValue {
        #[serde(rename = "ref")]
        reference: Reference,
        value: String,
    } => false,
    Check {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
    Uncheck {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
    Pick {
        #[serde(rename = "ref")]
        reference: Reference,
        option: Reference,
    } => false,
    Expand {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
    Collapse {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
    ScrollIntoView {
        #[serde(rename = "ref")]
        reference: Reference,
    } => false,
}

/// What an action does to a referenced element through Accessibility: no
/// pointer or key event, and no request for focus. A state is asked for,
/// never toggled: an element already in it is left as it is.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementAction {
    Press,
    /// Set its value to this text, or to the number it reads as.
    SetValue(String),
    /// Leave it checked, or unchecked.
    Check(bool),
    /// Select this element of it: one of its options.
    Pick(Reference),
    /// Leave it expanded, or collapsed.
    Expand(bool),
    ScrollIntoView,
}

/// Whether an element reads back as an action on it asked: `not-checked`
/// when nothing it reports can tell, as after a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verification {
    Matched,
    Mismatch,
    NotChecked,
}

/// The destination an action must reach before it can send input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Keys,
    Point(f64, f64),
    /// A retained pointer gesture checks its point without consulting window memory.
    Pointer(f64, f64),
    /// A drag, and the button it holds: only the left one moves a window by
    /// its title bar.
    Drag {
        from: (f64, f64),
        to: (f64, f64),
        button: Button,
    },
    Nothing,
}

impl Action {
    pub fn target(&self) -> Target {
        match self {
            Self::Type { .. } | Self::Key { .. } => Target::Keys,
            Self::Click { x, y, .. } | Self::Scroll { x, y, .. } | Self::Move { x, y } => {
                Target::Point(*x, *y)
            }
            Self::Pointer { x, y, .. } => Target::Pointer(*x, *y),
            Self::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                button,
            } => Target::Drag {
                from: (*from_x, *from_y),
                to: (*to_x, *to_y),
                button: *button,
            },
            // A reference names no point until the input service finds its
            // element again; it is then checked as the click it becomes, and
            // one never found again is never posted. An action done through
            // Accessibility sends no event and needs no focus.
            Self::Wait { .. }
            | Self::Reference { .. }
            | Self::Press { .. }
            | Self::SetValue { .. }
            | Self::Check { .. }
            | Self::Uncheck { .. }
            | Self::Pick { .. }
            | Self::Expand { .. }
            | Self::Collapse { .. }
            | Self::ScrollIntoView { .. } => Target::Nothing,
        }
    }

    /// The element it names by reference, when it names one.
    pub fn reference(&self) -> Option<Reference> {
        match self {
            Self::Reference { reference, .. }
            | Self::Press { reference }
            | Self::SetValue { reference, .. }
            | Self::Check { reference }
            | Self::Uncheck { reference }
            | Self::Pick { reference, .. }
            | Self::Expand { reference }
            | Self::Collapse { reference }
            | Self::ScrollIntoView { reference } => Some(*reference),
            Self::Pointer { .. }
            | Self::Move { .. }
            | Self::Click { .. }
            | Self::Drag { .. }
            | Self::Scroll { .. }
            | Self::Type { .. }
            | Self::Key { .. }
            | Self::Wait { .. } => None,
        }
    }

    /// The element it acts on through Accessibility and what it does to
    /// it; None for an action that sends events.
    pub fn element_action(&self) -> Option<(Reference, ElementAction)> {
        let action = match self {
            Self::Press { .. } => ElementAction::Press,
            Self::SetValue { value, .. } => ElementAction::SetValue(value.clone()),
            Self::Check { .. } => ElementAction::Check(true),
            Self::Uncheck { .. } => ElementAction::Check(false),
            Self::Pick { option, .. } => ElementAction::Pick(*option),
            Self::Expand { .. } => ElementAction::Expand(true),
            Self::Collapse { .. } => ElementAction::Expand(false),
            Self::ScrollIntoView { .. } => ElementAction::ScrollIntoView,
            Self::Pointer { .. }
            | Self::Move { .. }
            | Self::Click { .. }
            | Self::Drag { .. }
            | Self::Scroll { .. }
            | Self::Type { .. }
            | Self::Key { .. }
            | Self::Wait { .. }
            | Self::Reference { .. } => return None,
        };
        Some((self.reference()?, action))
    }
}

impl Kind {
    /// A malformed action can still name a point in remembered windows.
    /// Other field failures must wait behind that inference and admission.
    pub fn remembered_point(self, wire: &Value) -> Option<(f64, f64)> {
        let (x, y) = match self {
            Self::Click | Self::Scroll | Self::Move => ("x", "y"),
            Self::Drag => ("fromX", "fromY"),
            Self::Pointer
            | Self::Type
            | Self::Key
            | Self::Wait
            | Self::Reference
            | Self::Press
            | Self::SetValue
            | Self::Check
            | Self::Uncheck
            | Self::Pick
            | Self::Expand
            | Self::Collapse
            | Self::ScrollIntoView => return None,
        };
        Some((wire[x].as_f64()?, wire[y].as_f64()?))
    }
}

/// Decoded actions and their original wire projection. Only the projection
/// retains optional-field presence; decisions use the decoded actions.
#[derive(Debug)]
pub struct Batch<'a> {
    actions: Vec<Action>,
    wire: &'a Value,
}

impl<'a> Batch<'a> {
    /// # Errors
    ///
    /// Returns an error when `value` is not a list of actions.
    pub fn decode(value: &'a Value) -> Result<Self> {
        Ok(Self {
            actions: Vec::deserialize(value)?,
            wire: value,
        })
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Validates the whole batch on `display` without sending any of it.
    ///
    /// # Errors
    ///
    /// Rejects unsupported keys or modifiers, off-display points, and
    /// batches that exceed the action or duration limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use atlas_core::{actions::Batch, session::Display};
    /// let display = Display { width: 800.0, height: 600.0, scale: 1.0 };
    /// let batch = serde_json::json!([{"type":"move","x":10,"y":20}]);
    /// Batch::decode(&batch)?.check(&display)?;
    /// # Ok::<(), atlas_core::error::Error>(())
    /// ```
    pub fn check(&self, display: &Display) -> Result<()> {
        check(&self.actions, display)
    }

    pub fn wire(&self) -> &'a Value {
        self.wire
    }
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

/// The longest field name an expectation may carry, in characters: longer
/// than any label `atlas elements` shows whole.
const FIELD_MAX: usize = 256;

/// The app an agent's batch is for: its process in the helper and, when
/// named, the window and the field that should have keyboard focus. A
/// batch that names one is sent only while each action would reach it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Expect {
    pid: i32,
    /// The kernel start time read in the helper snapshot that resolved the app.
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<crate::process::Start>,
    /// The window's number, as the window list gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<u32>,
    /// The field's name, as `atlas elements --name` matches it.
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
}

impl Expect {
    /// The expectation for process `pid`, refused unless it can be one.
    ///
    /// # Errors
    ///
    /// Rejects a process ID outside the positive 32-bit range. This validates a name; it does
    /// not prove that the process still exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use atlas_core::actions::Expect;
    /// let expected = Expect::new(812)?.in_window(42)?.in_field("Search")?;
    /// assert_eq!(expected.pid(), 812);
    /// # Ok::<(), atlas_core::error::Error>(())
    /// ```
    pub fn new(pid: i64) -> Result<Self> {
        i32::try_from(pid)
            .ok()
            .filter(|pid| *pid > 0)
            .map(|pid| Self {
                pid,
                start: None,
                window: None,
                field: None,
            })
            .ok_or_else(not_a_process)
    }

    /// The expectation that window `number` of the app has keyboard focus
    /// (or, for a pointer action, is the one at the point), refused unless
    /// it can be a window number.
    ///
    /// # Errors
    ///
    /// Rejects a window number outside the positive 32-bit range. Window ownership and
    /// freshness are checked when input is admitted.
    pub fn in_window(self, number: i64) -> Result<Self> {
        let window = u32::try_from(number)
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(not_a_window)?;
        Ok(Self {
            window: Some(window),
            ..self
        })
    }

    /// The expectation that the focused field is named `name`, refused
    /// unless it names something.
    ///
    /// # Errors
    ///
    /// Rejects a blank field name or one longer than the supported character limit.
    pub fn in_field(self, name: &str) -> Result<Self> {
        let length = name.chars().count();
        if name.trim().is_empty() || length > FIELD_MAX {
            return Err(not_a_field());
        }

        Ok(Self {
            field: Some(name.to_owned()),
            ..self
        })
    }

    /// A request's expectation: none when absent or null. Anything but
    /// `{"pid": N}` naming a process, with an optional start time, window
    /// number and field name, is refused, each member with its own error,
    /// so a member this build does not know is never ignored.
    ///
    /// # Errors
    ///
    /// Rejects unknown members, malformed identity fields, invalid window numbers and blank or
    /// oversized field names. A null value means no expectation.
    pub fn from_request(value: &Value) -> Result<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }

        let members = value.as_object().ok_or_else(not_a_process)?;
        if let Some(unknown) = members
            .keys()
            .find(|key| !["pid", "start", "window", "field"].contains(&key.as_str()))
        {
            return Err(Error::new(
                Code::Input,
                format!("\"expect\" has a member this build does not check: {unknown}"),
            ));
        }

        let pid = members
            .get("pid")
            .and_then(Value::as_i64)
            .ok_or_else(not_a_process)?;
        let mut expect = Self::new(pid)?;
        // Older controllers omit the start time. A supplied time must be exact.
        if let Some(start) = members.get("start").filter(|value| !value.is_null()) {
            let invalid =
                || Error::new(Code::Input, "\"expect.start\" must be a process start time");
            let start = crate::process::Start::deserialize(start)
                .map_err(|error| invalid().with_source(error))?;
            if start[0] == 0 || start[1] >= 1_000_000 {
                return Err(invalid());
            }
            expect.start = Some(start);
        }
        if let Some(window) = members.get("window") {
            expect = expect.in_window(window.as_i64().ok_or_else(not_a_window)?)?;
        }
        if let Some(field) = members.get("field") {
            expect = expect.in_field(field.as_str().ok_or_else(not_a_field)?)?;
        }

        Ok(Some(expect))
    }

    /// The process the batch is for.
    pub fn pid(&self) -> i64 {
        self.pid.into()
    }

    /// Carry the instance checked while resolving the app, without rereading it.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance's process ID can't be expected.
    pub fn from_instance(instance: crate::process::Instance) -> Result<Self> {
        let mut expect = Self::new(i64::from(instance.pid))?;
        expect.start = Some(instance.start);
        Ok(expect)
    }

    pub fn instance(&self) -> Option<crate::process::Instance> {
        self.start
            .map(|start| crate::process::Instance::new(self.pid, start))
    }

    /// The window that should have keyboard focus, if one is named.
    pub fn window(&self) -> Option<i64> {
        self.window.map(i64::from)
    }

    /// The name of the field that should have keyboard focus, if one is
    /// named.
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// The expectation of the app alone: for a key after the batch's
    /// first, whose window or field an earlier key (tab, return, cmd+w)
    /// may have moved on purpose.
    pub fn app_only(&self) -> Self {
        Self {
            pid: self.pid,
            start: self.start,
            window: None,
            field: None,
        }
    }
}

fn not_a_process() -> Error {
    Error::new(Code::Input, "\"expect\" must name a process ID")
}

fn not_a_window() -> Error {
    Error::new(Code::Input, "\"window\" must be a window number")
}

fn not_a_field() -> Error {
    Error::new(
        Code::Input,
        format!("\"field\" must be a name of 1 to {FIELD_MAX} characters"),
    )
}

/// A modifier's aliases, CoreGraphics mask and release key share one declaration.
macro_rules! modifiers {
    ($($flag:ident = $mask:expr, $code:literal => $($name:literal)|+);+ $(;)?) => {
        $(pub const $flag: u64 = $mask;)+

        #[cfg(target_os = "macos")]
        pub const MODIFIER_KEYS: &[(u16, u64)] = &[$(($code, $flag)),+];

        fn modifier(value: &str) -> Option<u64> {
            match value { $($($name)|+ => Some($flag),)+ _ => None }
        }
    };
}

modifiers! {
    COMMAND = 1 << 20, 55 => "cmd" | "command";
    SHIFT = 1 << 17, 56 => "shift";
    OPTION = 1 << 19, 58 => "alt" | "opt" | "option";
    CONTROL = 1 << 18, 59 => "ctrl" | "control";
    FUNCTION = 1 << 23, 63 => "fn";
}

/// Five distinct modifier keys exist; longer lists only inflate requests.
fn modifiers_valid(modifiers: &[String]) -> bool {
    modifiers.len() <= 5 && modifiers.iter().all(|m| modifier(m).is_some())
}

/// The flags a validated modifier list sends.
pub fn modifier_flags(modifiers: &[String]) -> u64 {
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
pub fn parse_key(value: &str) -> Option<(u16, u64)> {
    if value.is_empty() || value.len() > 80 {
        return None;
    }

    let lower = value.to_lowercase();
    // The bare plus and a doubled chord suffix mean shift and the = key.
    if lower == "+" {
        return Some((keycode("=")?, SHIFT));
    }
    let (chord, key, flags) = if let Some(chord) = lower.strip_suffix("++") {
        (chord, "=", SHIFT)
    } else if let Some((chord, key)) = lower.rsplit_once('+') {
        (chord, key, 0)
    } else {
        return Some((keycode(&lower)?, 0));
    };
    let code = keycode(key)?;
    let flags = chord
        .split('+')
        .try_fold(flags, |flags, part| Some(flags | modifier(part)?))?;
    Some((code, flags))
}

pub fn valid_key(value: &str) -> bool {
    parse_key(value).is_some()
}

/// How long the engine pauses between the clicks of a multi-click.
pub const CLICK_GAP: Duration = Duration::from_millis(60);
/// How long a drag holds the button before it moves.
pub const DRAG_HOLD: Duration = Duration::from_millis(80);
/// The points a drag moves through, and the pause after each.
pub const DRAG_STEPS: u32 = 24;
pub const DRAG_STEP: Duration = Duration::from_millis(16);
/// How far, at most, the first step of a drag goes on each axis: what is
/// dragged follows the pointer from there.
pub const DRAG_ANCHOR: f64 = 1.0;
/// The pause after the first step that starts a drag.
pub const DRAG_START: Duration = Duration::from_millis(10);
/// The most characters one `type` action types.
pub const TEXT_MAX: usize = 256;
/// The pause after each typed character.
pub const TYPE_GAP: Duration = Duration::from_millis(2);
/// The pause between moving the pointer to a point and scrolling there.
pub const SCROLL_SETTLE: Duration = Duration::from_millis(50);
/// The longest the input service reads an app to find a referenced
/// element again: once before its app is brought forward, and once more
/// just before its click.
pub const RESOLVE: Duration = Duration::from_millis(RESOLVE_MS as u64);
/// The longest one action on a referenced element through Accessibility
/// takes in all: finding it again twice, reading what it supports and its
/// state, the action itself and reading it back.
pub const ACT: Duration = Duration::from_millis(ACT_MS as u64);
/// The most characters a value set on an element holds.
pub const VALUE_MAX: usize = 4096;

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
/// [`RESOLVE`], spent twice by a referenced click.
const RESOLVE_MS: u32 = 500;
// The longest referenced click fits one batch.
const _: () = assert!(2 * RESOLVE_MS + 5 * CLICK_MS <= BATCH_MS);
/// [`ACT`].
const ACT_MS: u32 = 1000;

/// # Errors
///
/// Returns the refusal of the batch, naming its first bad action.
pub fn check(actions: &[Action], display: &Display) -> Result<()> {
    if actions.is_empty() || actions.len() > 32 {
        return Err(Error::new(Code::Input, "Send between 1 and 32 actions"));
    }

    let mut cost: u32 = 0;

    for (index, action) in actions.iter().enumerate() {
        let (problem, duration) = action.assess(display);
        // A reference found stale after earlier actions were sent would be
        // refused too late to send nothing, so it comes first.
        let problem = problem.or(unless(
            index == 0 || action.reference().is_none(),
            "an action on a ref comes first in its batch",
        ));

        if let Some(problem) = problem {
            return Err(Error::new(
                Code::Input,
                format!(
                    "Action {} was refused and nothing was sent: {problem}",
                    index + 1
                ),
            ));
        }

        cost = cost.saturating_add(duration);

        if cost > BATCH_MS {
            return Err(Error::new(
                Code::Input,
                "The batch would take longer than two seconds; send it in parts",
            ));
        }
    }

    Ok(())
}

/// `problem`, unless what it denies holds.
fn unless(holds: bool, problem: &'static str) -> Option<&'static str> {
    (!holds).then_some(problem)
}

impl Action {
    /// What is wrong with the action on `display`, if anything, and its
    /// share of the batch budget.
    fn assess(&self, display: &Display) -> (Option<&'static str>, u32) {
        const OFF_DISPLAY: &str = "its point is off the display";

        let point = |x: f64, y: f64| {
            x.is_finite()
                && y.is_finite()
                && x >= 0.0
                && y >= 0.0
                && x < display.width
                && y < display.height
        };

        // A pointer press, a click and a referenced click are checked alike.
        let clicks = |count: u8, modifiers: &[String]| {
            unless((1..=5).contains(&count), "a click is 1 to 5 clicks").or(unless(
                modifiers_valid(modifiers),
                "a modifier is one of cmd, ctrl, alt, shift and fn",
            ))
        };
        let press = |x: f64, y: f64, count: u8, modifiers: &[String]| {
            unless(point(x, y), OFF_DISPLAY).or(clicks(count, modifiers))
        };

        match self {
            Self::Pointer {
                x,
                y,
                count,
                modifiers,
                ..
            } => (press(*x, *y, *count, modifiers), POINTER_MS),
            Self::Move { x, y } => (unless(point(*x, *y), OFF_DISPLAY), 0),
            Self::Click {
                x,
                y,
                count,
                modifiers,
                ..
            } => (
                press(*x, *y, *count, modifiers),
                u32::from(*count) * CLICK_MS,
            ),
            Self::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                ..
            } => (
                unless(point(*from_x, *from_y) && point(*to_x, *to_y), OFF_DISPLAY),
                DRAG_MS,
            ),
            Self::Scroll { x, y, dx, dy } => (
                unless(point(*x, *y), OFF_DISPLAY).or(unless(
                    (-100..=100).contains(dx) && (-100..=100).contains(dy),
                    "a scroll is at most 100 lines each way",
                )),
                SCROLL_MS,
            ),
            Self::Type { text } => {
                let characters = text.chars().count();
                (
                    unless(
                        (1..=TEXT_MAX).contains(&characters),
                        "text is 1 to 256 characters",
                    ),
                    u32::try_from(characters)
                        .unwrap_or(u32::MAX)
                        .saturating_mul(TYPE_MS),
                )
            }
            Self::Key { key } => (
                unless(
                    valid_key(key),
                    "the key is not a known name (see atlas key --help)",
                ),
                KEY_MS,
            ),
            Self::Wait { ms } => (unless(*ms <= 250, "a wait is at most 250 ms"), *ms),
            // Its point is checked once its element is found again.
            Self::Reference {
                count, modifiers, ..
            } => (
                clicks(*count, modifiers),
                2 * RESOLVE_MS + u32::from(*count) * CLICK_MS,
            ),
            Self::SetValue { value, .. } => (
                unless(
                    value.chars().count() <= VALUE_MAX,
                    "a value is at most 4096 characters",
                ),
                ACT_MS,
            ),
            Self::Pick { reference, option } => (
                unless(
                    option.look == reference.look && option.index != reference.index,
                    "an option is another element of its control's listing",
                ),
                ACT_MS,
            ),
            Self::Press { .. }
            | Self::Check { .. }
            | Self::Uncheck { .. }
            | Self::Expand { .. }
            | Self::Collapse { .. }
            | Self::ScrollIntoView { .. } => (None, ACT_MS),
        }
    }
}

#[cfg(test)]
mod tests;
