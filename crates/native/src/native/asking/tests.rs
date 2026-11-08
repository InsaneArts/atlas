use super::*;
use atlas_core::screen::elements::Reference;

/// A scripted helper: what lies at every point, what has focus, what a
/// reference names, and the message a Return would send.
struct Scripted {
    point: Target,
    focus: Target,
    referenced: Target,
    message: Message,
    /// How many times the message was read.
    messages: usize,
}

impl Look for Scripted {
    fn at_point(&mut self, _: f64, _: f64) -> Target {
        self.point.clone()
    }
    fn focused(&mut self) -> Target {
        self.focus.clone()
    }
    fn referenced(&mut self, _: &Reference) -> Target {
        self.referenced.clone()
    }
    fn message(&mut self) -> Message {
        self.messages += 1;
        self.message.clone()
    }
}

const CHROME: &str = "com.google.Chrome";
const SLACK: &str = "com.tinyspeck.slackmacgap";

/// A page on `host` in Chrome, with an element of `role` named `name` there.
fn page(host: &str, role: &str, name: Option<&str>) -> Target {
    Target {
        app: Some(CHROME.to_owned()),
        site: Site::Host(host.to_owned()),
        role: Some(role.to_owned()),
        name: name.map(str::to_owned),
        ..Target::default()
    }
}

/// A text field on `host` in Chrome with focus.
fn field(host: &str, multiline: bool) -> Target {
    Target {
        editable: true,
        multiline,
        ..page(
            host,
            if multiline {
                "AXTextArea"
            } else {
                "AXTextField"
            },
            Some("Message"),
        )
    }
}

/// The account key of site `id` in Chrome.
fn site(id: &str) -> AccountKey {
    AccountKey {
        kind: Kind::Site,
        id: id.to_owned(),
        browser: Some(CHROME.to_owned()),
    }
}

fn scripted(point: Target, focus: Target) -> Scripted {
    Scripted {
        point,
        focus,
        referenced: Target::default(),
        message: Message::whole("Atlas 0.1.1 is out"),
        messages: 0,
    }
}

/// The person lists `id` (a site in Chrome, or an app) at `level`.
fn listing(kind: Kind, id: &str, level: Level) -> Asking {
    Asking {
        accounts: vec![Listed {
            kind,
            id: id.to_owned(),
            name: id.to_owned(),
            browser: (kind == Kind::Site).then(|| CHROME.to_owned()),
            level,
        }],
        ..Asking::default()
    }
}

fn actions(value: serde_json::Value) -> Vec<Action> {
    serde_json::from_value(value).unwrap()
}

fn what(decision: &Decision) -> Option<&'static str> {
    match decision {
        Decision::Ask(question) => Some(question.what),
        Decision::Free | Decision::Deny { .. } => None,
    }
}

#[test]
fn a_draft_is_free_and_return_in_the_field_asks_with_the_message() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(Target::default(), field("app.slack.com", true));
    let draft = actions(json!([{"type": "type", "text": "Atlas 0.1.1 is out"}]));
    assert_eq!(decide(&draft, &asking, &mut look), Decision::Free);

    let send = actions(json!([{"type": "key", "key": "return"}]));
    let Decision::Ask(question) = decide(&send, &asking, &mut look) else {
        panic!("Return in a message field sends");
    };
    assert_eq!(question.what, "send-key");
    assert_eq!(question.account, "slack.com");
    assert_eq!(
        question.details()["message"]["preview"],
        "Atlas 0.1.1 is out"
    );
    assert_eq!(question.details()["once"], true);

    let line_break = actions(json!([{"type": "key", "key": "shift+return"}]));
    assert_eq!(decide(&line_break, &asking, &mut look), Decision::Free);
    let arrows = actions(json!([{"type": "key", "key": "down"}, {"type": "key", "key": "tab"}]));
    assert_eq!(decide(&arrows, &asking, &mut look), Decision::Free);
}

#[test]
fn a_key_with_no_text_field_focused_may_be_a_shortcut_and_asks() {
    let asking = listing(Kind::Site, "google.com", Level::Ask);
    let mut look = scripted(
        Target::default(),
        page("mail.google.com", "AXWebArea", None),
    );
    for key in ["#", "e", "r"] {
        let decision = decide(
            &actions(json!([{"type": "key", "key": key}])),
            &asking,
            &mut look,
        );
        assert_eq!(what(&decision), Some("unsure"), "{key}");
    }
    let return_on_nothing = actions(json!([{"type": "key", "key": "cmd+return"}]));
    assert!(matches!(
        decide(&return_on_nothing, &asking, &mut look),
        Decision::Ask(_)
    ));
}

