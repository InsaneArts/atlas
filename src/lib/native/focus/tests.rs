//! Checks that an action is sent only where it would land, refused with
//! the reason when anything is out of place, and that the check itself
//! never moves focus.
use super::*;
use crate::native::ax::Frame;

const APP: i64 = 812;
const OTHER: i64 = 900;

fn display() -> Bounds {
    Bounds {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    }
}

fn window(pid: i64, owner: &str, layer: i64, bounds: (f64, f64, f64, f64)) -> Window {
    // Each window its own number, as the window list gives them.
    static NUMBER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
    Window {
        number: NUMBER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        pid,
        owner: Some(owner.to_owned()),
        title: Some("Secret plans.txt".to_owned()),
        layer,
        alpha: 1.0,
        bounds: Bounds {
            x: bounds.0,
            y: bounds.1,
            width: bounds.2,
            height: bounds.3,
        },
    }
}

fn menu_bar() -> Window {
    window(
        1,
        "Window Server",
        windows::MENU_BAR_LAYER,
        (0.0, 0.0, 1000.0, 24.0),
    )
}

/// A helper as a test sets it, counting each read.
#[derive(Default)]
struct Seen {
    focused: Option<Result<Option<i64>>>,
    windows: Option<Vec<Window>>,
    /// Where the Dock takes clicks; no icons unless a test sets them.
    dock: Option<Dock>,
    /// Where focus is inside the app; unreadable unless a test sets it.
    keyboard: Option<Keyboard>,
    /// Why the keyboard read fails when it does, and how long it takes.
    unread: Option<Unread>,
    slow: Duration,
    focus_reads: usize,
    window_reads: usize,
    keyboard_reads: usize,
    app_focused: bool,
}

impl Seen {
    fn focused(pid: Option<i64>) -> Self {
        Self {
            focused: Some(Ok(pid)),
            ..Self::default()
        }
    }

    fn showing(mut self, windows: Vec<Window>) -> Self {
        self.windows = Some(windows);
        self
    }

    fn docked(mut self, dock: Dock) -> Self {
        self.dock = Some(dock);
        self
    }

    fn typing_in(mut self, keyboard: Keyboard) -> Self {
        self.keyboard = Some(keyboard);
        self
    }
}

impl Look for Seen {
    fn app_focused(&mut self, _: i64) -> Result<bool> {
        Ok(self.app_focused)
    }

    fn focused(&mut self) -> Result<Option<i64>> {
        self.focus_reads += 1;
        match &self.focused {
            Some(Ok(pid)) => Ok(*pid),
            _ => Err(Error::new(Code::Native, "unreadable")),
        }
    }

    fn keyboard(&mut self, _: i64, _: Judged) -> Result<Keyboard> {
        self.keyboard_reads += 1;
        std::thread::sleep(self.slow);
        if let Some(why) = self.unread {
            return Err(why.error());
        }
        self.keyboard
            .clone()
            .ok_or_else(|| Error::new(Code::Native, "unreadable"))
    }

    fn windows(&mut self) -> Result<&[Window]> {
        self.window_reads += 1;
        self.windows
            .as_deref()
            .ok_or_else(|| Error::new(Code::Windows, "unreadable"))
    }

    fn dock(&mut self) -> Dock {
        self.dock.clone().unwrap_or(Dock::Icons(Vec::new()))
    }
}

fn expect() -> Expect {
    Expect::new(APP).unwrap()
}

pub(in crate::native) fn verify_keyboard(
    expect: &Expect,
    keyboard: Keyboard,
    listed: Vec<Window>,
    deadline: Instant,
    slow: Duration,
) -> Result<Option<&'static str>> {
    let mut look = Seen::focused(Some(expect.pid()))
        .showing(listed)
        .typing_in(keyboard);
    look.slow = slow;
    require(expect, Need::Keys, &display(), &mut look, deadline)
}

pub(in crate::native) fn verify_pointer(
    expect: &Expect,
    need: Need,
    number: Option<i64>,
    focused: i64,
    keyboard: Keyboard,
    listed: Vec<Window>,
    deadline: Instant,
) -> Result<Option<&'static str>> {
    let mut look = Seen::focused(Some(focused))
        .showing(listed)
        .typing_in(keyboard);
    prepared(expect, need, number, &display(), &mut look, deadline)
}

/// The refusal's code, reason and what it found, as an agent reads them.
fn refusal(outcome: Result<Option<&str>>) -> serde_json::Value {
    let error = outcome.unwrap_err();
    assert_eq!(error.code, "focus-changed");
    assert!(!error.uncertain);
    assert!(error.error.ends_with(NOT_SENT), "{}", error.error);
    json!({"reason": error.details["reason"], "found": error.details.get("found")})
}

fn keys_with(look: &mut Seen) -> Result<Option<&'static str>> {
    require(&expect(), Need::Keys, &display(), look, later())
}

fn point_with(look: &mut Seen, x: f64, y: f64) -> Result<Option<&'static str>> {
    require(&expect(), Need::Point(x, y), &display(), look, later())
}

#[test]
fn keys_go_ahead_only_while_the_app_has_keyboard_focus() {
    assert!(keys_with(&mut Seen::focused(Some(APP))).is_ok());

    let calculator = window(OTHER, "Calculator", 0, (0.0, 0.0, 200.0, 300.0));
    let mut other = Seen::focused(Some(OTHER)).showing(vec![calculator]);
    assert_eq!(
        refusal(keys_with(&mut other)),
        json!({"reason": "app", "found": {"app": "Calculator", "pid": OTHER}})
    );

    let mut unnamed = Seen::focused(Some(OTHER)).showing(Vec::new());
    assert_eq!(
        refusal(keys_with(&mut unnamed)),
        json!({"reason": "app", "found": {"pid": OTHER}})
    );

    let mut nobody = Seen::focused(None);
    assert_eq!(
        refusal(keys_with(&mut nobody)),
        json!({"reason": "app", "found": null})
    );
}

