//! What an agent's command did, as the activity log keeps it and every
//! surface says it: a verb, the app it acted in and the control it named.
//! Typed text, argv, coordinates, output, file names and window titles are
//! never kept.
pub use crate::name::capitalized;
use crate::{actions::Button, activity_log::Row, error::Code, name::bounded};

/// The most chords a key press names; more read as "pressed keys".
const CHORD_LIMIT: usize = 3;
/// How many characters of a random token tell one command's rows apart.
const COMMAND_ID: usize = 12;
/// The most characters a bundle identifier keeps; a longer one is no
/// identifier.
const BUNDLE_ID_LIMIT: usize = 155;
/// The signs of the modifiers a key press's label shows before its key,
/// each a keycap of its own.
const MODIFIER_SIGNS: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];
/// fn's sign in a label: a word, set apart from the key it holds.
const FN_SIGN: &str = "fn";
/// Keys whose name is two words, kept as one keycap.
const TWO_WORD_KEYS: [&str; 2] = ["Page Up", "Page Down"];
/// Roles whose name is a control's own caption, such as a button's title.
/// Others may show a document's data: menu items name windows and recent
/// files, tabs and radio buttons name pages, pop-ups name folders.
const CONTROL_ROLES: [&str; 4] = [
    "AXButton",
    "AXCheckBox",
    "AXMenuBarItem",
    "AXDisclosureTriangle",
];
/// Roles of a field whose name is its own caption, such as "Email" or
/// "Search": a typed row names the field by it once the helper confirmed
/// the text went there. A text area's name may be a document's, and so is
/// left out, as is every other role.
pub(crate) const FIELD_ROLES: [&str; 3] = ["AXTextField", "AXSearchField", "AXComboBox"];
/// Verbs worded as their operation is.
const PLAIN_VERBS: [&str; 2] = ["run", "screenshot"];
/// Modifier names as `atlas key` takes them, in the order macOS menus show
/// them, with their symbols and whether they make a shortcut: ⌥, fn and
/// shift chords type characters too.
const MODIFIERS: [(&[&str], &str, bool); 5] = [
    (&["fn"], "fn ", false),
    (&["ctrl", "control"], "⌃", true),
    (&["alt", "opt", "option"], "⌥", false),
    (&["shift"], "⇧", false),
    (&["cmd", "command"], "⌘", true),
];

/// One command's verb, with the app and control it named when it named
/// one. Every row the command records shares its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doing {
    verb: &'static str,
    app: Option<String>,
    label: Option<String>,
    /// Whether the label names what was asked for whatever happened (a
    /// shortcut), not a name that matched something (a control's caption).
    asked: bool,
    /// The field text was meant for, by its name; a row names it only once
    /// the helper confirmed the text went to a field of [`FIELD_ROLES`].
    field: Option<String>,
    command: String,
}

impl Doing {
    /// A command doing `verb`, naming nothing yet.
    pub fn new(verb: &'static str) -> Self {
        let mut command = crate::files::token();
        command.truncate(COMMAND_ID);
        Self {
            verb,
            app: None,
            label: None,
            asked: false,
            field: None,
            command,
        }
    }

    /// The app it acted in: a name, or the name of an app bundle's path.
    pub fn app(mut self, app: Option<&str>) -> Self {
        self.app = app.and_then(app_name).map(bounded);
        self
    }

    /// The control it named, such as a button's title.
    pub fn label(mut self, label: Option<&str>) -> Self {
        self.label = label
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(bounded);
        self
    }

    /// The field it typed into, by the name the helper checks it has.
    pub fn field(mut self, field: Option<&str>) -> Self {
        self.field = field
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(bounded);
        self
    }

    /// Without the name it was given, for an element whose name is not a
    /// control's caption.
    pub(crate) fn unlabelled(mut self) -> Self {
        self.label = None;
        self
    }

    /// A click's verb from its button and count.
    pub fn click(button: Button, count: u8) -> Self {
        Self::new(match (button, count) {
            (Button::Right, _) => "right-click",
            (Button::Left | Button::Middle, 2) => "double-click",
            (Button::Left | Button::Middle, _) => "click",
        })
    }