#[test]
fn a_click_asks_by_the_controls_name() {
    let asking = listing(Kind::Site, "github.com", Level::Ask);
    let click = actions(json!([{"type": "click", "x": 10, "y": 20}]));
    for (name, asks) in [
        (Some("Merge pull request"), true),
        (Some("Send"), true),
        (Some("Senden"), true),
        (Some("Envoyer"), true),
        (Some("削除"), true),
        (None, true),
        (Some("Open"), false),
        (Some("Files changed"), false),
    ] {
        let mut look = scripted(page("github.com", "AXButton", name), Target::default());
        let decision = decide(&click, &asking, &mut look);
        assert_eq!(matches!(decision, Decision::Ask(_)), asks, "{name:?}");
    }
}

#[test]
fn every_pointer_phase_is_judged_and_the_press_decides_for_its_release() {
    let asking = listing(Kind::Site, "github.com", Level::Ask);
    let mut merge = scripted(
        page("github.com", "AXButton", Some("Merge")),
        Target::default(),
    );
    let down = actions(json!([{"type": "pointer", "phase": "down", "x": 1, "y": 1}]));
    assert_eq!(what(&decide(&down, &asking, &mut merge)), Some("click"));
    let up = actions(json!([{"type": "pointer", "phase": "up", "x": 1, "y": 1}]));
    assert_eq!(
        what(&decide(&up, &asking, &mut merge)),
        Some("click"),
        "a lone release"
    );
    let hover = actions(json!([{"type": "pointer", "phase": "move", "x": 1, "y": 1}]));
    assert_eq!(decide(&hover, &asking, &mut merge), Decision::Free);

    let mut files = scripted(
        page("github.com", "AXLink", Some("Files")),
        Target::default(),
    );
    let pair = actions(json!([
        {"type": "pointer", "phase": "down", "x": 1, "y": 1},
        {"type": "pointer", "phase": "up", "x": 1, "y": 1},
    ]));
    assert_eq!(decide(&pair, &asking, &mut files), Decision::Free);
    let drag = actions(json!([{"type": "drag", "fromX": 1, "fromY": 1, "toX": 5, "toY": 5}]));
    assert_eq!(what(&decide(&drag, &asking, &mut files)), Some("drag"));
}

#[test]
fn a_later_action_after_one_that_may_move_focus_cant_be_told_and_asks() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(
        page("app.slack.com", "AXGroup", Some("Message #general")),
        field("app.slack.com", true),
    );
    let whole = actions(json!([
        {"type": "click", "x": 1, "y": 1},
        {"type": "type", "text": "hello"},
        {"type": "key", "key": "return"},
    ]));
    let Decision::Ask(question) = decide(&whole, &asking, &mut look) else {
        panic!("the request asks before its first event");
    };
    assert_eq!(
        question.what, "unsure",
        "after the click, focus can't be told"
    );
}

#[test]
fn text_with_a_line_break_or_into_a_password_field_asks_without_its_text() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(Target::default(), field("app.slack.com", false));
    let newline = actions(json!([{"type": "type", "text": "one\ntwo"}]));
    assert_eq!(what(&decide(&newline, &asking, &mut look)), Some("newline"));

    let mut secure = field("app.slack.com", false);
    secure.secure = true;
    let mut look = scripted(Target::default(), secure);
    let password = actions(json!([{"type": "type", "text": "hunter2-canary"}]));
    let Decision::Ask(question) = decide(&password, &asking, &mut look) else {
        panic!("a password field asks");
    };
    assert_eq!(question.what, "password");
    assert_eq!(question.typed, 14);
    assert!(!question.details().to_string().contains("hunter2-canary"));
    assert_eq!(look.messages, 0, "a password field's value is never read");
}

#[test]
fn look_only_refuses_every_change_and_allows_looking() {
    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let slack = Target {
        app: Some(SLACK.to_owned()),
        name: Some("Open".to_owned()),
        ..Target::default()
    };
    let mut look = scripted(slack.clone(), slack);
    for action in [
        json!({"type": "click", "x": 1, "y": 1}),
        json!({"type": "pointer", "phase": "down", "x": 1, "y": 1}),
        json!({"type": "pointer", "phase": "up", "x": 1, "y": 1}),
        json!({"type": "key", "key": "down"}),
        json!({"type": "type", "text": "a"}),
        json!({"type": "drag", "fromX": 1, "fromY": 1, "toX": 2, "toY": 2}),
    ] {
        let decision = decide(&actions(json!([action.clone()])), &asking, &mut look);
        assert!(matches!(decision, Decision::Deny { .. }), "{action}");
    }
    for action in [
        json!({"type": "scroll", "x": 1, "y": 1, "dy": 3}),
        json!({"type": "move", "x": 1, "y": 1}),
        json!({"type": "pointer", "phase": "move", "x": 1, "y": 1}),
        json!({"type": "wait", "ms": 10}),
    ] {
        assert_eq!(
            decide(&actions(json!([action.clone()])), &asking, &mut look),
            Decision::Free,
            "{action}"
        );
    }
}