#[test]
fn keys_ask_for_no_window_list_unless_refused() {
    let mut look = Seen::focused(Some(APP)).showing(Vec::new());
    keys_with(&mut look).unwrap();
    assert_eq!((look.focus_reads, look.window_reads), (1, 0));
}

#[test]
fn a_point_goes_ahead_only_where_the_apps_own_window_is_on_top() {
    let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
    let sheet = window(APP, "TextEdit", 0, (150.0, 100.0, 300.0, 150.0));
    let menu = window(
        APP,
        "TextEdit",
        windows::MENU_LAYER,
        (120.0, 24.0, 150.0, 200.0),
    );
    let panel = window(APP, "TextEdit", 3, (600.0, 100.0, 200.0, 200.0));
    let calculator = window(OTHER, "Calculator", 0, (400.0, 300.0, 200.0, 300.0));
    let dock = window(2, "Dock", 20, (0.0, 760.0, 1000.0, 40.0));
    let status = window(3, "Control Center", 25, (900.0, 0.0, 30.0, 24.0));
    let listed = vec![
        menu,
        status,
        dock,
        panel,
        sheet,
        calculator,
        document,
        menu_bar(),
    ];
    let look = || Seen::focused(Some(APP)).showing(listed.clone());

    for (x, y, what) in [
        (110.0, 350.0, "its document"),
        (200.0, 150.0, "its sheet"),
        (150.0, 100.0, "its open menu"),
        (650.0, 150.0, "its floating panel"),
    ] {
        assert!(point_with(&mut look(), x, y).is_ok(), "{what}");
    }

    assert_eq!(
        refusal(point_with(&mut look(), 450.0, 350.0)),
        json!({"reason": "point", "found": {"app": "Calculator", "pid": OTHER}}),
        "another app's window in front"
    );
    assert_eq!(
        refusal(point_with(&mut look(), 300.0, 780.0))["found"]["app"],
        "Dock"
    );
    assert_eq!(
        refusal(point_with(&mut look(), 910.0, 10.0))["found"]["app"],
        "Control Center"
    );
    assert_eq!(
        refusal(point_with(&mut look(), 50.0, 600.0)),
        json!({"reason": "point", "found": null}),
        "nothing there"
    );
}

#[test]
fn the_menu_bar_counts_as_the_apps_only_while_it_has_keyboard_focus() {
    let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
    let calculator = window(OTHER, "Calculator", 0, (400.0, 300.0, 200.0, 300.0));
    let listed = vec![menu_bar(), calculator, document];

    let mut front = Seen::focused(Some(APP)).showing(listed.clone());
    assert!(point_with(&mut front, 200.0, 10.0).is_ok());

    let mut behind = Seen::focused(Some(OTHER)).showing(listed.clone());
    assert_eq!(
        refusal(point_with(&mut behind, 200.0, 10.0)),
        json!({"reason": "point", "found": {"app": "Calculator", "pid": OTHER}})
    );

    let mut nobody = Seen::focused(None).showing(listed);
    assert_eq!(
        refusal(point_with(&mut nobody, 200.0, 10.0)),
        json!({"reason": "point", "found": null})
    );
}

#[test]
fn click_through_overlays_do_not_hide_the_window_under_them() {
    let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
    for owner in ["Dock", "Notification Center"] {
        let overlay = window(2, owner, 20, (0.0, 0.0, 1000.0, 800.0));
        let mut look = Seen::focused(Some(APP)).showing(vec![overlay, document.clone()]);
        assert!(point_with(&mut look, 200.0, 200.0).is_ok(), "{owner}");
    }

    let other = window(2, "Shade", 20, (0.0, 0.0, 1000.0, 800.0));
    let mut look = Seen::focused(Some(APP)).showing(vec![other, document]);
    assert_eq!(
        refusal(point_with(&mut look, 200.0, 200.0))["found"]["app"],
        "Shade"
    );
}

/// The test display's Dock, its icons along the bottom edge.
fn icons() -> Dock {
    Dock::Icons(vec![Bounds {
        x: 200.0,
        y: 740.0,
        width: 600.0,
        height: 60.0,
    }])
}

/// The Dock's window, an app's window partly under its icons, and
/// another app's window behind both.
fn under_the_dock() -> Vec<Window> {
    vec![
        window(2, "Dock", 20, (0.0, 0.0, 1000.0, 800.0)),
        window(APP, "Calculator", 0, (150.0, 600.0, 300.0, 200.0)),
        window(OTHER, "Finder", 0, (0.0, 0.0, 1000.0, 800.0)),
    ]
}

/// A check of `need` for `pid` with the Dock as `dock`.
fn docked_need(pid: i64, dock: &Dock, need: Need) -> Result<Option<&'static str>> {
    let mut look = Seen::focused(Some(pid))
        .showing(under_the_dock())
        .docked(dock.clone());
    require(
        &Expect::new(pid).unwrap(),
        need,
        &display(),
        &mut look,
        later(),
    )
}

/// The Dock's full-display window lets clicks through except on its
/// icons and the shelf around them: there it is on top for every app,
/// even over an app's own window under it.
#[test]
fn the_docks_icons_refuse_every_app_and_the_points_beside_them_pass() {
    let at = |pid, x, y| docked_need(pid, &icons(), Need::Point(x, y));
    let dock = json!({"reason": "point", "found": {"app": "Dock", "pid": 2}});
    for (pid, x, y, what) in [
        (APP, 300.0, 770.0, "an icon"),
        (OTHER, 300.0, 770.0, "an icon"),
        (OTHER, 790.0, 799.0, "an icon at the edge"),
        (APP, 195.0, 770.0, "the shelf beside the icons"),
        (APP, 300.0, 732.0, "the shelf above the icons"),
    ] {
        assert_eq!(refusal(at(pid, x, y)), dock, "{pid}: {what}");
    }

    assert!(at(APP, 170.0, 770.0).is_ok(), "beside the shelf");
    assert!(at(APP, 300.0, 700.0).is_ok(), "above the shelf");
    assert!(at(OTHER, 900.0, 770.0).is_ok(), "beside the shelf");
}

