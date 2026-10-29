//! What an agent's command did, as the activity log keeps it and every
//! surface says it: a verb, the app it acted in and the control it named.
//! Typed text, argv, coordinates, output, file names and window titles are
//! never kept.
use crate::activity_log::Row;

/// The most characters an app name or element label keeps.
const NAME_LIMIT: usize = 40;
/// The most chords a key press names; more read as "pressed keys".
const CHORD_LIMIT: usize = 3;
/// How many characters of a random token tell one command's rows apart.
const COMMAND_ID: usize = 12;
/// Roles whose name is a control's own caption, such as a button's title.
/// Others may show a document's data: menu items name windows and recent
/// files, tabs and radio buttons name pages, pop-ups name folders.
const CONTROL_ROLES: [&str; 4] = [
    "AXButton",
    "AXCheckBox",
    "AXMenuBarItem",
    "AXDisclosureTriangle",
];
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

    /// Without the name it was given, for an element whose name is not a
    /// control's caption.
    pub fn unlabelled(mut self) -> Self {
        self.label = None;
        self
    }

    /// A click's verb from its button and count.
    pub fn click(right: bool, count: u8) -> Self {
        Self::new(match (right, count) {
            (true, _) => "right-click",
            (false, 2) => "double-click",
            _ => "click",
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

    /// Fill in `row`'s fields for these facts. A control is named only
    /// when the action worked: a name that matched nothing may be
    /// anything. A shortcut is named either way.
    pub fn fill(&self, row: &mut Row, ok: bool) {
        row.verb = Some(self.verb.to_owned());
        row.command = Some(self.command.clone());
        if let Some(app) = &self.app {
            row.app = Some(app.clone());
        }
        if let Some(label) = self.label.as_ref().filter(|_| ok || self.asked) {
            row.label = Some(label.clone());
        }
    }
}

/// Whether an element of `role` is named by its caption, so clicking it
/// may name it.
pub fn captioned(role: Option<&str>) -> bool {
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

/// At most [`NAME_LIMIT`] characters, cut with an ellipsis.
pub(crate) fn bounded(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= NAME_LIMIT {
        return text.to_owned();
    }

    let mut cut = text.chars().take(NAME_LIMIT - 1).collect::<String>();
    cut.push('…');
    cut
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

/// What a recorded row did, after who did it ("clicked Save in TextEdit"),
/// or what could not be done.
pub fn says(row: &Row) -> String {
    if row.under_way() {
        return doing(row);
    }

    let ok = row.worked();
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
            (on_label(past), format!("couldn't {}", on_label(verb)))
        }
        Some("type") => (
            app.map_or("typed text".into(), |app| format!("typed in {app}")),
            within("couldn't type"),
        ),
        Some("key") => match (label, row.action_count) {
            (Some(label), _) => (
                within(&format!("pressed {label}")),
                within(&format!("couldn't press {label}")),
            ),
            (None, Some(1)) => (within("pressed a key"), within("couldn't press a key")),
            (None, _) => (within("pressed keys"), within("couldn't press keys")),
        },
        Some("scroll") => (within("scrolled"), within("couldn't scroll")),
        Some("drag") => (within("dragged"), within("couldn't drag")),
        Some("move") => (
            "moved the pointer".into(),
            "couldn't move the pointer".into(),
        ),
        Some("open") => match app {
            Some(app) => (format!("opened {app}"), format!("couldn't open {app}")),
            None => ("opened a file".into(), "couldn't open a file".into()),
        },
        Some("open-link") => ("opened a link".into(), "couldn't open a link".into()),
        Some("close") => match app {
            Some(app) => (format!("closed {app}"), format!("couldn't close {app}")),
            None => ("closed the apps".into(), "couldn't close the apps".into()),
        },
        Some("install") => match app {
            Some(app) => (
                format!("installed {app}"),
                format!("couldn't install {app}"),
            ),
            None => ("installed an app".into(), "couldn't install an app".into()),
        },
        Some("cleanup") => (
            "cleaned up the task's work".into(),
            "couldn't clean up the task's work".into(),
        ),
        Some("job-start") => (
            "started a background job".into(),
            "couldn't start a background job".into(),
        ),
        Some("job-stop") => (
            "stopped a background job".into(),
            "couldn't stop a background job".into(),
        ),
        Some("collect") => match row.count {
            Some(count) if count > 0 => (
                format!(
                    "collected {}",
                    crate::ui::text::count(count as usize, "file")
                ),
                format!(
                    "collected {}, not all asked for",
                    crate::ui::text::count(count as usize, "file")
                ),
            ),
            _ => ("collected no files".into(), "couldn't collect files".into()),
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

    if ok {
        done
    } else if row.coded("focus-changed") {
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
    match (row.verb.as_deref(), app) {
        (Some("open"), Some(app)) => format!("opening {app}…"),
        (Some("open"), None) => "opening a file…".into(),
        (Some("open-link"), _) => "opening a link…".into(),
        (Some("close"), Some(app)) => format!("closing {app}…"),
        (Some("close"), None) => "closing the apps…".into(),
        (Some("install"), Some(app)) => format!("installing {app}…"),
        (Some("install"), None) => "installing an app…".into(),
        (Some("cleanup"), _) => "cleaning up…".into(),
        (Some("job-start"), _) => "starting a background job…".into(),
        (Some("job-stop"), _) => "stopping a background job…".into(),
        (Some("collect"), _) => "collecting files…".into(),
        _ => "running a command…".into(),
    }
}

/// What an operation did and what could not be done, worded as Atlas.app
/// words it: for rows from older builds and raw commands, which say only
/// their operation, and verbs that need no more (run, screenshot).
fn operation(name: Option<&str>, count: Option<u64>) -> (String, String) {
    let count = count.unwrap_or(0);
    let actions = format!("{count} action{}", if count == 1 { "" } else { "s" });

    match name {
        Some("input") if count > 0 => (
            format!("sent {actions}"),
            format!("couldn't send {actions}"),
        ),
        Some("input") => (
            "used the mouse and keyboard".into(),
            "couldn't use the mouse and keyboard".into(),
        ),
        Some("run") => ("ran a command".into(), "couldn't run a command".into()),
        Some("restart") => (
            "recovered the helper".into(),
            "couldn't recover the helper".into(),
        ),
        Some("logout") => (
            "logged out the helper".into(),
            "couldn't log out the helper".into(),
        ),
        Some("screenshot") => (
            "took a screenshot".into(),
            "couldn't take a screenshot".into(),
        ),
        Some("apps") => (
            "listed the open apps".into(),
            "couldn't list the open apps".into(),
        ),
        _ => (
            "did something in the helper".into(),
            "couldn't finish an action".into(),
        ),
    }
}

/// `text` with its first letter upper case, for a line shown on its own.
pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_log::Op;
    use serde_json::{Value, json};

    fn row(doing: &Doing, ok: bool) -> Row {
        let mut row = Row {
            operation: Some(Op::Input),
            ok: Some(ok),
            ..Row::default()
        };
        doing.fill(&mut row, ok);
        row
    }

    /// `value` as a logged row.
    fn read(value: Value) -> Row {
        Row::parse(&value).unwrap()
    }

    fn said(doing: &Doing) -> (String, String) {
        (says(&row(doing, true)), says(&row(doing, false)))
    }

    #[test]
    fn each_verb_says_what_it_did_and_where_or_what_it_could_not_do() {
        let save = Doing::click(false, 1)
            .app(Some("TextEdit"))
            .label(Some("Save"));
        let text = |doing: Doing| said(&doing.app(Some("TextEdit")));
        let pair = |done: &str, failed: &str| (done.to_owned(), failed.to_owned());

        assert_eq!(
            said(&save),
            pair("clicked Save in TextEdit", "couldn't click in TextEdit"),
            "a name that matched nothing is not kept"
        );
        assert_eq!(
            said(&Doing::click(false, 1)),
            pair("clicked", "couldn't click")
        );
        assert_eq!(said(&Doing::click(false, 2)).0, "double-clicked");
        assert_eq!(
            said(&Doing::click(true, 1).label(Some("Item"))).0,
            "right-clicked Item"
        );
        assert_eq!(
            said(&Doing::new("type")),
            pair("typed text", "couldn't type")
        );
        assert_eq!(
            text(Doing::new("type")),
            pair("typed in TextEdit", "couldn't type in TextEdit")
        );
        assert_eq!(
            text(Doing::new("scroll")),
            pair("scrolled in TextEdit", "couldn't scroll in TextEdit")
        );
        assert_eq!(said(&Doing::new("drag")), pair("dragged", "couldn't drag"));
        assert_eq!(
            said(&Doing::new("move")),
            pair("moved the pointer", "couldn't move the pointer")
        );
        assert_eq!(
            text(Doing::new("close")),
            pair("closed TextEdit", "couldn't close TextEdit")
        );
        assert_eq!(said(&Doing::new("close")).0, "closed the apps");
    }

    #[test]
    fn a_command_under_way_says_what_it_is_doing() {
        let started = |doing: &Doing| {
            let mut row = row(doing, true);
            row.ok = None;
            row.started = Some(true);
            says(&row)
        };

        assert_eq!(
            started(&Doing::new("open").app(Some("TextEdit"))),
            "opening TextEdit…"
        );
        assert_eq!(
            started(&Doing::open(None, &["https://x.org".into()])),
            "opening a link…"
        );
        assert_eq!(started(&Doing::new("close")), "closing the apps…");
        assert_eq!(started(&Doing::new("run")), "running a command…");
        assert_eq!(
            started(&Doing::new("install").app(Some("Probe"))),
            "installing Probe…"
        );
        assert_eq!(started(&Doing::new("cleanup")), "cleaning up…");
    }

    #[test]
    fn an_install_row_names_its_app_and_a_cleanup_row_names_no_file() {
        let pair = |done: &str, failed: &str| (done.to_owned(), failed.to_owned());
        assert_eq!(
            said(&Doing::new("install").app(Some("/tmp/Secret/Probe.app"))),
            pair("installed Probe", "couldn't install Probe")
        );
        assert_eq!(
            said(&Doing::new("cleanup")),
            pair(
                "cleaned up the task's work",
                "couldn't clean up the task's work"
            )
        );
    }

    #[test]
    fn opening_names_an_app_never_a_file_and_a_web_address_is_a_link() {
        let open = |app: Option<&str>, targets: &[&str]| {
            let targets = targets.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
            said(&Doing::open(app, &targets))
        };

        assert_eq!(open(Some("TextEdit"), &["notes.txt"]).0, "opened TextEdit");
        assert_eq!(
            open(None, &["./build/TextEdit.app/"]),
            ("opened TextEdit".into(), "couldn't open TextEdit".into())
        );
        assert_eq!(open(None, &["notes/plan.txt"]).0, "opened a file");
        assert_eq!(open(Some("notes/plan.txt"), &[]).0, "opened a file");
        assert_eq!(open(Some("notes.txt"), &[]).0, "opened a file");
        assert_eq!(
            open(Some("Visual Studio Code"), &[]).0,
            "opened Visual Studio Code"
        );
        assert_eq!(
            open(None, &["https://example.com"]),
            ("opened a link".into(), "couldn't open a link".into())
        );
    }

    #[test]
    fn shortcuts_and_keys_that_type_nothing_are_named_and_others_are_not() {
        let keys = |keys: &[&str]| {
            let keys = keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
            let mut row = row(&Doing::keys(&keys), true);
            row.action_count = Some(keys.len() as u64);
            says(&row)
        };

        assert_eq!(keys(&["cmd+s"]), "pressed ⌘S");
        assert_eq!(keys(&["cmd+shift+z", "ctrl+tab"]), "pressed ⇧⌘Z ⌃⇥");
        assert_eq!(keys(&["cmd++"]), "pressed ⌘+");
        assert_eq!(keys(&["shift+tab"]), "pressed ⇧⇥", "Tab types nothing");
        assert_eq!(keys(&["escape"]), "pressed Escape");
        assert_eq!(keys(&["down", "return"]), "pressed ↓ Return");
        assert_eq!(keys(&["f5"]), "pressed F5");
        assert_eq!(
            keys(&["a"]),
            "pressed a key",
            "a bare key may be typed text"
        );
        assert_eq!(keys(&["space"]), "pressed a key", "so may a space");
        assert_eq!(keys(&["shift+a"]), "pressed a key", "shift types a capital");
        assert_eq!(keys(&["alt+e"]), "pressed a key", "⌥ types accents");
        assert_eq!(keys(&["fn+a"]), "pressed a key");
        assert_eq!(
            keys(&["cmd+s", "x"]),
            "pressed keys",
            "one bare key hides them all"
        );
        assert_eq!(
            keys(&["cmd+a", "cmd+c", "cmd+v", "cmd+w"]),
            "pressed keys",
            "a long sequence is not spelled out"
        );

        let mut failed = row(&Doing::keys(&["cmd+s".into()]), false);
        failed.action_count = Some(1);
        assert_eq!(says(&failed), "couldn't press ⌘S");
    }

    #[test]
    fn only_a_controls_caption_is_a_label() {
        assert!(captioned(Some("AXButton")));
        assert!(captioned(Some("AXMenuBarItem")));
        assert!(
            !captioned(Some("AXMenuItem")),
            "a menu item may name a window or file"
        );
        assert!(!captioned(Some("AXTab")), "a tab may name a page");
        assert!(
            !captioned(Some("AXPopUpButton")),
            "a pop-up may name a folder"
        );
        assert!(!captioned(Some("AXWindow")), "a window's name is its title");
        assert!(!captioned(Some("AXStaticText")), "text may be a document's");
        assert!(!captioned(Some("AXRow")), "a row may name a file");
        assert!(!captioned(None));

        let save = Doing::click(false, 1).label(Some("Save"));
        assert_eq!(save.unlabelled().label, None);
    }

    #[test]
    fn names_are_bounded_and_rows_carry_nothing_else() {
        let long = "x".repeat(100);
        let doing = Doing::click(false, 1).app(Some(&long)).label(Some("  "));
        assert_eq!(doing.app.as_ref().unwrap().chars().count(), NAME_LIMIT);
        assert!(doing.app.as_ref().unwrap().ends_with('…'));
        assert_eq!(doing.label, None, "a blank label is no label");

        let fields = row(&doing, true);
        let written = serde_json::to_value(&fields).unwrap();
        let keys = written.as_object().unwrap().keys().collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "actionCount",
                "actor",
                "app",
                "at",
                "command",
                "elapsedMs",
                "ok",
                "operation",
                "verb"
            ]
        );
        assert_eq!(fields.command.unwrap().len(), COMMAND_ID);
        assert_ne!(Doing::new("type").command, Doing::new("type").command);
    }

    #[test]
    fn rows_without_a_verb_read_by_their_operation_as_atlas_app_words_them() {
        let old = |operation: &str, count: Option<u64>, ok: bool| {
            says(&read(
                json!({"operation": operation, "ok": ok, "actionCount": count}),
            ))
        };

        assert_eq!(old("input", Some(3), true), "sent 3 actions");
        assert_eq!(old("input", Some(1), false), "couldn't send 1 action");
        assert_eq!(old("input", None, true), "used the mouse and keyboard");
        assert_eq!(old("run", None, true), "ran a command");
        assert_eq!(old("restart", None, false), "couldn't recover the helper");
        assert_eq!(old("logout", None, true), "logged out the helper");
        assert_eq!(old("screenshot", None, true), "took a screenshot");
        assert_eq!(old("apps", None, false), "couldn't list the open apps");
        assert_eq!(old("newop", None, true), "did something in the helper");
        assert_eq!(old("newop", None, false), "couldn't finish an action");
        assert_eq!(
            says(&read(
                json!({"operation": "run", "ok": true, "verb": "someday"})
            )),
            "ran a command",
            "a verb this build lacks reads by its operation"
        );
        assert_eq!(capitalized("clicked Save"), "Clicked Save");
    }

    #[test]
    fn a_focus_refusal_says_what_it_did_not_do() {
        let refused = |verb: &str, label: Option<&str>, partial: bool| {
            says(&read(
                json!({"operation": "input", "ok": false, "code": "focus-changed",
                "verb": verb, "app": "Notes", "label": label, "partial": partial}),
            ))
        };

        assert_eq!(refused("type", None, false), "didn't type in Notes");
        assert_eq!(refused("type", None, true), "stopped typing in Notes");
        assert_eq!(refused("click", None, false), "didn't click in Notes");
        assert_eq!(
            refused("key", Some("⌘S"), false),
            "didn't press ⌘S in Notes"
        );
        assert_eq!(
            refused("key", Some("⌘S"), true),
            "didn't press ⌘S in Notes",
            "only typing can stop part way"
        );
        assert_eq!(
            says(&read(
                json!({"operation": "input", "ok": false, "code": "busy",
                "verb": "type", "app": "Notes"})
            )),
            "couldn't type in Notes",
            "other refusals still say what could not be done"
        );
    }

    #[test]
    fn background_jobs_and_collections_are_said_plainly() {
        let row = |verb: &str, ok: bool| read(json!({"verb": verb, "ok": ok, "operation": "job"}));
        assert_eq!(says(&row("job-start", true)), "started a background job");
        assert_eq!(
            says(&row("job-start", false)),
            "couldn't start a background job"
        );
        assert_eq!(says(&row("job-stop", true)), "stopped a background job");
        assert_eq!(
            says(&row("job-stop", false)),
            "couldn't stop a background job"
        );
        assert_eq!(
            says(&read(
                json!({"verb": "collect", "ok": true, "count": 2, "operation": "collect"})
            )),
            "collected 2 files"
        );
        assert_eq!(
            says(&read(
                json!({"verb": "collect", "ok": false, "count": 0, "operation": "collect"})
            )),
            "couldn't collect files"
        );
        assert_eq!(
            says(&read(
                json!({"verb": "collect", "started": true, "ok": null, "operation": "collect"})
            )),
            "collecting files…"
        );
    }
}