    /// A key press, naming its keys when none may type text: shortcuts with
    /// ⌘ or ⌃ (⌘S) and keys such as Escape.
    pub fn keys(keys: &[String]) -> Self {
        let chords = keys
            .iter()
            .map(|key| chord(key))
            .collect::<Option<Vec<_>>>();
        let label = chords
            .filter(|chords| chords.len() <= CHORD_LIMIT)
            .map(|chords| chords.join(" "));

        Self {
            asked: true,
            ..Self::new("key").label(label.as_deref())
        }
    }

    /// Opening `app`, or else the first of `targets` that is an app bundle;
    /// a web address opens a link. Other targets are files, never named.
    pub fn open(app: Option<&str>, targets: &[String]) -> Self {
        let bundle = targets
            .iter()
            .map(String::as_str)
            .find(|target| target.trim_end_matches('/').ends_with(".app"));

        match app.or(bundle) {
            Some(app) => Self::new("open").app(Some(app)),
            None if targets.iter().any(|target| target.contains("://")) => Self::new("open-link"),
            None => Self::new("open"),
        }
    }

    /// What it does, as rows name it: "click", "type", "screenshot".
    pub fn verb(&self) -> &'static str {
        self.verb
    }

    /// The safe app name this command keeps, when it names one.
    pub(crate) fn app_name(&self) -> Option<&str> {
        self.app.as_deref()
    }

    /// Fill in `row`'s fields for these facts. A control is named only
    /// when the action worked: a name that matched nothing may be
    /// anything. A shortcut is named either way. The field typed text went
    /// to is named only when it worked and the helper, checking that field
    /// had keyboard focus, found it of `field_role`, one of [`FIELD_ROLES`].
    pub fn fill(&self, row: &mut Row, ok: bool, field_role: Option<&str>) {
        row.verb = Some(self.verb.to_owned());
        row.command = Some(self.command.clone());
        if let Some(app) = &self.app {
            row.app = Some(app.clone());
        }
        if let Some(label) = self.label.as_ref().filter(|_| ok || self.asked) {
            row.label = Some(label.clone());
        }
        if ok
            && self.verb == "type"
            && let Some(field) = &self.field
            && field_role.is_some_and(|role| FIELD_ROLES.contains(&role))
        {
            row.label = Some(field.clone());
        }
    }
}

/// Whether an element of `role` is named by its caption, so clicking it
/// may name it.
pub(crate) fn captioned(role: Option<&str>) -> bool {
    role.is_some_and(|role| CONTROL_ROLES.contains(&role))
}

/// An app's name from a name or an `.app` path; other paths, and names
/// with another extension, name a file, not an app.
fn app_name(app: &str) -> Option<&str> {
    let app = app.trim().trim_end_matches('/');
    let last = app.rsplit_once('/').map_or(app, |(_, last)| last);
    let name = match last.strip_suffix(".app") {
        Some(name) => name,
        None if app.contains('/') || last.contains('.') => return None,
        None => last,
    };

    (!name.is_empty()).then_some(name)
}

/// `text` as an app's bundle identifier (`com.apple.TextEdit`), when it is
/// one: 1 to [`BUNDLE_ID_LIMIT`] letters, digits, dots and hyphens, with
/// no empty part. Anything else, free text or a path, is none.
pub(crate) fn bundle_id(text: &str) -> Option<String> {
    let identifier = !text.is_empty()
        && text.len() <= BUNDLE_ID_LIMIT
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && text.split('.').all(|part| !part.is_empty());

    identifier.then(|| text.to_owned())
}

/// A key press's label (`⇧⌘Z ⌃⇥`, `fn F5`, `Page Up`) as keycaps, one list
/// per chord: each modifier's sign, then its key.
pub(crate) fn chords(label: &str) -> Vec<Vec<String>> {
    let mut words = label.split(' ').filter(|word| !word.is_empty()).peekable();
    let mut chords = Vec::new();

    while let Some(word) = words.next() {
        let mut caps = Vec::new();
        let mut word = word.to_owned();
        if word == FN_SIGN {
            caps.push(word);
            let Some(next) = words.next() else {
                chords.push(caps);
                break;
            };
            word = next.to_owned();
        }
        if let Some(next) = words.peek()
            && TWO_WORD_KEYS.contains(&format!("{word} {next}").as_str())
        {
            word = format!("{word} {next}");
            words.next();
        }

        let key = word.trim_start_matches(MODIFIER_SIGNS);
        caps.extend(word[..word.len() - key.len()].chars().map(String::from));
        if !key.is_empty() {
            caps.push(key.to_owned());
        }
        chords.push(caps);
    }

    chords
}