/// A Dock that shows more than its icons, or could not be read, covers
/// all of its window: what else it shows takes clicks too.
#[test]
fn a_dock_that_is_not_only_its_icons_covers_the_whole_display() {
    for (x, y) in [(300.0, 700.0), (500.0, 300.0), (10.0, 10.0)] {
        assert_eq!(
            refusal(docked_need(OTHER, &Dock::Other, Need::Point(x, y))),
            json!({"reason": "point", "found": {"app": "Dock", "pid": 2}}),
            "{x},{y}"
        );
        assert_eq!(
            refusal(docked_need(OTHER, &Dock::Unread, Need::Point(x, y))),
            json!({"reason": "unreadable", "found": null}),
            "{x},{y}"
        );
    }
}

/// A drag needs the app's window on top where it drops as well as
/// where it starts, so nothing is dropped on a Dock icon or the Trash.
#[test]
fn a_drag_is_refused_when_it_would_drop_on_the_dock() {
    let drag = |to| Need::Drag {
        from: (300.0, 650.0),
        to,
    };
    assert!(docked_need(APP, &icons(), drag((400.0, 700.0))).is_ok());

    let onto_icon = refusal(docked_need(APP, &icons(), drag((400.0, 770.0))));
    assert_eq!(
        onto_icon,
        json!({"reason": "point", "found": {"app": "Dock", "pid": 2}})
    );
    let error = docked_need(APP, &icons(), drag((400.0, 770.0))).unwrap_err();
    assert!(error.error.contains("the drop point"), "{}", error.error);

    assert_eq!(
        refusal(docked_need(APP, &icons(), drag((900.0, 300.0))))["found"]["app"],
        "Finder",
        "a drop on another app"
    );
    let from_icon = Need::Drag {
        from: (300.0, 770.0),
        to: (400.0, 700.0),
    };
    assert_eq!(
        refusal(docked_need(APP, &icons(), from_icon))["found"]["app"],
        "Dock"
    );
}

#[test]
fn each_action_needs_what_it_touches_first() {
    let need =
        |action: serde_json::Value| Need::of(&serde_json::from_value::<Action>(action).unwrap());

    assert_eq!(need(json!({"type": "type", "text": "hi"})), Need::Keys);
    assert_eq!(need(json!({"type": "key", "key": "cmd+s"})), Need::Keys);
    assert_eq!(
        need(json!({"type": "click", "x": 1, "y": 2})),
        Need::Point(1.0, 2.0)
    );
    assert_eq!(
        need(json!({"type": "scroll", "x": 3, "y": 4, "dy": 1})),
        Need::Point(3.0, 4.0)
    );
    assert_eq!(
        need(json!({"type": "drag", "fromX": 5, "fromY": 6, "toX": 7, "toY": 8})),
        Need::Drag {
            from: (5.0, 6.0),
            to: (7.0, 8.0)
        },
        "a drag needs its start and its drop"
    );
    for phase in ["down", "move", "up"] {
        assert_eq!(
            need(json!({"type": "pointer", "phase": phase, "x": 9, "y": 10})),
            Need::Point(9.0, 10.0),
            "a pointer's {phase} may press, carry or drop"
        );
    }
    assert_eq!(
        need(json!({"type": "move", "x": 1, "y": 2})),
        Need::Point(1.0, 2.0),
        "a move may carry what a press holds"
    );
    assert_eq!(need(json!({"type": "wait", "ms": 10})), Need::Nothing);
}

#[test]
fn nothing_needed_is_never_looked_for() {
    let mut look = Seen::default();
    require(
        &expect(),
        Need::Nothing,
        &display(),
        &mut look,
        Instant::now(),
    )
    .unwrap();
    assert_eq!((look.focus_reads, look.window_reads), (0, 0));
}

#[test]
fn a_focus_that_cannot_be_read_refuses() {
    let unreadable = json!({"reason": "unreadable", "found": null});
    assert_eq!(refusal(keys_with(&mut Seen::default())), unreadable);

    let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
    let mut no_list = Seen::focused(Some(APP));
    assert_eq!(refusal(point_with(&mut no_list, 200.0, 200.0)), unreadable);

    let mut no_focus = Seen::default().showing(vec![menu_bar(), document]);
    assert_eq!(refusal(point_with(&mut no_focus, 200.0, 10.0)), unreadable);
}

/// A deadline well past any test's own reads.
fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

#[test]
fn a_look_that_ends_after_its_deadline_refuses() {
    let document = window(APP, "TextEdit", 0, (100.0, 100.0, 400.0, 300.0));
    for need in [Need::Keys, Need::Point(200.0, 200.0)] {
        // The look answers as if in time; the deadline has passed by
        // the time it ends.
        let mut late = Seen::focused(Some(APP)).showing(vec![document.clone()]);
        let deadline = Instant::now();
        assert!(require(&expect(), need, &display(), &mut late, later()).is_ok());
        assert_eq!(
            refusal(require(&expect(), need, &display(), &mut late, deadline)),
            json!({"reason": "unreadable", "found": null}),
            "{need:?}"
        );
    }
}

#[test]
fn checks_stop_once_their_time_is_spent() {
    let now = Instant::now();
    let mut spent = Live::until(now);
    for need in [Need::Keys, Need::Point(10.0, 10.0)] {
        assert_eq!(
            refusal(require(&expect(), need, &display(), &mut spent, now)),
            json!({"reason": "unreadable", "found": null}),
            "{need:?}"
        );
    }
    assert!(spent.windows.is_none(), "nothing was read");
}

