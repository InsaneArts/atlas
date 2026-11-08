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
    /// Whether focus stays on the element last read.
    unmoved: bool,
    /// How many times focus was read whole.
    focus_reads: usize,
}

impl Look for Scripted {
    fn at_point(&mut self, _: f64, _: f64) -> Target {
        self.point.clone()
    }
    fn focused(&mut self) -> Target {
        self.focus_reads += 1;
        self.focus.clone()
    }
    fn referenced(&mut self, _: &Reference) -> Target {
        self.referenced.clone()
    }
    fn message(&mut self) -> Message {
        self.messages += 1;
        self.message.clone()
    }
    fn unmoved(&mut self) -> bool {
        self.unmoved
    }
}

const CHROME: &str = "com.google.Chrome";
const SLACK: &str = "com.tinyspeck.slackmacgap";

/// A page on `host` in Chrome's window 7, with an element of `role` named
/// `name` there.
fn page(host: &str, role: &str, name: Option<&str>) -> Target {
    Target {
        app: Some(CHROME.to_owned()),
        site: Site::Host(host.to_owned()),
        window: Some(7),
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
        unmoved: false,
        focus_reads: 0,
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
    let keys = actions(json!([{"type": "type", "text": "a"}, {"type": "type", "text": "b"}]));
    let mut recheck = recheck(&keys, &asking);
    assert_eq!(recheck.check(&keys[0], &mut look), Decision::Free);
    // Before the second key, Slack, kept Look Only, has come to the front.
    look.focus = Target {
        app: Some(SLACK.to_owned()),
        ..Target::default()
    };
    assert_eq!(
        recheck.check(&keys[1], &mut look),
        Decision::Deny {
            account: SLACK.to_owned()
        }
    );
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
        held: None,
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

/// A look whose points differ: `from` everywhere but at x = 9, where `to` lies.
struct TwoPlaces {
    from: Target,
    to: Target,
}

impl Look for TwoPlaces {
    fn at_point(&mut self, x: f64, _: f64) -> Target {
        if (x - 9.0).abs() < f64::EPSILON {
            self.to.clone()
        } else {
            self.from.clone()
        }
    }
    fn focused(&mut self) -> Target {
        self.from.clone()
    }
    fn referenced(&mut self, _: &Reference) -> Target {
        self.from.clone()
    }
    fn message(&mut self) -> Message {
        Message::Unread
    }
    fn unmoved(&mut self) -> bool {
        false
    }
}

#[test]
fn a_drag_goes_by_where_it_drops_and_a_move_with_a_button_held_is_a_drag() {
    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let mut look = TwoPlaces {
        from: Target {
            app: Some("com.apple.finder".to_owned()),
            ..Target::default()
        },
        to: Target {
            app: Some(SLACK.to_owned()),
            ..Target::default()
        },
    };
    let into_slack = actions(json!([{"type": "drag", "fromX": 1, "fromY": 1, "toX": 9, "toY": 1}]));
    assert!(matches!(
        decide(&into_slack, &asking, &mut look),
        Decision::Deny { .. }
    ));

    let held = actions(json!([
        {"type": "pointer", "phase": "down", "x": 9, "y": 1},
        {"type": "pointer", "phase": "move", "x": 9, "y": 2},
    ]));
    assert!(matches!(
        decide(&held, &asking, &mut look),
        Decision::Deny { .. }
    ));
}

#[test]
fn a_character_key_into_a_password_field_follows_the_password_rule() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut secure = field("app.slack.com", false);
    secure.secure = true;
    let mut look = scripted(Target::default(), secure);
    for key in ["a", "shift+a", "7"] {
        let decision = decide(
            &actions(json!([{"type": "key", "key": key}])),
            &asking,
            &mut look,
        );
        assert_eq!(what(&decision), Some("password"), "{key}");
    }
    assert_eq!(look.messages, 0, "the field is never read");
}

/// The once grant `question` offers, as the person's Just This Once gives it.
fn once(question: &Question) -> Allowance {
    let value = match &question.message {
        Some(Message::Whole { digest, .. }) => Some(digest.clone()),
        Some(Message::Unread) | None => None,
    };
    Allowance::Once {
        account: question.account_key.clone(),
        batch: question.batch.clone(),
        target: question.bound.clone(),
        value,
    }
}

#[test]
fn an_action_judged_again_stops_when_the_same_account_now_has_a_control_that_sends_there() {
    let asking = listing(Kind::Site, "github.com", Level::Ask);
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let mut look = scripted(
        page("github.com", "AXButton", Some("Open")),
        Target::default(),
    );
    let mut again = recheck(&click, &asking);
    look.point = page("github.com", "AXButton", Some("Send"));
    assert_eq!(
        what(&again.check(&click[0], &mut look)),
        Some("click"),
        "Open became Send at the same level"
    );
}

#[test]
fn an_action_judged_again_keeps_each_grant_to_its_own_account_window_and_element() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    asking.accounts.push(Listed {
        kind: Kind::Site,
        id: "github.com".to_owned(),
        name: "github.com".to_owned(),
        browser: Some(CHROME.to_owned()),
        level: Level::Ask,
    });
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let send = page("app.slack.com", "AXButton", Some("Send"));
    let mut look = scripted(send.clone(), Target::default());

    let mut task = asking.clone();
    task.grants.push(Allowance::Task {
        account: site("slack.com"),
    });
    let mut again = recheck(&click, &task);
    look.point = page("github.com", "AXButton", Some("Send"));
    assert!(
        matches!(again.check(&click[0], &mut look), Decision::Ask(_)),
        "another account at the same level"
    );

    look.point = send.clone();
    let Decision::Ask(question) = decide(&click, &asking, &mut look) else {
        panic!("asks");
    };
    asking.grants.push(once(&question));
    let mut again = recheck(&click, &asking);
    assert_eq!(again.check(&click[0], &mut look), Decision::Free);
    look.point = Target {
        window: Some(8),
        ..send.clone()
    };
    assert!(
        matches!(again.check(&click[0], &mut look), Decision::Ask(_)),
        "the same control in another window"
    );
    look.point = Target {
        identifier: Some("delete".to_owned()),
        ..send
    };
    assert!(
        matches!(again.check(&click[0], &mut look), Decision::Ask(_)),
        "another element by the same name"
    );
}