#[test]
fn an_unreadable_page_takes_the_strictest_level_listed_for_its_browser() {
    let mut asking = listing(Kind::Site, "bank.example", Level::LookOnly);
    asking.accounts.push(Listed {
        kind: Kind::Site,
        id: "github.com".to_owned(),
        name: "github.com".to_owned(),
        browser: Some(CHROME.to_owned()),
        level: Level::Ask,
    });
    let unread = Target {
        app: Some(CHROME.to_owned()),
        site: Site::Unread,
        name: Some("Open".to_owned()),
        ..Target::default()
    };
    let mut look = scripted(unread, Target::default());
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    assert!(matches!(
        decide(&click, &asking, &mut look),
        Decision::Deny { .. }
    ));
    // Whatever grant it carries: Look Only is never asked, so no grant can cover it.
    asking.grants.push(Allowance::Task {
        account: site("bank.example"),
    });
    assert!(matches!(
        decide(&click, &asking, &mut look),
        Decision::Deny { .. }
    ));
}

#[test]
fn accounts_not_listed_and_trusted_ones_ask_nothing() {
    let mut look = scripted(
        page("github.com", "AXButton", Some("Merge")),
        Target::default(),
    );
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    assert_eq!(
        decide(
            &click,
            &listing(Kind::Site, "gitlab.com", Level::Ask),
            &mut look
        ),
        Decision::Free
    );
    assert_eq!(
        decide(
            &click,
            &listing(Kind::Site, "github.com", Level::Trusted),
            &mut look
        ),
        Decision::Free
    );
    let unread = Asking {
        unknown: true,
        ..Asking::default()
    };
    assert_eq!(
        what(&decide(&click, &unread, &mut look)),
        Some("click"),
        "an unreadable list asks"
    );
}

#[test]
fn a_once_grant_covers_only_this_request_with_this_message_and_a_task_grant_only_its_account() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(Target::default(), field("app.slack.com", true));
    let send = actions(json!([{"type": "key", "key": "return"}]));
    let Decision::Ask(question) = decide(&send, &asking, &mut look) else {
        panic!("asks");
    };
    let Some(Message::Whole { digest, .. }) = &question.message else {
        panic!("read whole");
    };
    asking.grants.push(Allowance::Once {
        account: question.account_key.clone(),
        batch: question.batch.clone(),
        target: question.bound.clone(),
        value: Some(digest.clone()),
    });
    assert_eq!(decide(&send, &asking, &mut look), Decision::Free);

    // The same prefix with another ending is another message: it asks again.
    look.message = Message::whole("Atlas 0.1.1 is out — and so is 0.1.2");
    assert!(matches!(
        decide(&send, &asking, &mut look),
        Decision::Ask(_)
    ));
    // Another request asks again too.
    let other = actions(json!([{"type": "key", "key": "enter"}]));
    look.message = Message::whole("Atlas 0.1.1 is out");
    assert!(matches!(
        decide(&other, &asking, &mut look),
        Decision::Ask(_)
    ));

    let mut task = listing(Kind::Site, "slack.com", Level::Ask);
    task.grants.push(Allowance::Task {
        account: site("github.com"),
    });
    assert!(
        matches!(decide(&send, &task, &mut look), Decision::Ask(_)),
        "another account's grant"
    );
    task.grants.push(Allowance::Task {
        account: site("slack.com"),
    });
    assert_eq!(decide(&send, &task, &mut look), Decision::Free);
}

#[test]
fn a_message_that_cant_be_read_whole_offers_no_just_this_once() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(Target::default(), field("app.slack.com", true));
    look.message = Message::Unread;
    let send = actions(json!([{"type": "key", "key": "return"}]));
    let Decision::Ask(question) = decide(&send, &asking, &mut look) else {
        panic!("asks");
    };
    assert_eq!(question.details()["once"], false);
    asking.grants.push(Allowance::Once {
        account: question.account_key.clone(),
        batch: question.batch.clone(),
        target: question.bound.clone(),
        value: None,
    });
    assert!(matches!(
        decide(&send, &asking, &mut look),
        Decision::Ask(_)
    ));
}

#[test]
fn referenced_actions_go_by_the_elements_name() {
    let asking = listing(Kind::App, SLACK, Level::Ask);
    let mut look = scripted(Target::default(), Target::default());
    look.referenced = Target {
        app: Some(SLACK.to_owned()),
        role: Some("AXButton".to_owned()),
        name: Some("Delete message".to_owned()),
        ..Target::default()
    };
    let reference = "8123abc.12";
    for action in [
        json!({"type": "press", "ref": reference}),
        json!({"type": "check", "ref": reference}),
        json!({"type": "expand", "ref": reference}),
    ] {
        assert_eq!(
            what(&decide(
                &actions(json!([action.clone()])),
                &asking,
                &mut look
            )),
            Some("click"),
            "{action}"
        );
    }
    let into_view = actions(json!([{"type": "scroll-into-view", "ref": reference}]));
    assert_eq!(decide(&into_view, &asking, &mut look), Decision::Free);
}