#[test]
fn a_refusal_names_what_was_there_but_no_title() {
    let calculator = window(OTHER, "Calculator", 0, (0.0, 0.0, 1000.0, 800.0));
    for outcome in [
        keys_with(&mut Seen::focused(Some(OTHER)).showing(vec![calculator.clone()])),
        point_with(
            &mut Seen::focused(Some(APP)).showing(vec![calculator]),
            5.0,
            5.0,
        ),
    ] {
        let error = outcome.unwrap_err();
        let said = serde_json::to_string(&error).unwrap();
        assert!(!said.contains("Secret plans"), "{said}");
        assert!(error.error.starts_with("Calculator"), "{}", error.error);
    }
}

/// The document window the window and field tests type into.
const DOCUMENT: i64 = 4120;

/// A listed window with the number `number`.
fn numbered(number: i64, pid: i64, title: Option<&str>, bounds: (f64, f64, f64, f64)) -> Window {
    Window {
        number,
        pid,
        owner: Some(if pid == APP { "TextEdit" } else { "Calculator" }.to_owned()),
        title: title.map(str::to_owned),
        layer: 0,
        alpha: 1.0,
        bounds: Bounds {
            x: bounds.0,
            y: bounds.1,
            width: bounds.2,
            height: bounds.3,
        },
    }
}

/// The document as the window list shows it.
fn document() -> Window {
    numbered(
        DOCUMENT,
        APP,
        Some("Letter.txt"),
        (100.0, 100.0, 400.0, 300.0),
    )
}

/// An element as a focus read reports it: a role, a frame and the
/// names given.
fn element(role: &str, frame: (f64, f64, f64, f64)) -> Element {
    Element {
        index: 0,
        depth: 0,
        window: None,
        role: Some(role.to_owned()),
        subrole: None,
        title: None,
        description: None,
        value: None,
        identifier: None,
        enabled: Some(true),
        focused: Some(true),
        selected: None,
        frame: Some(Frame {
            x: frame.0,
            y: frame.1,
            width: frame.2,
            height: frame.3,
        }),
        unreadable: Vec::new(),
        truncated: Vec::new(),
        menu: false,
        mark: None,
    }
}

/// The document as Accessibility reports it when it is the focused
/// window.
fn document_window() -> Element {
    Element {
        title: Some("Letter.txt".to_owned()),
        subrole: Some("AXStandardWindow".to_owned()),
        ..element("AXWindow", (100.0, 100.0, 400.0, 300.0))
    }
}

/// A text field named `name` by its description, as `elements` would
/// list it.
fn field(name: &str) -> Element {
    Element {
        description: Some(name.to_owned()),
        value: Some("what someone typed".to_owned()),
        ..element("AXTextField", (150.0, 150.0, 200.0, 24.0))
    }
}

/// Focus in `focused`, owned by the app, inside the document window.
fn typing(focused: Element) -> Keyboard {
    Keyboard {
        window: Some(document_window()),
        focused: Some(ax::Focused {
            element: focused,
            pid: APP,
        }),
        within: Within::Window,
    }
}

/// The app in front, typing as `keyboard` says, with `listed` on screen.
fn inside_with(keyboard: Keyboard, listed: Vec<Window>) -> Seen {
    Seen::focused(Some(APP)).showing(listed).typing_in(keyboard)
}

/// What `expect` says of keys sent with `look`.
fn expecting(expect: &Expect, look: &mut Seen) -> Result<Option<&'static str>> {
    require(expect, Need::Keys, &display(), look, later())
}

fn in_document() -> Expect {
    expect().in_window(DOCUMENT).unwrap()
}

fn in_field(name: &str) -> Expect {
    expect().in_field(name).unwrap()
}

#[test]
fn keys_ask_for_nothing_inside_the_app_without_a_window_or_field() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    keys_with(&mut look).unwrap();
    assert_eq!(look.keyboard_reads, 0);
}

#[test]
fn a_focused_window_that_is_the_named_one_passes() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    assert!(expecting(&in_document(), &mut look).is_ok());
    assert_eq!(look.keyboard_reads, 1, "one read for the window and field");
}

#[test]
fn another_focused_window_of_the_app_is_refused_and_named() {
    let other = numbered(4121, APP, Some("Untitled"), (140.0, 140.0, 400.0, 300.0));
    let keyboard = Keyboard {
        window: Some(Element {
            title: Some("Untitled".to_owned()),
            ..element("AXWindow", (140.0, 140.0, 400.0, 300.0))
        }),
        ..typing(field("Email"))
    };
    let mut look = inside_with(keyboard, vec![other, document()]);
    let refused = refusal(expecting(&in_document(), &mut look));
    assert_eq!(refused["reason"], "window");
    assert_eq!(refused["found"]["window"], 4121);
    assert_eq!(refused["found"]["app"], "TextEdit");

    let mut none = inside_with(
        Keyboard {
            window: None,
            ..Keyboard::default()
        },
        vec![document()],
    );
    assert_eq!(
        refusal(expecting(&in_document(), &mut none))["reason"],
        "window"
    );
}

#[test]
fn a_focused_window_of_another_app_is_refused_as_the_app() {
    let calculator = numbered(339, OTHER, Some("Calculator"), (0.0, 0.0, 200.0, 300.0));
    let mut look = Seen::focused(Some(OTHER))
        .showing(vec![calculator, document()])
        .typing_in(typing(field("Email")));
    for expect in [in_document(), in_field("Email")] {
        assert_eq!(
            refusal(expecting(&expect, &mut look)),
            json!({"reason": "app", "found": {"app": "Calculator", "pid": OTHER}})
        );
    }
    assert_eq!(look.keyboard_reads, 0, "the app is judged first");
}