#[test]
fn between_characters_focus_that_stayed_is_not_read_again_and_focus_that_moved_is_judged_again() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let typing = actions(json!([{"type": "type", "text": "hunter2"}]));
    let mut look = scripted(Target::default(), field("app.slack.com", true));
    let mut again = recheck(&typing, &asking);
    assert_eq!(again.check(&typing[0], &mut look), Decision::Free);
    let reads = look.focus_reads;
    look.unmoved = true;
    assert_eq!(again.event(&mut look), Decision::Free);
    assert_eq!(
        look.focus_reads, reads,
        "focus that stayed is not read again"
    );

    look.unmoved = false;
    look.focus.secure = true;
    assert_eq!(
        what(&again.event(&mut look)),
        Some("password"),
        "focus moved to a password field after a character"
    );
    look.focus = field("bank.example", false);
    let mut bank = asking.clone();
    bank.accounts[0].id = "bank.example".to_owned();
    bank.accounts[0].level = Level::LookOnly;
    let mut again = recheck(&typing, &bank);
    let _ = again.check(&typing[0], &mut look);
    assert!(
        matches!(again.event(&mut look), Decision::Deny { .. }),
        "the page is now a Look Only site"
    );
}

#[test]
fn a_typing_commands_grant_covers_only_its_own_text_into_the_field_and_window_it_was_asked_about() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    asking.typing = Some("whole text".to_owned());
    asking.dry = true;
    let mut secure = field("app.slack.com", false);
    secure.secure = true;
    let mut look = scripted(Target::default(), secure.clone());
    let probe = actions(json!([{"type": "type", "text": "x"}]));
    let Decision::Ask(question) = decide(&probe, &asking, &mut look) else {
        panic!("a password field asks");
    };
    assert_eq!(question.details()["typing"], "whole text");
    assert_eq!(question.details()["bind"]["typing"], "whole text");
    asking.dry = false;
    asking.grants.push(Allowance::Command {
        account: question.account_key.clone(),
        target: question.bound.clone(),
        typing: "whole text".to_owned(),
    });
    let part = actions(json!([{"type": "type", "text": "hunter2"}]));
    assert_eq!(
        decide(&part, &asking, &mut look),
        Decision::Free,
        "its own text, into that field"
    );

    let mut other = asking.clone();
    other.typing = Some("other text".to_owned());
    assert!(
        matches!(decide(&part, &other, &mut look), Decision::Ask(_)),
        "another command's text"
    );
    for moved in [
        Target {
            window: Some(8),
            ..secure.clone()
        },
        Target {
            name: Some("Search".to_owned()),
            ..secure.clone()
        },
    ] {
        look.focus = moved;
        assert!(
            matches!(decide(&part, &asking, &mut look), Decision::Ask(_)),
            "another field or window"
        );
    }
    look.focus = secure;
    let key = actions(json!([{"type": "key", "key": "a"}]));
    assert!(
        matches!(decide(&key, &asking, &mut look), Decision::Ask(_)),
        "never a key"
    );
}