/// A key as macOS menus show it (cmd+shift+z is ⇧⌘Z), when it cannot type
/// text: it holds ⌘ or ⌃, or is a key such as Escape. None otherwise: ⌥
/// and fn chords type characters too.
fn chord(key: &str) -> Option<String> {
    let lower = key.to_lowercase();
    let split = match lower.strip_suffix("++") {
        Some(rest) => Some((rest, "+")),
        None => lower.rsplit_once('+'),
    };
    let Some((mods, last)) = split else {
        return named(&lower);
    };

    let mut held = [false; MODIFIERS.len()];
    for part in mods.split('+') {
        let slot = MODIFIERS
            .iter()
            .position(|(names, ..)| names.contains(&part))?;
        held[slot] = true;
    }

    let shortcut = MODIFIERS
        .iter()
        .zip(held)
        .any(|((.., shortcut), on)| *shortcut && on);
    if !shortcut && named(last).is_none() {
        return None;
    }

    let mut shown = MODIFIERS
        .iter()
        .zip(held)
        .filter_map(|((_, symbol, _), on)| on.then_some(*symbol))
        .collect::<String>();
    shown.push_str(&key_name(last));
    Some(shown)
}

/// A key pressed alone that types no text, in words: Escape, Return, F5.
fn named(key: &str) -> Option<String> {
    let name = match key {
        "return" | "enter" => "Return",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" | "delete" => "Delete",
        "up" | "down" | "left" | "right" => return Some(key_name(key)),
        "home" => "Home",
        "end" => "End",
        "pageup" => "Page Up",
        "pagedown" => "Page Down",
        _ if key.len() > 1
            && key.starts_with('f')
            && key[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            return Some(key.to_uppercase());
        }
        _ => return None,
    };

    Some(name.into())
}

/// A key's name in a shortcut: letters upper case, named keys as menus
/// show them.
fn key_name(key: &str) -> String {
    match key {
        "return" | "enter" => "↩".into(),
        "tab" => "⇥".into(),
        "escape" | "esc" => "⎋".into(),
        "space" => "Space".into(),
        "backspace" | "delete" => "⌫".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ => capitalized(key),
    }
}

/// What was done and what could not be: `done`, and "couldn't `action`".
fn worded(done: &str, action: &str) -> (String, String) {
    (done.to_owned(), format!("couldn't {action}"))
}