#[test]
fn a_sheet_over_the_window_is_refused_even_when_the_field_inside_matches() {
    let sheet = numbered(4122, APP, None, (150.0, 100.0, 300.0, 150.0));
    let named = in_document().in_field("Email").unwrap();

    // Focus in a field of a sheet the focused window holds.
    let mut held = inside_with(
        Keyboard {
            within: Within::Sheet,
            ..typing(field("Email"))
        },
        vec![sheet.clone(), document()],
    );
    let refused = refusal(expecting(&named, &mut held));
    assert_eq!(refused["reason"], "sheet");
    assert_eq!(refused["found"]["field"]["name"], "Email");

    // The sheet reported as the focused window itself.
    let mut focused = inside_with(
        Keyboard {
            window: Some(element("AXSheet", (150.0, 100.0, 300.0, 150.0))),
            ..typing(element("AXSheet", (150.0, 100.0, 300.0, 150.0)))
        },
        vec![sheet.clone(), document()],
    );
    let refused = refusal(expecting(&named, &mut focused));
    assert_eq!(refused["reason"], "sheet");
    assert_eq!(refused["found"]["window"], 4122);

    // A window of the app's in front of the document, as a sheet's own
    // window shows, even when the reads miss the sheet: named for what
    // the window list says it is, another window.
    let mut covered = inside_with(typing(field("Email")), vec![sheet, document()]);
    assert_eq!(
        refusal(expecting(&in_document(), &mut covered))["reason"],
        "window"
    );
}

#[test]
fn a_sheet_named_by_its_own_number_is_the_window_with_focus() {
    let sheet = numbered(4122, APP, None, (150.0, 100.0, 300.0, 150.0));
    let name = Element {
        identifier: Some("saveAsNameTextField".to_owned()),
        ..element("AXTextField", (200.0, 120.0, 200.0, 24.0))
    };
    let mut look = inside_with(
        Keyboard {
            window: Some(element("AXSheet", (150.0, 100.0, 300.0, 150.0))),
            ..typing(name)
        },
        vec![sheet, document()],
    );
    let expect = expect()
        .in_window(4122)
        .unwrap()
        .in_field("saveAsNameTextField")
        .unwrap();
    assert!(expecting(&expect, &mut look).is_ok());
}

#[test]
fn a_window_of_the_app_in_front_at_a_higher_layer_is_refused_as_another_window() {
    let panel = |bounds| Window {
        layer: 3,
        ..numbered(4130, APP, Some("Fonts"), bounds)
    };
    let menu = Window {
        layer: windows::MENU_LAYER,
        ..numbered(4131, APP, None, (120.0, 120.0, 150.0, 200.0))
    };
    for (listed, what) in [
        (
            vec![panel((150.0, 150.0, 200.0, 200.0)), document()],
            "a panel",
        ),
        (vec![menu, document()], "an open menu"),
    ] {
        let mut look = inside_with(typing(field("Email")), listed);
        let error = expecting(&in_document(), &mut look).unwrap_err();
        assert_eq!(error.details["reason"], "window", "{what}");
        assert!(
            error
                .error
                .starts_with("Another window of the app is in front"),
            "{what}: {}",
            error.error
        );
    }

    let aside = panel((600.0, 100.0, 200.0, 200.0));
    let mut look = inside_with(typing(field("Email")), vec![aside, document()]);
    assert!(
        expecting(&in_document(), &mut look).is_ok(),
        "a panel beside it"
    );
}

/// A floating panel and a document at the same place, with titles
/// that do not tell them apart: the panel's subrole says which one has
/// focus, and the document is never taken for the panel.
#[test]
fn a_panel_with_the_same_frame_is_told_apart_by_its_kind_or_refused() {
    let panel = Window {
        layer: 3,
        ..numbered(4130, APP, None, (100.0, 100.0, 400.0, 300.0))
    };
    let panel_window = |subrole: Option<&str>| Element {
        title: None,
        subrole: subrole.map(str::to_owned),
        ..document_window()
    };
    let listed = vec![panel.clone(), document()];

    assert_eq!(
        number(&listed, APP, &panel_window(Some("AXFloatingWindow"))),
        Listed::One(4130)
    );
    assert_eq!(
        number(&listed, APP, &document_window()),
        Listed::Several,
        "a standard window may be raised to the panel's layer: never the document by default"
    );
    assert_eq!(
        number(&listed, APP, &panel_window(None)),
        Listed::Several,
        "a window that says nothing of its kind"
    );

    let mut panel_in_front = inside_with(
        Keyboard {
            window: Some(panel_window(Some("AXFloatingWindow"))),
            ..typing(field("Email"))
        },
        listed.clone(),
    );
    let refused = refusal(expecting(&in_document(), &mut panel_in_front));
    assert_eq!(refused["reason"], "window");
    assert_eq!(refused["found"]["window"], 4130);

    let mut unsaid = inside_with(
        Keyboard {
            window: Some(panel_window(None)),
            ..typing(field("Email"))
        },
        listed,
    );
    let error = expecting(&in_document(), &mut unsaid).unwrap_err();
    assert_eq!(error.details["reason"], "unreadable");
    assert_eq!(error.details["unread"], "alike");
}

#[test]
fn a_focused_window_missing_from_the_list_is_unreadable() {
    let keyboard = Keyboard {
        window: Some(Element {
            title: Some("Untitled".to_owned()),
            ..element("AXWindow", (140.0, 140.0, 400.0, 300.0))
        }),
        ..typing(field("Email"))
    };
    let mut look = inside_with(keyboard, vec![document()]);
    let error = expecting(&in_document(), &mut look).unwrap_err();
    assert_eq!(error.details["reason"], "unreadable");
    assert_eq!(error.details["unread"], "unlisted");
}

#[test]
fn an_app_that_gives_itself_as_its_focused_window_withheld_its_windows() {
    let keyboard = Keyboard {
        window: Some(Element {
            frame: None,
            ..element("AXApplication", (0.0, 0.0, 0.0, 0.0))
        }),
        ..typing(field("Email"))
    };
    let mut look = inside_with(keyboard, vec![document()]);
    let error = expecting(&in_document(), &mut look).unwrap_err();
    assert_eq!(error.details["reason"], "unreadable");
    assert_eq!(error.details["unread"], "withheld");
}