#[test]
fn a_keyboard_once_grant_is_bound_to_the_window_the_focused_field_is_in() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    let mut look = scripted(Target::default(), field("app.slack.com", true));
    let send = actions(json!([{"type": "key", "key": "return"}]));
    let Decision::Ask(question) = decide(&send, &asking, &mut look) else {
        panic!("asks");
    };
    assert_eq!(question.bound.window, Some(7));
    asking.grants.push(once(&question));
    assert_eq!(decide(&send, &asking, &mut look), Decision::Free);

    look.focus.window = Some(8);
    assert!(
        matches!(decide(&send, &asking, &mut look), Decision::Ask(_)),
        "the same field, name and message in another window"
    );
    look.focus.window = None;
    let Decision::Ask(question) = decide(&send, &asking, &mut look) else {
        panic!("asks");
    };
    assert_eq!(
        question.details()["once"],
        false,
        "a field whose window can't be told can't be allowed just once"
    );
}

/// The person lists slack.com and github.com in Chrome, both asking.
fn two_sites() -> Asking {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    asking.accounts.push(Listed {
        kind: Kind::Site,
        id: "github.com".to_owned(),
        name: "github.com".to_owned(),
        browser: Some(CHROME.to_owned()),
        level: Level::Ask,
    });
    asking
}

#[test]
fn a_page_that_cant_be_read_is_no_one_sites_so_no_grant_for_a_site_covers_it() {
    let mut asking = two_sites();
    asking.grants.push(Allowance::Task {
        account: site("slack.com"),
    });
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let unread = Target {
        site: Site::Unread,
        ..page("app.slack.com", "AXButton", Some("Send"))
    };
    let mut look = scripted(unread, Target::default());
    let Decision::Ask(question) = decide(&click, &asking, &mut look) else {
        panic!("slack.com's grant doesn't cover a page that could be github.com");
    };
    assert_eq!(
        question.details()["once"],
        false,
        "nor can it be allowed once"
    );
    asking.grants.push(once(&question));
    asking.grants.push(Allowance::Task {
        account: question.account_key.clone(),
    });
    assert!(
        matches!(decide(&click, &asking, &mut look), Decision::Ask(_)),
        "it asks until the page is read"
    );
    look.point = page("app.slack.com", "AXButton", Some("Send"));
    assert_eq!(decide(&click, &asking, &mut look), Decision::Free);
}

#[test]
fn a_drag_between_two_accounts_needs_each_of_them_to_allow_it() {
    let asking = two_sites();
    let mut look = TwoPlaces {
        from: page("app.slack.com", "AXGroup", Some("Message")),
        to: page("github.com", "AXGroup", Some("Comment")),
    };
    let drag = actions(json!([{"type": "drag", "fromX": 1, "fromY": 1, "toX": 9, "toY": 1}]));

    let mut source = asking.clone();
    source.grants.push(Allowance::Task {
        account: site("slack.com"),
    });
    let Decision::Ask(question) = decide(&drag, &source, &mut look) else {
        panic!("the source's grant doesn't cover where it drops");
    };
    assert_eq!(question.account, "github.com");
    source.grants.push(Allowance::Task {
        account: site("github.com"),
    });
    assert_eq!(decide(&drag, &source, &mut look), Decision::Free);

    let mut once_only = asking;
    let Decision::Ask(question) = decide(&drag, &once_only, &mut look) else {
        panic!("asks");
    };
    assert!(question.bound.drop.is_some(), "bound where it drops too");
    once_only.grants.push(once(&question));
    assert_eq!(decide(&drag, &once_only, &mut look), Decision::Free);
    look.to = page("github.com", "AXButton", Some("Delete branch"));
    assert!(
        matches!(decide(&drag, &once_only, &mut look), Decision::Ask(_)),
        "another place to drop"
    );
}