/// What a recorded row did, after who did it ("clicked Save in TextEdit"),
/// or what could not be done.
pub fn says(row: &Row) -> String {
    if row.under_way() {
        return doing(row);
    }

    let app = row.app.as_deref();
    let label = row.label.as_deref();
    let within = |text: &str| match app {
        Some(app) => format!("{text} in {app}"),
        None => text.to_owned(),
    };
    let on_label = |verb: &str| match label {
        Some(label) => within(&format!("{verb} {label}")),
        None => within(verb),
    };

    let (done, failed) = match row.verb.as_deref() {
        Some(verb @ ("click" | "double-click" | "right-click")) => {
            let past = match verb {
                "click" => "clicked",
                "double-click" => "double-clicked",
                _ => "right-clicked",
            };
            worded(&on_label(past), &on_label(verb))
        }
        Some("type") => match label {
            Some(field) => worded(
                &within(&format!("typed into {field}")),
                &within(&format!("type into {field}")),
            ),
            None => worded(
                &app.map_or("typed text".into(), |app| format!("typed in {app}")),
                &within("type"),
            ),
        },
        Some("key") => match (label, row.action_count) {
            (Some(label), _) => worded(
                &within(&format!("pressed {label}")),
                &within(&format!("press {label}")),
            ),
            (None, Some(1)) => worded(&within("pressed a key"), &within("press a key")),
            (None, _) => worded(&within("pressed keys"), &within("press keys")),
        },
        Some("scroll") => worded(&within("scrolled"), &within("scroll")),
        Some("drag") => worded(&within("dragged"), &within("drag")),
        Some("move") => worded("moved the pointer", "move the pointer"),
        Some(verb @ ("open" | "close" | "install")) => {
            let (past, object) = match verb {
                "open" => ("opened", "a file"),
                "close" => ("closed", "the apps"),
                _ => ("installed", "an app"),
            };
            let object = app.unwrap_or(object);
            worded(&format!("{past} {object}"), &format!("{verb} {object}"))
        }
        Some("open-link") => worded("opened a link", "open a link"),
        Some("cleanup") => worded("cleaned up the task's work", "clean up the task's work"),
        Some("job-start") => worded("started a background job", "start a background job"),
        Some("job-stop") => worded("stopped a background job", "stop a background job"),
        Some("collect") => match row.count {
            Some(count) if count > 0 => {
                let files = crate::name::count(count as usize, "file");
                (
                    format!("collected {files}"),
                    format!("collected {files}, not all asked for"),
                )
            }
            _ => worded("collected no files", "collect files"),
        },
        // Verbs worded as their operation is, and rows that say only that
        // (from older builds, raw commands, or verbs this build lacks).
        verb => {
            let plain = verb.filter(|verb| PLAIN_VERBS.contains(verb));
            operation(
                plain.or(row.operation.as_ref().map(|op| op.as_str())),
                row.action_count,
            )
        }
    };

    if row.worked() {
        done
    } else if row.coded(Code::FocusChanged) {
        not_sent(row, &failed)
    } else {
        failed
    }
}

/// What a command refused because another app came to the front did not
/// do, as a skip rather than a failure: "didn't type in Notes", or
/// "stopped typing in Notes" when part of its text was sent first.
fn not_sent(row: &Row, failed: &str) -> String {
    if row.partial == Some(true) && row.verb.as_deref() == Some("type") {
        return match row.app.as_deref() {
            Some(app) => format!("stopped typing in {app}"),
            None => "stopped typing".into(),
        };
    }

    match failed.strip_prefix("couldn't ") {
        Some(rest) => format!("didn't {rest}"),
        None => failed.to_owned(),
    }
}

/// What a command is doing while it runs: "opening TextEdit…".
fn doing(row: &Row) -> String {
    let app = row.app.as_deref();
    match row.verb.as_deref() {
        Some("open") => format!("opening {}…", app.unwrap_or("a file")),
        Some("open-link") => "opening a link…".into(),
        Some("close") => format!("closing {}…", app.unwrap_or("the apps")),
        Some("install") => format!("installing {}…", app.unwrap_or("an app")),
        Some("cleanup") => "cleaning up…".into(),
        Some("job-start") => "starting a background job…".into(),
        Some("job-stop") => "stopping a background job…".into(),
        Some("collect") => "collecting files…".into(),
        _ => "running a command…".into(),
    }
}

/// What an operation did and what could not be done, worded as Atlas.app
/// words it: for rows from older builds and raw commands, which say only
/// their operation, and verbs that need no more (run, screenshot).
fn operation(name: Option<&str>, count: Option<u64>) -> (String, String) {
    let count = count.unwrap_or(0);
    if name == Some("input") && count > 0 {
        let actions = format!("{count} action{}", if count == 1 { "" } else { "s" });
        return (
            format!("sent {actions}"),
            format!("couldn't send {actions}"),
        );
    }

    match name {
        Some("input") => worded("used the mouse and keyboard", "use the mouse and keyboard"),
        Some("run") => worded("ran a command", "run a command"),
        Some("restart") => worded("recovered the helper", "recover the helper"),
        Some("logout") => worded("logged out the helper", "log out the helper"),
        Some("screenshot") => worded("took a screenshot", "take a screenshot"),
        Some("apps") => worded("listed the open apps", "list the open apps"),
        _ => worded("did something in the helper", "finish an action"),
    }
}

#[cfg(test)]
mod tests;