#[test]
fn a_window_is_known_only_by_a_unique_frame_match_in_the_same_process() {
    let twin_elsewhere = numbered(
        5000,
        OTHER,
        Some("Letter.txt"),
        (100.0, 100.0, 400.0, 300.0),
    );
    let offset = numbered(4121, APP, Some("Letter.txt"), (103.0, 100.0, 400.0, 300.0));
    let window = document_window();
    assert_eq!(
        number(&[twin_elsewhere, offset.clone(), document()], APP, &window),
        Listed::One(DOCUMENT)
    );
    assert_eq!(
        number(&[offset], APP, &window),
        Listed::Missing,
        "off by three points"
    );

    let renamed = numbered(
        DOCUMENT,
        APP,
        Some("Other.txt"),
        (100.0, 100.0, 400.0, 300.0),
    );
    assert_eq!(
        number(&[renamed], APP, &window),
        Listed::Missing,
        "titles differ"
    );
    let browser = Element {
        title: Some("Letter.txt - Google Chrome".to_owned()),
        ..document_window()
    };
    assert_eq!(
        number(&[document()], APP, &browser),
        Listed::One(DOCUMENT),
        "the app's name added to the title"
    );
    let numbered_twin = numbered(
        4121,
        APP,
        Some("Letter.txt 2"),
        (100.0, 100.0, 400.0, 300.0),
    );
    assert_eq!(
        number(&[numbered_twin, document()], APP, &window),
        Listed::Several,
        "titles that both hold the window's leave it unproven"
    );
    let untitled = numbered(DOCUMENT, APP, None, (100.0, 100.0, 400.0, 300.0));
    assert_eq!(
        number(&[untitled], APP, &window),
        Listed::One(DOCUMENT),
        "a title the list does not show proves nothing either way"
    );
}

#[test]
fn two_windows_of_one_app_with_the_same_frame_and_title_leave_the_window_unproven() {
    let twin = numbered(4121, APP, Some("Letter.txt"), (100.0, 100.0, 400.0, 300.0));
    assert_eq!(
        number(&[twin.clone(), document()], APP, &document_window()),
        Listed::Several
    );

    let mut look = inside_with(typing(field("Email")), vec![twin, document()]);
    let error = expecting(&in_document(), &mut look).unwrap_err();
    assert_eq!(error.details["reason"], "unreadable");
    assert_eq!(error.details["unread"], "alike");
    assert!(error.details.get("found").is_none());
}

#[test]
fn a_field_reached_from_outside_the_window_leaves_the_window_unproven() {
    let keyboard = Keyboard {
        within: Within::Elsewhere,
        ..typing(field("Email"))
    };
    let mut look = inside_with(keyboard, vec![document()]);
    assert_eq!(
        refusal(expecting(&in_document(), &mut look))["reason"],
        "unreadable"
    );
}

#[test]
fn a_focused_field_that_elements_would_list_by_the_name_passes() {
    for (focused, name) in [
        (field("Email"), "email"),
        (
            Element {
                identifier: Some("First Text View".to_owned()),
                ..element("AXTextArea", (100.0, 132.0, 400.0, 268.0))
            },
            "First Text View",
        ),
        (
            Element {
                identifier: Some("_NS:9".to_owned()),
                focused: Some(true),
                ..element("AXTextField", (120.0, 110.0, 200.0, 22.0))
            },
            "_NS:9",
        ),
        (
            Element {
                title: Some("Search".to_owned()),
                ..element("AXSearchField", (120.0, 110.0, 200.0, 22.0))
            },
            "Search",
        ),
    ] {
        let filter = ElementFilter {
            role: None,
            name: Some(name.to_owned()),
        };
        assert!(filter.matches(&focused), "elements lists it as {name}");
        let mut look = inside_with(typing(focused), vec![document()]);
        assert!(expecting(&in_field(name), &mut look).is_ok(), "{name}");
    }
}

#[test]
fn a_find_field_identifier_requires_fresh_app_and_sibling_focus_proof() {
    let target = in_document().in_field("_NS:9").unwrap();
    let area = Element {
        identifier: Some("First Text View".to_owned()),
        focused: Some(true),
        ..element("AXTextArea", (100.0, 132.0, 400.0, 268.0))
    };
    let mut background = inside_with(typing(area), vec![document()]);
    background.focused = Some(Ok(Some(OTHER)));
    assert_eq!(
        refusal(expecting(&target, &mut background))["reason"],
        "app"
    );
    background.focused = Some(Ok(Some(APP)));
    assert_eq!(
        refusal(expecting(&target, &mut background))["reason"],
        "field"
    );
    background.keyboard = Some(typing(Element {
        identifier: Some("_NS:9".to_owned()),
        focused: Some(true),
        ..element("AXTextField", (120.0, 110.0, 200.0, 22.0))
    }));
    assert_eq!(
        expecting(&target, &mut background).unwrap(),
        Some("AXTextField")
    );
    assert_eq!(background.focus_reads, 3);
    assert_eq!(background.keyboard_reads, 2);
}

#[test]
fn a_field_matched_only_by_the_first_part_of_its_label_passes_as_elements_would() {
    let mut look = inside_with(typing(field("Email, required")), vec![document()]);
    assert!(expecting(&in_field("Email"), &mut look).is_ok());
}

#[test]
fn a_field_is_never_named_by_what_it_holds() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    assert_eq!(
        refusal(expecting(&in_field("what someone typed"), &mut look))["reason"],
        "field"
    );
}