#[test]
fn with_a_button_held_a_move_or_a_scroll_is_a_drag_where_it_lands() {
    let mut look = TwoPlaces {
        from: Target {
            app: Some("com.apple.finder".to_owned()),
            ..Target::default()
        },
        to: Target {
            app: Some(SLACK.to_owned()),
            window: Some(7),
            ..Target::default()
        },
    };
    let press_then = |action: serde_json::Value| {
        actions(json!([
            {"type": "pointer", "phase": "down", "x": 1, "y": 1},
            action,
        ]))
    };
    let moves = [
        json!({"type": "move", "x": 9, "y": 1}),
        json!({"type": "scroll", "x": 9, "y": 1, "dy": 3}),
    ];
    let look_only = listing(Kind::App, SLACK, Level::LookOnly);
    let asks = listing(Kind::App, SLACK, Level::Ask);
    for moved in moves {
        let held = press_then(moved.clone());
        assert!(
            matches!(decide(&held, &look_only, &mut look), Decision::Deny { .. }),
            "{moved} into Look Only"
        );
        assert_eq!(
            what(&decide(&held, &asks, &mut look)),
            Some("drag"),
            "{moved}"
        );
        assert_eq!(
            decide(&actions(json!([moved.clone()])), &look_only, &mut look),
            Decision::Free,
            "{moved} with no button held only looks"
        );
    }
}

#[test]
fn while_the_list_cant_be_read_no_task_grant_covers_anything_and_none_is_offered() {
    let mut asking = Asking {
        unknown: true,
        ..Asking::default()
    };
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let send = page("app.slack.com", "AXButton", Some("Send"));
    let mut look = scripted(send.clone(), Target::default());
    let Decision::Ask(question) = decide(&click, &asking, &mut look) else {
        panic!("asks");
    };
    assert_eq!(question.details()["task"], false, "no Allow for This Task");
    assert_eq!(question.details()["once"], true);
    asking.grants.push(Allowance::Task {
        account: question.account_key.clone(),
    });
    assert!(
        matches!(decide(&click, &asking, &mut look), Decision::Ask(_)),
        "a task grant from before covers nothing"
    );
    asking.grants.push(once(&question));
    assert_eq!(decide(&click, &asking, &mut look), Decision::Free);
    look.point = page("github.com", "AXButton", Some("Send"));
    assert!(
        matches!(decide(&click, &asking, &mut look), Decision::Ask(_)),
        "once is bound to where it was asked about"
    );
}

#[test]
fn a_floating_panel_or_menu_of_a_listed_app_over_another_app_is_judged_as_the_listed_app() {
    let window = |number: i64, pid: i64, layer: i64| atlas_core::screen::windows::Window {
        number,
        pid,
        owner: None,
        title: None,
        layer,
        alpha: 1.0,
        bounds: atlas_core::screen::geometry::Bounds {
            x: 0.0,
            y: 30.0,
            width: 400.0,
            height: 300.0,
        },
        system: None,
    };
    let display = atlas_core::screen::geometry::Bounds::display(800.0, 600.0);
    // Front to back: the listed app's floating panel, then its open menu
    // elsewhere, over an unlisted app's ordinary window.
    let panel = vec![window(11, 812, 3), window(10, 900, 0)];
    let received = live::receiving(&panel, display.clone(), 50.0, 50.0).unwrap();
    assert_eq!((received.pid, received.number), (812, 11));
    let menu = vec![window(12, 812, 101), window(10, 900, 0)];
    assert_eq!(
        live::receiving(&menu, display, 50.0, 50.0).unwrap().pid,
        812
    );

    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let slack = Target {
        app: Some(SLACK.to_owned()),
        window: Some(11),
        name: Some("Open".to_owned()),
        ..Target::default()
    };
    let mut look = scripted(slack, Target::default());
    let click = actions(json!([{"type": "click", "x": 50, "y": 50}]));
    assert!(matches!(
        decide(&click, &asking, &mut look),
        Decision::Deny { .. }
    ));
}