#[test]
fn a_once_grant_for_send_never_covers_the_same_click_landing_on_delete() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let mut look = scripted(
        page("app.slack.com", "AXButton", Some("Send")),
        Target::default(),
    );
    let Decision::Ask(question) = decide(&click, &asking, &mut look) else {
        panic!("asks");
    };
    asking.grants.push(Allowance::Once {
        account: question.account_key.clone(),
        batch: question.batch.clone(),
        target: question.bound.clone(),
        value: None,
    });
    assert_eq!(decide(&click, &asking, &mut look), Decision::Free);
    look.point = page("app.slack.com", "AXButton", Some("Delete message"));
    assert!(
        matches!(decide(&click, &asking, &mut look), Decision::Ask(_)),
        "another element there now"
    );
}

#[test]
fn the_same_site_in_two_browsers_is_two_accounts_to_a_grant() {
    let mut asking = listing(Kind::Site, "github.com", Level::Ask);
    asking.accounts.push(Listed {
        kind: Kind::Site,
        id: "github.com".to_owned(),
        name: "github.com".to_owned(),
        browser: Some("org.mozilla.firefox".to_owned()),
        level: Level::Ask,
    });
    asking.grants.push(Allowance::Task {
        account: AccountKey {
            kind: Kind::Site,
            id: "github.com".to_owned(),
            browser: Some("org.mozilla.firefox".to_owned()),
        },
    });
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let mut chrome = scripted(
        page("github.com", "AXButton", Some("Merge")),
        Target::default(),
    );
    assert!(
        matches!(decide(&click, &asking, &mut chrome), Decision::Ask(_)),
        "Firefox's grant, not Chrome's"
    );
}

#[test]
fn an_action_that_would_now_land_somewhere_stricter_stops_the_rest() {
    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let notes = Target {
        app: Some("com.apple.Notes".to_owned()),
        name: Some("Open".to_owned()),
        ..Target::default()
    };
    let mut look = scripted(notes.clone(), notes);
    let keys = actions(json!([{"type": "key", "key": "a"}, {"type": "key", "key": "b"}]));
    let (decision, mut recheck) = recheck(&keys, &asking, &mut look, None);
    assert_eq!(decision, Decision::Free, "judged in an app not listed");
    assert_eq!(recheck.check(&keys[0], &mut look), Ok(()));
    // Before the second key, Slack, kept Look Only, has come to the front.
    look.focus = Target {
        app: Some(SLACK.to_owned()),
        ..Target::default()
    };
    assert_eq!(recheck.check(&keys[1], &mut look), Err(SLACK.to_owned()));
}

#[test]
fn a_request_naming_an_app_not_yet_in_front_is_judged_as_that_app() {
    let looks = crate::native::ax::Looks::default();
    let epoch = atlas_core::session::InputEpoch {
        uid: 501,
        security_session_id: 1,
        display_id: 2,
        width: 800.0,
        height: 600.0,
        process_id: 1,
    };
    let screen = live::OnScreen {
        looks: &looks,
        epoch: &epoch,
        deadline: std::time::Instant::now(),
        intended: Some(SLACK.to_owned()),
    };
    let front = Target {
        app: Some("com.apple.TextEdit".to_owned()),
        editable: true,
        name: Some("Untitled".to_owned()),
        ..Target::default()
    };
    let intended = screen.as_intended(front);
    assert_eq!(intended.app.as_deref(), Some(SLACK));
    assert_eq!(
        (intended.name, intended.editable),
        (None, false),
        "what lies in it can't be told yet"
    );
    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let mut look = scripted(Target::default(), screen.as_intended(Target::default()));
    let send = actions(json!([{"type": "key", "key": "return"}]));
    assert!(matches!(
        decide(&send, &asking, &mut look),
        Decision::Deny { .. }
    ));
}

#[test]
fn a_browser_read_that_failed_counts_as_an_unread_page() {
    // A process that isn't running reads as no bundle at all; the rule is on what a browser's failed read gives.
    let unread = live::target(i64::from(std::process::id()), None);
    assert_ne!(unread.site, Site::Host(String::new()));
    let asking = listing(Kind::Site, "bank.example", Level::LookOnly);
    let browser = Target {
        app: Some(CHROME.to_owned()),
        site: Site::Unread,
        ..Target::default()
    };
    let mut look = scripted(browser, Target::default());
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    assert!(matches!(
        decide(&click, &asking, &mut look),
        Decision::Deny { .. }
    ));
}