#[test]
fn a_focused_element_with_another_name_is_refused_as_another_field() {
    let mut look = inside_with(typing(field("Password")), vec![document()]);
    let refused = refusal(expecting(&in_field("Email"), &mut look));
    assert_eq!(
        refused,
        json!({"reason": "field", "found": {"app": "TextEdit", "pid": APP,
            "window": DOCUMENT, "field": {"role": "AXTextField", "name": "Password"}}})
    );

    let mut nothing = inside_with(
        Keyboard {
            focused: None,
            ..typing(field("Email"))
        },
        vec![document()],
    );
    assert_eq!(
        refusal(expecting(&in_field("Email"), &mut nothing))["reason"],
        "field"
    );
}

/// A custom view without accessibility leaves its window reported as
/// focused, and so does an app while focus moves between fields; a
/// browser may report its page; a custom control may say it is unknown.
/// Only a control that takes text itself is a field, whatever the name.
#[test]
fn only_a_control_that_takes_text_is_the_field() {
    for role in [
        Some("AXWindow"),
        Some("AXSheet"),
        Some("AXWebArea"),
        Some("AXGroup"),
        Some("AXUnknown"),
        Some("AXTabGroup"),
        Some("AXPopover"),
        Some("AXToolbar"),
        Some("AXButton"),
        None,
    ] {
        let named = Element {
            title: Some("Email".to_owned()),
            role: role.map(str::to_owned),
            ..field("Email")
        };
        let mut look = inside_with(typing(named), vec![document()]);
        assert_eq!(
            refusal(expecting(&in_field("Email"), &mut look))["reason"],
            "field",
            "{role:?}"
        );
    }

    for role in FIELDS {
        let named = Element {
            role: Some(role.to_owned()),
            ..field("Email")
        };
        let mut look = inside_with(typing(named), vec![document()]);
        assert_eq!(
            expecting(&in_field("Email"), &mut look).unwrap(),
            Some(role),
            "the check says the role of the field it passed"
        );
    }
}

#[test]
fn only_a_check_that_names_a_field_says_its_role() {
    let look = || inside_with(typing(field("Email")), vec![document()]);
    let both = in_document().in_field("Email").unwrap();

    assert_eq!(expecting(&both, &mut look()).unwrap(), Some("AXTextField"));
    assert_eq!(expecting(&in_document(), &mut look()).unwrap(), None);
    assert_eq!(keys_with(&mut look()).unwrap(), None);
    assert_eq!(
        require(
            &in_field("Email"),
            Need::Point(200.0, 200.0),
            &display(),
            &mut look(),
            later()
        )
        .unwrap(),
        None,
        "a pointer action types into no field"
    );
}

#[test]
fn an_element_that_says_it_is_not_focused_is_not_the_field() {
    let unfocused = Element {
        focused: Some(false),
        ..field("Email")
    };
    let mut look = inside_with(typing(unfocused), vec![document()]);
    assert_eq!(
        refusal(expecting(&in_field("Email"), &mut look))["reason"],
        "field"
    );

    let unsaid = Element {
        focused: None,
        enabled: None,
        ..field("Email")
    };
    let mut look = inside_with(typing(unsaid), vec![document()]);
    assert!(
        expecting(&in_field("Email"), &mut look).is_ok(),
        "an app that does not say either"
    );
}

/// A container's name is a title, a document's or a page's: a refusal
/// names its role alone.
#[test]
fn a_refusal_names_a_container_by_its_role_alone() {
    let page = Element {
        title: Some("Bank statement".to_owned()),
        ..element("AXWebArea", (100.0, 100.0, 400.0, 300.0))
    };
    let mut look = inside_with(typing(page), vec![document()]);
    let refused = refusal(expecting(&in_field("Email"), &mut look));
    assert_eq!(refused["found"]["field"], json!({"role": "AXWebArea"}));
}

#[test]
fn a_disabled_focused_field_is_refused() {
    let disabled = Element {
        enabled: Some(false),
        ..field("Email")
    };
    let mut look = inside_with(typing(disabled), vec![document()]);
    let error = expecting(&in_field("Email"), &mut look).unwrap_err();
    assert_eq!(error.details["reason"], "disabled");
    assert!(
        error.error.starts_with("That field is disabled"),
        "{}",
        error.error
    );
}

/// A read that could not tell where focus is, for whatever reason,
/// refuses and says why.
#[test]
fn a_read_that_could_not_tell_refuses_and_says_why() {
    for why in [Unread::Cut, Unread::Deep, Unread::Failed, Unread::Malformed] {
        let mut look = inside_with(typing(field("Email")), vec![document()]);
        look.unread = Some(why);
        for expect in [in_document(), in_field("Email")] {
            let error = expecting(&expect, &mut look).unwrap_err();
            assert_eq!(error.details["reason"], "unreadable", "{why:?}");
            assert_eq!(error.details["unread"], json!(why));
            assert!(error.error.ends_with(NOT_SENT), "{}", error.error);
            assert!(!error.uncertain);
        }
    }
}

#[test]
fn a_focused_element_owned_by_another_process_is_refused() {
    let mut keyboard = typing(field("Email"));
    if let Some(focused) = keyboard.focused.as_mut() {
        focused.pid = OTHER;
    }
    let mut look = inside_with(keyboard, vec![document()]);
    assert_eq!(
        refusal(expecting(&in_field("Email"), &mut look))["reason"],
        "field"
    );
}

/// A read of where focus is that answered after the check's deadline
/// proves nothing, however good its answer.
#[test]
fn a_check_that_ran_out_of_time_is_refused_and_says_so() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    look.slow = Duration::from_millis(20);
    for expect in [in_document(), in_field("Email")] {
        let deadline = Instant::now() + Duration::from_millis(5);
        let error = require(&expect, Need::Keys, &display(), &mut look, deadline).unwrap_err();
        assert_eq!(error.details["reason"], "unreadable");
        assert_eq!(error.details["unread"], "slow");
        assert!(error.error.contains("could not tell"), "{}", error.error);
        assert!(!error.uncertain);
    }

    let mut spent = Live::until(Instant::now());
    assert_eq!(
        spent
            .keyboard(
                APP,
                Judged {
                    window: true,
                    field: true
                }
            )
            .unwrap_err()
            .details["unread"],
        "slow",
        "nothing is asked once the time is spent"
    );
}