#[test]
fn a_check_before_an_event_judges_the_app_it_finds_not_the_one_the_request_named() {
    let looks = crate::native::ax::Looks::default();
    let epoch = atlas_core::session::InputEpoch {
        uid: 501,
        security_session_id: 1,
        display_id: 2,
        width: 800.0,
        height: 600.0,
        process_id: 1,
    };
    let screen = live::OnScreen::observing(&looks, &epoch, std::time::Instant::now(), None);
    let slack = Target {
        app: Some(SLACK.to_owned()),
        editable: true,
        ..Target::default()
    };
    assert_eq!(screen.as_intended(slack.clone()), slack);

    // Typing into TextEdit, which isn't listed, when Slack, kept Look Only,
    // takes focus partway.
    let asking = listing(Kind::App, SLACK, Level::LookOnly);
    let typing = actions(json!([{"type": "type", "text": "hello"}]));
    let mut look = scripted(
        Target::default(),
        Target {
            app: Some("com.apple.TextEdit".to_owned()),
            editable: true,
            ..Target::default()
        },
    );
    let mut again = recheck(&typing, &asking);
    assert_eq!(again.check(&typing[0], &mut look), Decision::Free);
    look.focus = slack;
    assert!(matches!(again.event(&mut look), Decision::Deny { .. }));
}

/// The permit the person's Allow gives for the action `question` stopped.
fn permit(question: &Question) -> Allowance {
    let value = match &question.message {
        Some(Message::Whole { digest, .. }) => Some(digest.clone()),
        Some(Message::Unread) | None => None,
    };
    Allowance::Permit {
        account: question.account_key.clone(),
        action: question.action.clone(),
        target: question.bound.clone(),
        value,
    }
}

/// Slack's message field, in its window 7, with the draft `typed` in it.
fn composer(typed: &str) -> (Target, Message) {
    let field = Target {
        editable: true,
        multiline: false,
        ..page("app.slack.com", "AXTextField", Some("Message"))
    };
    (field, Message::whole(typed))
}

#[test]
fn a_draft_clicked_and_typed_never_asks_and_return_asks_once_about_the_typed_message() {
    let asking = listing(Kind::Site, "slack.com", Level::Ask);
    let (field, _) = composer("");
    let mut look = scripted(field.clone(), field);
    let request = actions(json!([
        {"type": "click", "x": 1, "y": 1},
        {"type": "type", "text": "hello team"},
        {"type": "key", "key": "return"},
    ]));
    assert_eq!(
        decide(&request[..2], &asking, &mut look),
        Decision::Free,
        "a draft"
    );
    let mut each = recheck(&request, &asking);
    assert_eq!(each.check(&request[0], &mut look), Decision::Free);
    assert_eq!(each.check(&request[1], &mut look), Decision::Free);
    // What Return would send is read as it is when Return is reached.
    look.message = Message::whole("hello team");
    let Decision::Ask(question) = each.check(&request[2], &mut look) else {
        panic!("Return in the field sends");
    };
    assert_eq!(question.what, "send-key");
    assert_eq!(question.details()["message"]["preview"], "hello team");
    assert_eq!(look.messages, 1, "asked once, at the step that sends");
}

#[test]
fn a_permit_covers_exactly_the_action_it_was_given_for() {
    let mut asking = listing(Kind::Site, "slack.com", Level::Ask);
    let (field, message) = composer("hello team");
    let mut look = scripted(page("app.slack.com", "AXButton", Some("Send")), field);
    look.message = message;
    let send = actions(json!([{"type": "key", "key": "return"}]));
    let click = actions(json!([{"type": "click", "x": 1, "y": 1}]));
    let typed = actions(json!([{"type": "type", "text": "hunter2\n"}]));
    for stopped in [&send, &click, &typed] {
        let Decision::Ask(question) = decide(stopped, &asking, &mut look) else {
            panic!("asks");
        };
        asking.grants.push(permit(&question));
        assert_eq!(decide(stopped, &asking, &mut look), Decision::Free);
    }
    // Another request holding the same action is covered too: it's the
    // action, not the request, that was allowed.
    let after = actions(json!([
        {"type": "key", "key": "down"},
        {"type": "key", "key": "return"},
    ]));
    assert_eq!(decide(&after, &asking, &mut look), Decision::Free);
    for other in [
        json!({"type": "key", "key": "enter"}),
        json!({"type": "click", "x": 1, "y": 1, "count": 2}),
        json!({"type": "type", "text": "hunter3\n"}),
    ] {
        assert!(
            matches!(
                decide(&actions(json!([other.clone()])), &asking, &mut look),
                Decision::Ask(_)
            ),
            "{other}"
        );
    }
    look.message = Message::whole("hello everyone");
    assert!(
        matches!(decide(&send, &asking, &mut look), Decision::Ask(_)),
        "another message"
    );
}
