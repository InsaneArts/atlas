use super::*;
use crate::activity_log::Op;
use atlas_core::name::NAME_LIMIT;
use serde_json::{Value, json};

fn row(doing: &Doing, ok: bool) -> Row {
    let mut row = Row {
        operation: Some(Op::Input),
        ok: Some(ok),
        ..Row::default()
    };
    doing.fill(&mut row, ok, None);
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
    let save = Doing::click(Button::Left, 1)
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
        said(&Doing::click(Button::Left, 1)),
        pair("clicked", "couldn't click")
    );
    assert_eq!(said(&Doing::click(Button::Left, 2)).0, "double-clicked");
    assert_eq!(
        said(&Doing::click(Button::Right, 1).label(Some("Item"))).0,
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

/// An action on an element says what kind of thing it did, in the app it
/// names, and never names the element.
#[test]
fn each_action_on_an_element_says_what_it_did_without_naming_it() {
    let pair = |done: &str, failed: &str| (done.to_owned(), failed.to_owned());
    assert_eq!(
        said(&Doing::new("press").app(Some("TextEdit"))),
        pair(
            "pressed a control in TextEdit",
            "couldn't press a control in TextEdit"
        )
    );
    assert_eq!(
        said(&Doing::new("set-value")),
        pair("set a value", "couldn't set a value")
    );
    for (verb, done) in [
        ("check", "checked a box"),
        ("uncheck", "unchecked a box"),
        ("pick", "picked an option"),
        ("expand", "expanded an element"),
        ("collapse", "collapsed an element"),
        ("scroll-into-view", "scrolled an element into view"),
    ] {
        assert_eq!(said(&Doing::new(verb)).0, done);
    }
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

    let save = Doing::click(Button::Left, 1).label(Some("Save"));
    assert_eq!(save.unlabelled().label, None);
}

#[test]
fn names_are_bounded_and_rows_carry_nothing_else() {
    let long = "x".repeat(100);
    let doing = Doing::click(Button::Left, 1)
        .app(Some(&long))
        .label(Some("  "));
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
fn a_bundle_id_is_kept_only_when_it_is_an_identifier() {
    for kept in ["com.apple.TextEdit", "test.atlas.install-probe", "Opie"] {
        assert_eq!(bundle_id(kept).as_deref(), Some(kept), "{kept}");
    }
    let long = format!("com.{}", "x".repeat(BUNDLE_ID_LIMIT));
    for refused in [
        "",
        "com.apple Text",
        "/Applications/Opie.app",
        "com/apple",
        "com..apple",
        ".com.apple",
        "com.apple.",
        "com.apple_text",
        "com.äpple",
        long.as_str(),
    ] {
        assert_eq!(bundle_id(refused), None, "{refused}");
    }
    let longest = "x".repeat(BUNDLE_ID_LIMIT);
    assert_eq!(bundle_id(&longest), Some(longest.clone()));
}

#[test]
fn a_key_rows_chords_split_into_caps_and_page_up_stays_one_key() {
    let caps = |label: &str| chords(label);
    assert_eq!(caps("⌘S"), [["⌘", "S"]]);
    assert_eq!(caps("⇧⌘Z ⌃⇥"), vec![vec!["⇧", "⌘", "Z"], vec!["⌃", "⇥"]]);
    assert_eq!(caps("⌘+"), [["⌘", "+"]]);
    assert_eq!(caps("Page Up"), [["Page Up"]]);
    assert_eq!(
        caps("↓ Page Down Return"),
        [vec!["↓"], vec!["Page Down"], vec!["Return"]]
    );
    assert_eq!(caps("fn F5"), [["fn", "F5"]]);
    assert_eq!(caps("fn ⌘S"), [["fn", "⌘", "S"]]);
    assert_eq!(caps("Escape"), [["Escape"]]);
    assert!(caps("").is_empty());

    let pressed = |keys: &[&str]| {
        let keys = keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
        chords(&row(&Doing::keys(&keys), true).label.unwrap())
    };
    assert_eq!(
        pressed(&["cmd+shift+z", "pageup"]),
        vec![vec!["⇧", "⌘", "Z"], vec!["Page Up"]]
    );
    assert_eq!(pressed(&["fn+f5"]), [["fn", "F5"]]);
}

/// A typed row's label and words, once its text went to a field the
/// helper found of `role`, as `ok` says.
fn typed_into(doing: &Doing, ok: bool, role: Option<&str>) -> (Option<String>, String) {
    let mut row = Row {
        operation: Some(Op::Input),
        ok: Some(ok),
        ..Row::default()
    };
    doing.fill(&mut row, ok, role);
    let words = says(&row);
    (row.label, words)
}

#[test]
fn a_typed_row_names_its_field_only_when_the_field_was_checked_and_is_a_text_field() {
    let email = Doing::new("type").app(Some("Notes")).field(Some(" Email "));
    let named = |role: &str| typed_into(&email, true, Some(role));
    for role in FIELD_ROLES {
        assert_eq!(
            named(role),
            (
                Some("Email".to_owned()),
                "typed into Email in Notes".to_owned()
            ),
            "{role}"
        );
    }
    assert_eq!(
        typed_into(
            &Doing::new("type").field(Some("Search")),
            true,
            Some("AXSearchField")
        )
        .1,
        "typed into Search"
    );

    let unnamed = (None, "typed in Notes".to_owned());
    assert_eq!(
        typed_into(&email, true, None),
        unnamed,
        "a field the helper did not check is not named"
    );
    assert_eq!(
        typed_into(&email, false, Some("AXTextField")),
        (None, "couldn't type in Notes".to_owned()),
        "nor one text did not reach"
    );
    assert_eq!(
        typed_into(
            &Doing::new("type").app(Some("Notes")),
            true,
            Some("AXTextField")
        ),
        unnamed,
        "nor one the command did not name"
    );
    assert_eq!(
        typed_into(
            &Doing::new("type").field(Some("  ")),
            true,
            Some("AXTextField")
        )
        .0,
        None,
        "a blank name is no name"
    );
    let long = "x".repeat(100);
    let bounded = typed_into(
        &Doing::new("type").field(Some(&long)),
        true,
        Some("AXTextField"),
    );
    assert_eq!(bounded.0.unwrap().chars().count(), NAME_LIMIT);
}

#[test]
fn a_text_areas_name_is_never_kept() {
    let body = Doing::new("type").app(Some("Notes")).field(Some("Body"));
    for role in [
        "AXTextArea",
        "AXSecureTextField",
        "AXStaticText",
        "AXWebArea",
        "AXButton",
        "",
    ] {
        assert_eq!(
            typed_into(&body, true, Some(role)),
            (None, "typed in Notes".to_owned()),
            "{role}"
        );
    }
    let tab = Doing::keys(&["tab".into()]).field(Some("Email"));
    assert_eq!(
        typed_into(&tab, true, Some("AXTextField")).0.as_deref(),
        Some("Tab"),
        "only typed text names its field; a key names its keys"
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