#[test]
fn an_unreadable_verification_that_exceeds_the_deadline_retains_the_slow_marker() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    look.slow = Duration::from_millis(20);
    look.unread = Some(Unread::Failed);
    let deadline = Instant::now() + Duration::from_millis(5);
    let error = require(
        &in_field("Email"),
        Need::Keys,
        &display(),
        &mut look,
        deadline,
    )
    .unwrap_err();
    assert_eq!(error.details["reason"], "unreadable");
    assert_eq!(error.details["unread"], "slow");
    assert!(!error.uncertain);

    let mut spent = Live::until(Instant::now());
    assert_eq!(spent.focused().unwrap_err().details["unread"], "slow");
    assert_eq!(spent.windows().unwrap_err().details["unread"], "slow");
}

#[test]
fn the_refusal_names_the_focused_field_without_its_value() {
    let secret = Element {
        value: Some("hunter2".to_owned()),
        ..field("Password")
    };
    let mut look = inside_with(typing(secret), vec![document()]);
    let error = expecting(&in_field("Email"), &mut look).unwrap_err();
    let said = serde_json::to_string(&error).unwrap();
    assert!(!said.contains("hunter2"), "{said}");
    assert!(!said.contains("Letter.txt"), "{said}");
    assert!(said.contains("Password"), "{said}");
}

#[test]
fn a_point_outside_the_named_window_is_refused_before_a_click() {
    let other = numbered(4121, APP, Some("Untitled"), (450.0, 100.0, 300.0, 300.0));
    let calculator = numbered(339, OTHER, Some("Calculator"), (100.0, 500.0, 200.0, 200.0));
    let listed = vec![other, calculator, document()];
    let at = |x, y| {
        require(
            &in_document(),
            Need::Point(x, y),
            &display(),
            &mut Seen::focused(Some(APP)).showing(listed.clone()),
            later(),
        )
    };

    assert!(at(200.0, 200.0).is_ok(), "inside it");
    assert_eq!(
        refusal(at(470.0, 200.0)),
        json!({"reason": "window", "found": {"app": "TextEdit", "pid": APP, "window": 4121}}),
        "the app's other window on top there"
    );
    assert_eq!(
        refusal(at(150.0, 550.0))["reason"],
        "point",
        "another app's window there"
    );
    assert_eq!(
        refusal(at(900.0, 700.0))["reason"],
        "window",
        "nothing there"
    );

    let gone = expect().in_window(9999).unwrap();
    let mut look = Seen::focused(Some(APP)).showing(listed.clone());
    assert_eq!(
        refusal(require(
            &gone,
            Need::Point(200.0, 200.0),
            &display(),
            &mut look,
            later()
        ))["reason"],
        "window",
        "a window that is not on screen"
    );
    let elsewhere = Expect::new(OTHER).unwrap().in_window(DOCUMENT).unwrap();
    let mut look = Seen::focused(Some(APP)).showing(listed);
    assert_eq!(
        refusal(require(
            &elsewhere,
            Need::Point(200.0, 200.0),
            &display(),
            &mut look,
            later()
        ))["reason"],
        "window",
        "a window of another app"
    );
}

#[test]
fn a_drag_is_checked_at_its_press_point_only() {
    let other = numbered(4121, APP, Some("Untitled"), (450.0, 100.0, 300.0, 300.0));
    let listed = vec![other, document()];
    let drag = |from, to| {
        require(
            &in_document(),
            Need::Drag { from, to },
            &display(),
            &mut Seen::focused(Some(APP)).showing(listed.clone()),
            later(),
        )
    };

    assert!(
        drag((200.0, 200.0), (600.0, 200.0)).is_ok(),
        "a drop on the app's other window"
    );
    assert_eq!(
        refusal(drag((600.0, 200.0), (200.0, 200.0)))["reason"],
        "window"
    );
    assert_eq!(
        refusal(drag((200.0, 200.0), (900.0, 700.0)))["reason"],
        "point",
        "a drop where no window of the app is"
    );
}

#[test]
fn read_only_verification_leaves_wrong_and_absent_focus_in_place() {
    for focused in [None, Some(OTHER)] {
        let mut look = Seen::focused(focused);
        assert_eq!(refusal(keys_with(&mut look))["reason"], "app");
        assert_eq!(look.focused.as_ref().unwrap().as_ref().unwrap(), &focused);
        assert_eq!(look.keyboard_reads, 0);
        assert_eq!(refusal(keys_with(&mut look))["reason"], "app");
    }
}

#[test]
fn absent_system_focus_accepts_direct_app_proof_and_still_checks_the_window_and_field() {
    let mut look = inside_with(typing(field("Email")), vec![document()]);
    look.focused = Some(Ok(None));
    look.app_focused = true;
    assert!(expecting(&in_document().in_field("Email").unwrap(), &mut look).is_ok());
    assert_eq!(
        refusal(expecting(
            &in_document().in_field("Other").unwrap(),
            &mut look
        ))["reason"],
        "field"
    );
    assert_eq!(
        refusal(expecting(&expect().in_window(999).unwrap(), &mut look))["reason"],
        "window"
    );
    look.app_focused = false;
    assert_eq!(refusal(expecting(&expect(), &mut look))["reason"], "app");
    look.app_focused = true;
    look.focused = Some(Ok(Some(OTHER)));
    assert_eq!(refusal(expecting(&expect(), &mut look))["reason"], "app");
    look.focused = Some(Err(Error::new(Code::Native, "unreadable")));
    assert_eq!(
        refusal(expecting(&expect(), &mut look))["reason"],
        "unreadable"
    );
}
