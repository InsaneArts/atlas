use super::super::keyboard::Answer;
use super::*;
use crate::native::windows::Bounds;
use std::{cell::Cell, collections::HashMap, rc::Rc};

const APP: usize = 0;
const WINDOW: usize = 1;
const FIELD: usize = 2;

#[derive(Clone)]
struct Fake {
    now: Rc<Cell<Instant>>,
    answers: HashMap<(usize, &'static str), Answer<usize>>,
    lists: HashMap<(usize, &'static str), Vec<usize>>,
    windows: Vec<Window>,
    writes: Vec<(usize, Write)>,
    supported: bool,
    status: AXError,
    present: bool,
    malformed: bool,
    delay: Duration,
    bounds: Vec<Duration>,
    owners: HashMap<usize, i64>,
    after_write: Option<fn(&mut Fake)>,
    on_ask: Option<fn(&mut Fake, usize, &'static str)>,
    observed_window: Rc<Cell<usize>>,
    focused_element: Rc<Cell<usize>>,
    frontmost: Rc<Cell<bool>>,
}

impl Fake {
    fn new() -> Self {
        let mut fake = Self {
            now: Rc::new(Cell::new(Instant::now())),
            answers: HashMap::new(),
            lists: HashMap::new(),
            windows: vec![Window {
                number: 42,
                pid: 812,
                owner: Some("Editor".into()),
                title: Some("private-title-sentinel".into()),
                layer: 0,
                alpha: 1.0,
                bounds: Bounds {
                    x: 10.0,
                    y: 30.0,
                    width: 400.0,
                    height: 300.0,
                },
            }],
            writes: Vec::new(),
            supported: true,
            status: AXError::Success,
            present: true,
            malformed: false,
            delay: Duration::ZERO,
            bounds: Vec::new(),
            owners: HashMap::new(),
            after_write: None,
            on_ask: None,
            observed_window: Rc::new(Cell::new(WINDOW)),
            focused_element: Rc::new(Cell::new(FIELD)),
            frontmost: Rc::new(Cell::new(false)),
        };
        fake.lists.insert((APP, "AXWindows"), vec![WINDOW]);
        fake.lists.insert((WINDOW, "AXChildren"), vec![FIELD]);
        fake.answers
            .insert((FIELD, "AXParent"), Answer::Element(WINDOW));
        fake.answers
            .insert((APP, "AXFocusedWindow"), Answer::Element(WINDOW));
        fake.window(WINDOW);
        fake.answers
            .insert((FIELD, "AXRole"), Answer::Text("AXTextField".into()));
        fake.answers
            .insert((FIELD, "AXDescription"), Answer::Text("Email".into()));
        fake.answers
            .insert((FIELD, "AXEnabled"), Answer::Flag(true));
        fake
    }
    fn window(&mut self, element: usize) {
        for (name, value) in [
            ("AXRole", Answer::Text("AXWindow".into())),
            ("AXSubrole", Answer::Text("AXStandardWindow".into())),
            ("AXTitle", Answer::Text("private-title-sentinel".into())),
            ("AXPosition", Answer::Point(10.0, 30.0)),
            ("AXSize", Answer::Size(400.0, 300.0)),
        ] {
            self.answers.insert((element, name), value);
        }
    }
    fn sheet(&mut self, sheet: usize, parent: usize) {
        self.window(sheet);
        self.answers
            .insert((sheet, "AXRole"), Answer::Text("AXSheet".into()));
        self.answers
            .insert((sheet, "AXTitle"), Answer::Text("Page Setup".into()));
        self.answers
            .insert((sheet, "AXPosition"), Answer::Point(30.0, 50.0));
        self.answers
            .insert((sheet, "AXSize"), Answer::Size(500.0, 180.0));
        self.answers
            .insert((sheet, "AXParent"), Answer::Element(parent));
        self.answers.insert((sheet, "AXModal"), Answer::Flag(true));
        self.lists.insert((parent, "AXSheets"), vec![sheet]);
    }
    fn run(&mut self, expect: &Expect) -> Result<()> {
        let deadline = self.now.get() + Duration::from_millis(450);
        prepare(self, expect, Need::Keys, deadline, &|| Ok(()), &mut |_| {
            false
        })
        .map(|_| ())
    }
}

impl Ask for &mut Fake {
    type Element = usize;
    fn now(&self) -> Instant {
        self.now.get()
    }
    fn bound(&mut self, _: &usize, timeout: Duration) -> bool {
        self.bounds.push(timeout);
        true
    }
    fn ask(&mut self, element: &usize, name: &'static str) -> Answer<usize> {
        assert_ne!(name, "AXValue");
        if let Some(on_ask) = self.on_ask {
            on_ask(self, *element, name);
        }
        self.now.set(self.now.get() + self.delay);
        self.answers
            .get(&(*element, name))
            .cloned()
            .unwrap_or(Answer::Absent)
    }
    fn owner(&mut self, element: &usize) -> Option<i64> {
        Some(*self.owners.get(element).unwrap_or(&812))
    }
}
impl Backend for &mut Fake {
    fn app(&mut self, _: i64) -> Option<usize> {
        self.present.then_some(APP)
    }
    fn windows(&mut self) -> Result<Vec<Window>> {
        Ok(self.windows.clone())
    }
    fn list(
        &mut self,
        element: &usize,
        name: &'static str,
        _: usize,
        _: Instant,
    ) -> Result<Vec<usize>> {
        if self.malformed {
            return Err(Unread::Malformed.error());
        }
        Ok(self
            .lists
            .get(&(*element, name))
            .cloned()
            .unwrap_or_default())
    }
    fn supports(&mut self, _: &usize, _: Write, _: Instant) -> Result<bool> {
        Ok(self.supported)
    }
    fn write(&mut self, element: &usize, operation: Write) -> AXError {
        self.writes.push((*element, operation));
        if let Some(after) = self.after_write {
            after(self);
        }
        self.status
    }
}

fn named() -> Expect {
    Expect::new(812).unwrap().in_window(42).unwrap()
}

#[test]
fn an_oversized_action_array_is_rejected_before_decoding() {
    let raise = CFString::from_static_str("AXRaise").into();
    let names = CFArray::from_retained_objects(&vec![raise; ACTIONS + 1]);
    let error = supports_raise(&names, Instant::now() + REQUEST, || {
        panic!("an oversized action array must not be scanned")
    })
    .unwrap_err();
    assert_eq!(error.details["unread"], "malformed");
}

#[test]
fn a_malformed_action_after_raise_is_rejected() {
    let names = CFArray::from_retained_objects(&[
        CFString::from_static_str("AXRaise").into(),
        objc2_core_foundation::CFBoolean::new(true).retain().into(),
    ]);
    let now = Instant::now();
    let error = supports_raise(&names, now + REQUEST, || now).unwrap_err();
    assert_eq!(error.details["unread"], "malformed");
}

#[test]
fn action_decoding_accepts_only_complete_bounded_string_arrays() {
    let now = Instant::now();
    let other = CFString::from_static_str("AXPress").into();
    let mut values = vec![other; ACTIONS];
    let names = CFArray::from_retained_objects(&values);
    assert!(!supports_raise(&names, now + REQUEST, || now).unwrap());
    values[ACTIONS - 1] = CFString::from_static_str("AXRaise").into();
    let names = CFArray::from_retained_objects(&values);
    assert!(supports_raise(&names, now + REQUEST, || now).unwrap());
    let names = CFArray::from_retained_objects(&[]);
    assert!(!supports_raise(&names, now + REQUEST, || now).unwrap());
    assert_eq!(
        supports_raise(&names, now, || now).unwrap_err().details["unread"],
        "slow"
    );
}

#[test]
fn action_decoding_checks_the_deadline_during_and_after_the_scan() {
    let names = CFArray::from_retained_objects(&[
        CFString::from_static_str("AXRaise").into(),
        CFString::from_static_str("AXPress").into(),
    ]);
    let now = Instant::now();
    let deadline = now + REQUEST;
    for expires_at in 1..=3 {
        let mut checks = 0;
        let error = supports_raise(&names, deadline, || {
            checks += 1;
            if checks >= expires_at { deadline } else { now }
        })
        .unwrap_err();
        assert_eq!(error.details["unread"], "slow");
        assert_eq!(checks, expires_at);
    }
}
fn reason(result: Result<()>, wanted: &str) {
    let error = result.err().unwrap();
    assert_eq!(error.details["reason"], wanted);
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("private-title-sentinel")
    );
}

#[test]
fn a_unique_window_is_raised_and_given_key_focus_once() {
    let mut fake = Fake::new();
    fake.run(&named()).unwrap();
    assert_eq!(
        fake.writes,
        [
            (APP, Write::Flag("AXFrontmost")),
            (WINDOW, Write::Raise),
            (WINDOW, Write::Flag("AXMain")),
            (WINDOW, Write::Flag("AXFocused"))
        ]
    );
    assert!(fake.bounds.iter().all(|timeout| *timeout <= REQUEST));
}

#[test]
fn windows_swapping_positions_during_resolution_refuse_before_any_write() {
    let mut fake = Fake::new();
    fake.window(3);
    fake.answers
        .insert((3, "AXPosition"), Answer::Point(600.0, 30.0));
    let mut other = fake.windows[0].clone();
    other.number = 43;
    other.bounds.x = 600.0;
    fake.windows.push(other);
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
    fake.on_ask = Some(|fake, element, name| {
        if element == WINDOW && name == "AXPosition" {
            fake.answers
                .insert((WINDOW, "AXPosition"), Answer::Point(600.0, 30.0));
            fake.answers
                .insert((3, "AXPosition"), Answer::Point(10.0, 30.0));
            fake.windows[0].bounds.x = 600.0;
            fake.windows[1].bounds.x = 10.0;
            fake.on_ask = None;
        }
    });
    assert_eq!(
        fake.run(&named()).unwrap_err().details["unread"],
        "unlisted"
    );
    assert!(fake.writes.is_empty());
}

#[test]
fn unrelated_lookalike_windows_do_not_block_a_unique_target() {
    for ready in [false, true] {
        let mut fake = Fake::new();
        for (element, number) in [(3, 43), (4, 44)] {
            fake.window(element);
            fake.answers
                .insert((element, "AXPosition"), Answer::Point(600.0, 30.0));
            let mut other = fake.windows[0].clone();
            other.number = number;
            other.bounds.x = 600.0;
            fake.windows.push(other);
        }
        fake.lists.insert((APP, "AXWindows"), vec![3, 4, WINDOW]);
        let deadline = fake.now.get() + Duration::from_millis(450);
        prepare(
            &mut fake,
            &named(),
            Need::Keys,
            deadline,
            &|| Ok(()),
            &mut |_| ready,
        )
        .unwrap();
        assert_eq!(fake.writes.len(), if ready { 0 } else { 4 });
    }
}

#[test]
fn a_main_window_missing_from_other_references_is_admitted_for_app_input() {
    for sheet in [false, true] {
        let mut fake = Fake::new();
        fake.lists.insert((APP, "AXWindows"), vec![]);
        fake.answers.remove(&(APP, "AXFocusedWindow"));
        fake.answers
            .insert((APP, "AXMainWindow"), Answer::Element(WINDOW));
        if sheet {
            fake.sheet(3, WINDOW);
        } else {
            fake.answers.insert((WINDOW, "AXModal"), Answer::Flag(true));
        }
        reason(fake.run(&Expect::new(812).unwrap()), "sheet");
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn identical_window_references_are_never_chosen_by_order() {
    let mut fake = Fake::new();
    fake.window(3);
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
    assert_eq!(fake.run(&named()).unwrap_err().details["unread"], "alike");
    assert!(fake.writes.is_empty());
}

#[test]
fn missing_apps_windows_and_wrong_owners_make_no_writes() {
    let mut fake = Fake::new();
    fake.present = false;
    reason(fake.run(&named()), "app");
    fake.present = true;
    fake.windows.clear();
    reason(fake.run(&named()), "window");
    fake = Fake::new();
    fake.owners.insert(WINDOW, 999);
    assert!(fake.run(&named()).is_err());
    assert!(fake.writes.is_empty());
}

#[test]
fn a_sheet_blocks_named_and_app_only_input_before_any_write() {
    for expect in [named(), Expect::new(812).unwrap()] {
        let mut fake = Fake::new();
        fake.sheet(3, WINDOW);
        reason(fake.run(&expect), "sheet");
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_separate_modal_window_blocks_a_document_but_an_ordinary_window_does_not() {
    let mut fake = Fake::new();
    fake.answers
        .insert((APP, "AXFocusedWindow"), Answer::Element(3));
    fake.answers.insert((3, "AXModal"), Answer::Flag(true));
    reason(fake.run(&named()), "sheet");
    assert!(fake.writes.is_empty());
    fake.answers.insert((3, "AXModal"), Answer::Flag(false));
    fake.run(&named()).unwrap();
    assert!(!fake.writes.is_empty());
}

#[test]
fn an_unfocused_separately_listed_modal_blocks_a_selected_document_before_any_write() {
    for focused in [Some(WINDOW), None] {
        let mut fake = Fake::new();
        fake.window(3);
        fake.answers
            .insert((3, "AXTitle"), Answer::Text("Dialog".into()));
        fake.answers.insert((3, "AXModal"), Answer::Flag(true));
        fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
        if let Some(focused) = focused {
            fake.answers
                .insert((APP, "AXFocusedWindow"), Answer::Element(focused));
        } else {
            fake.answers.remove(&(APP, "AXFocusedWindow"));
        }
        reason(fake.run(&named()), "sheet");
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn an_unreadable_separately_listed_dialog_refuses_before_any_write() {
    for modal in [Answer::Absent, Answer::Failed(AXError::CannotComplete)] {
        let mut fake = Fake::new();
        fake.window(3);
        fake.answers
            .insert((3, "AXTitle"), Answer::Text("Dialog".into()));
        fake.answers
            .insert((3, "AXSubrole"), Answer::Text("AXDialog".into()));
        fake.answers.insert((3, "AXModal"), modal);
        fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
        assert!(fake.run(&named()).is_err());
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn an_explicit_sheet_target_can_receive_its_own_focus() {
    let mut fake = Fake::new();
    fake.answers
        .insert((WINDOW, "AXRole"), Answer::Text("AXSheet".into()));
    fake.run(&named()).unwrap();
}

#[test]
fn an_explicit_sheet_target_is_admitted_when_its_parent_also_lists_it() {
    let mut fake = Fake::new();
    fake.window(3);
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Document".into()));
    fake.lists.insert((APP, "AXWindows"), vec![3, WINDOW]);
    fake.lists.insert((3, "AXSheets"), vec![WINDOW]);
    fake.answers
        .insert((WINDOW, "AXRole"), Answer::Text("AXSheet".into()));
    fake.run(&named()).unwrap();
    assert!(fake.writes.contains(&(WINDOW, Write::Raise)));
}

#[test]
fn an_already_ready_target_is_admitted_without_writes() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(450);
    prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| true,
    )
    .unwrap();
    assert!(fake.writes.is_empty());
}

#[test]
fn a_unique_enabled_field_is_focused_after_its_window() {
    let mut fake = Fake::new();
    fake.run(&named().in_field("Email").unwrap()).unwrap();
    assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
}

fn find_bar() -> Fake {
    let mut fake = Fake::new();
    fake.lists.insert((WINDOW, "AXChildren"), vec![4, 5]);
    for (group, child) in [(4, FIELD), (5, 3)] {
        fake.answers
            .insert((group, "AXRole"), Answer::Text("AXGroup".into()));
        fake.answers
            .insert((group, "AXParent"), Answer::Element(WINDOW));
        fake.lists.insert((group, "AXChildren"), vec![child]);
        fake.answers
            .insert((child, "AXParent"), Answer::Element(group));
    }
    fake.answers
        .insert((3, "AXRole"), Answer::Text("AXTextArea".into()));
    fake.answers
        .insert((3, "AXIdentifier"), Answer::Text("First Text View".into()));
    for (element, attribute) in [
        (3, "AXDescription"),
        (FIELD, "AXTitle"),
        (FIELD, "AXDescription"),
    ] {
        fake.answers
            .insert((element, attribute), Answer::Failed(AXError::Failure));
    }
    fake.answers
        .insert((FIELD, "AXIdentifier"), Answer::Text("_NS:9".into()));
    fake.focused_element.set(3);
    fake.on_ask = Some(|fake, element, attribute| {
        let answer = match (element, attribute) {
            (APP, "AXFocusedUIElement") => Answer::Element(fake.focused_element.get()),
            (APP, "AXFrontmost") => Answer::Flag(fake.frontmost.get()),
            (_, "AXFocused") => Answer::Flag(element == fake.focused_element.get()),
            _ => return,
        };
        fake.answers.insert((element, attribute), answer);
    });
    fake.after_write = Some(|fake| match fake.writes.last().copied().unwrap() {
        (APP, Write::Flag("AXFrontmost")) => fake.frontmost.set(true),
        (FIELD, Write::Flag("AXFocused")) => fake.focused_element.set(FIELD),
        _ => {}
    });
    fake
}

fn read_find_bar(fake: &mut Fake, deadline: Instant) -> super::super::Keyboard {
    super::super::keyboard::read(
        fake,
        &APP,
        deadline,
        super::super::Judged {
            window: true,
            field: true,
        },
    )
    .unwrap()
}

#[test]
fn an_identifier_only_find_field_is_established_from_a_background_sibling_text_area() {
    let mut fake = find_bar();
    let expect = named().in_field("_NS:9").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let mut observation = fake.clone();
    let initial = read_find_bar(&mut observation, deadline);
    assert_eq!(
        initial.focused.unwrap().element.role.as_deref(),
        Some("AXTextArea")
    );
    assert!(!fake.frontmost.get());
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| observation.frontmost.get(),
    )
    .unwrap();
    let mut reads = 0;
    let role = settle(target, &expect, deadline, &|| Ok(()), |number, until| {
        reads += 1;
        assert!(reads <= 2);
        assert!(observation.frontmost.get());
        assert_eq!(number, Some(42));
        let keyboard = read_find_bar(&mut observation, until);
        assert_eq!(keyboard.within, Within::Window);
        let focused = keyboard.focused.unwrap().element;
        assert_eq!(focused.focused, Some(true));
        assert!(
            ElementFilter {
                name: Some("_NS:9".into()),
                ..Default::default()
            }
            .matches(&focused)
        );
        assert_eq!(focused.role.as_deref(), Some("AXTextField"));
        Ok(Some("AXTextField"))
    })
    .unwrap();
    assert_eq!(role, Some("AXTextField"));
    assert_eq!(reads, 2);
    assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
}

#[test]
fn a_missing_find_field_name_refuses_before_activating_the_background_app() {
    let mut fake = find_bar();
    let error = fake.run(&named().in_field("_NS:10").unwrap()).unwrap_err();
    assert_eq!(error.details["reason"], "field");
    assert!(
        error
            .error
            .starts_with("No field in the target window has that name")
    );
    assert!(fake.writes.is_empty());
    assert!(!fake.frontmost.get());
    assert_eq!(fake.focused_element.get(), 3);
}

#[test]
fn duplicate_find_field_identifiers_refuse_even_when_their_name_getters_fail() {
    let mut fake = find_bar();
    fake.answers
        .insert((3, "AXIdentifier"), Answer::Text("_NS:9".into()));
    reason(fake.run(&named().in_field("_NS:9").unwrap()), "field");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_failed_exact_title_cannot_promote_a_readable_leading_match() {
    let mut fake = find_bar();
    // The field's title would be Email, but that getter cannot be read.
    fake.answers.insert(
        (FIELD, "AXIdentifier"),
        Answer::Text("personal-email".into()),
    );
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Email, Work".into()));
    let error = fake.run(&named().in_field("Email").unwrap()).unwrap_err();
    assert_eq!(error.details["reason"], "field");
    assert!(error.error.contains("told apart"));
    assert!(fake.writes.is_empty());
    assert!(!fake.frontmost.get());
}

#[test]
fn a_failed_title_cannot_hide_an_exact_duplicate_behind_a_readable_identifier() {
    for attribute in ["AXTitle", "AXDescription"] {
        let mut fake = Fake::new();
        fake.lists.insert((WINDOW, "AXChildren"), vec![FIELD, 3]);
        fake.answers
            .insert((FIELD, "AXDescription"), Answer::Absent);
        fake.answers.insert(
            (FIELD, "AXIdentifier"),
            Answer::Text("personal-email".into()),
        );
        fake.answers
            .insert((FIELD, attribute), Answer::Failed(AXError::Failure));
        fake.answers
            .insert((3, "AXRole"), Answer::Text("AXTextField".into()));
        fake.answers
            .insert((3, "AXParent"), Answer::Element(WINDOW));
        fake.answers
            .insert((3, "AXTitle"), Answer::Text("Email".into()));
        let error = fake.run(&named().in_field("Email").unwrap()).unwrap_err();
        assert_eq!(error.details["reason"], "field");
        assert!(error.error.contains("told apart"));
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_plain_failed_identifier_blocks_an_identifier_winner_despite_a_readable_title() {
    let mut fake = find_bar();
    fake.answers
        .insert((3, "AXIdentifier"), Answer::Failed(AXError::Failure));
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Document".into()));
    let error = fake.run(&named().in_field("_NS:9").unwrap()).unwrap_err();
    assert_eq!(error.details["unread"], "failed");
    assert!(fake.writes.is_empty());
    assert!(!fake.frontmost.get());
}

#[test]
fn a_plain_failed_identifier_prevents_selecting_a_readable_title_or_description() {
    for attribute in ["AXTitle", "AXDescription"] {
        for name in ["Email, Work", "Email"] {
            let mut fake = find_bar();
            for element in [FIELD, 3] {
                fake.answers
                    .insert((element, "AXTitle"), Answer::Text("Document".into()));
                fake.answers
                    .insert((element, "AXDescription"), Answer::Text("Editor".into()));
            }
            fake.answers
                .insert((FIELD, attribute), Answer::Text(name.into()));
            fake.answers
                .insert((3, "AXIdentifier"), Answer::Failed(AXError::Failure));
            let error = fake.run(&named().in_field("Email").unwrap()).unwrap_err();
            assert_eq!(error.details["reason"], "field");
            assert!(error.error.contains("told apart"));
            assert!(fake.writes.is_empty());
            assert!(!fake.frontmost.get());
            assert_eq!(fake.focused_element.get(), 3);
        }
    }
}

#[test]
fn an_identifier_winner_uses_its_own_namespace_and_accepts_readable_absence() {
    for title in ["Document", "_NS:9"] {
        let mut fake = find_bar();
        fake.answers.insert((3, "AXIdentifier"), Answer::Absent);
        fake.answers
            .insert((3, "AXTitle"), Answer::Text(title.into()));
        fake.run(&named().in_field("_NS:9").unwrap()).unwrap();
        assert_eq!(fake.focused_element.get(), FIELD);
        assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
    }
}

#[test]
fn an_exactly_focused_field_skips_search_and_retains_its_identity_without_writes() {
    let mut fake = find_bar();
    fake.focused_element.set(FIELD);
    fake.frontmost.set(true);
    // Searching the sibling's names would fail; readiness needs no search.
    fake.answers
        .insert((3, "AXIdentifier"), Answer::Failed(AXError::CannotComplete));
    let expect = named().in_field("_NS:9").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let mut observation = fake.clone();
    let listed = fake.windows.clone();
    let mut reads = 0;
    let mut verify = |number, until| {
        reads += 1;
        assert!(reads <= 3);
        assert_eq!(number, Some(42));
        let keyboard = read_find_bar(&mut observation, until);
        crate::native::focus::tests::verify_keyboard(
            &expect,
            keyboard,
            listed.clone(),
            until,
            Duration::ZERO,
        )
    };
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |number| verify(number, deadline).is_ok(),
    )
    .unwrap();
    assert_eq!(target.field, Some(FIELD));
    assert_eq!(
        settle(target, &expect, deadline, &|| Ok(()), &mut verify).unwrap(),
        Some("AXTextField")
    );
    assert_eq!(reads, 3);
    assert!(fake.writes.is_empty());
}

#[test]
fn a_leading_focused_field_is_prepared_and_input_is_authorized_for_the_exact_field() {
    let mut fake = find_bar();
    for element in [FIELD, 3] {
        fake.answers
            .insert((element, "AXDescription"), Answer::Absent);
    }
    fake.answers
        .insert((FIELD, "AXTitle"), Answer::Text("Email".into()));
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Email, Work".into()));
    fake.frontmost.set(true);
    let expect = named().in_field("Email").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let mut observation = fake.clone();
    let listed = fake.windows.clone();
    let mut reads = 0;
    let mut verify = |number, until| {
        reads += 1;
        assert!(reads <= 3, "verification must remain bounded");
        assert_eq!(number, Some(42));
        let keyboard = read_find_bar(&mut observation, until);
        crate::native::focus::tests::verify_keyboard(
            &expect,
            keyboard,
            listed.clone(),
            until,
            Duration::ZERO,
        )
    };
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |number| verify(number, deadline).is_ok(),
    )
    .unwrap();
    assert_eq!(
        settle(target, &expect, deadline, &|| Ok(()), &mut verify).unwrap(),
        Some("AXTextField")
    );
    assert_eq!(reads, 3);
    assert_eq!(fake.focused_element.get(), FIELD);
    assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
}

#[test]
fn a_window_refusal_survives_a_later_production_verification_that_exceeds_its_deadline() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(150);
    let mut observation = fake.clone();
    let mut listed = fake.windows.clone();
    let mut other = listed[0].clone();
    other.number = 43;
    other.bounds.x = 600.0;
    listed.push(other);
    let expect = named();
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    let mut reads = 0;
    let error = settle(target, &expect, deadline, &|| Ok(()), |_, until| {
        reads += 1;
        assert!(reads <= 2, "settlement must remain bounded");
        let mut keyboard = read_find_bar(&mut observation, until);
        let slow = if reads == 1 {
            keyboard.window.as_mut().unwrap().frame.as_mut().unwrap().x = 600.0;
            Duration::ZERO
        } else {
            until.saturating_duration_since(Instant::now()) + Duration::from_millis(1)
        };
        crate::native::focus::tests::verify_keyboard(&expect, keyboard, listed.clone(), until, slow)
    })
    .unwrap_err();
    assert_eq!(reads, 2);
    assert_eq!(error.details["reason"], "window");
    assert!(error.error.contains("Window 43 has keyboard focus"));
    assert!(!error.details.contains_key("unread"));
    assert!(!error.uncertain);
}

#[test]
fn a_plain_failed_name_on_the_winner_also_prevents_selection() {
    for name in ["Email", "Email, Work"] {
        let mut fake = Fake::new();
        fake.answers
            .insert((FIELD, "AXDescription"), Answer::Text(name.into()));
        fake.answers
            .insert((FIELD, "AXTitle"), Answer::Failed(AXError::Failure));
        let error = fake.run(&named().in_field("Email").unwrap()).unwrap_err();
        assert_eq!(error.details["reason"], "field");
        assert!(error.error.contains("told apart"));
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_candidate_without_readable_names_refuses_even_beside_an_exact_match() {
    for readable_exact in [false, true] {
        let mut fake = find_bar();
        for attribute in ["AXTitle", "AXDescription", "AXIdentifier"] {
            fake.answers
                .insert((3, attribute), Answer::Failed(AXError::Failure));
        }
        let name = if readable_exact { "_NS:9" } else { "Email" };
        let error = fake.run(&named().in_field(name).unwrap()).unwrap_err();
        assert_eq!(error.details["unread"], "failed");
        assert!(fake.writes.is_empty());
        assert!(!fake.frontmost.get());
    }
}

#[test]
fn a_successful_focus_setter_cannot_authorize_input_to_a_leading_sibling_or_inner_editor() {
    for descendant in [false, true] {
        let mut fake = find_bar();
        for element in [FIELD, 3] {
            fake.answers
                .insert((element, "AXDescription"), Answer::Absent);
        }
        fake.answers
            .insert((FIELD, "AXTitle"), Answer::Text("Email".into()));
        fake.answers
            .insert((3, "AXTitle"), Answer::Text("Email, Work".into()));
        if descendant {
            fake.lists.insert((5, "AXChildren"), vec![]);
            fake.lists.insert((FIELD, "AXChildren"), vec![3]);
            fake.answers.insert((3, "AXParent"), Answer::Element(FIELD));
        }
        fake.after_write = Some(|fake| {
            if fake.writes.last() == Some(&(APP, Write::Flag("AXFrontmost"))) {
                fake.frontmost.set(true);
            }
        });
        let expect = named().in_field("Email").unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        let mut observation = fake.clone();
        let now = fake.now.clone();
        let target = prepare(
            &mut fake,
            &expect,
            Need::Keys,
            deadline,
            &|| Ok(()),
            &mut |_| false,
        )
        .unwrap();
        let mut reads = 0;
        let mut inputs = 0;
        let result = settle(target, &expect, deadline, &|| Ok(()), |_, until| {
            reads += 1;
            assert!(reads <= 5, "settlement must remain bounded");
            let keyboard = read_find_bar(&mut observation, until);
            let focused = keyboard.focused.unwrap().element;
            assert_eq!(focused.focused, Some(true));
            assert!(
                ElementFilter {
                    name: Some("Email".into()),
                    ..Default::default()
                }
                .matches(&focused)
            );
            now.set(now.get() + Duration::from_millis(100));
            Ok(Some("AXTextArea"))
        });
        if result.is_ok() {
            inputs += 1;
        }
        let error = result.unwrap_err();
        assert_eq!(error.details["reason"], "field");
        assert_eq!(inputs, 0);
        assert_eq!(fake.focused_element.get(), 3);
        assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
    }
}

#[test]
fn an_already_ready_field_still_requires_the_selected_element_to_hold_focus() {
    let mut fake = find_bar();
    fake.answers
        .insert((FIELD, "AXTitle"), Answer::Text("Email".into()));
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Email, Work".into()));
    fake.frontmost.set(true);
    fake.focused_element.set(FIELD);
    let focused = fake.focused_element.clone();
    let expect = named().in_field("Email").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| true,
    )
    .unwrap();
    focused.set(3);
    let mut reads = 0;
    let error = settle(target, &expect, deadline, &|| Ok(()), |_, _| {
        reads += 1;
        assert_eq!(reads, 1);
        Ok(Some("AXTextArea"))
    })
    .unwrap_err();
    assert_eq!(error.details["reason"], "field");
    assert!(fake.writes.is_empty());
}

#[test]
fn field_identity_is_read_again_after_the_second_destination_verification() {
    let mut fake = find_bar();
    fake.focused_element.set(FIELD);
    fake.frontmost.set(true);
    let focused = fake.focused_element.clone();
    let expect = named().in_field("_NS:9").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| true,
    )
    .unwrap();
    let mut reads = 0;
    let error = settle(target, &expect, deadline, &|| Ok(()), |_, _| {
        reads += 1;
        assert!(reads <= 2);
        if reads == 2 {
            focused.set(3);
        }
        Ok(Some("AXTextField"))
    })
    .unwrap_err();
    assert_eq!(error.details["reason"], "field");
    assert_eq!(reads, 2);
    assert!(fake.writes.is_empty());
}

#[test]
fn a_find_field_focus_write_that_is_rejected_has_a_field_reason() {
    let mut fake = find_bar();
    fake.after_write = Some(|fake| {
        if fake.writes.last() == Some(&(FIELD, Write::Flag("AXFocused"))) {
            fake.status = AXError::Failure;
        }
    });
    let error = fake.run(&named().in_field("_NS:9").unwrap()).unwrap_err();
    assert_eq!(error.details["reason"], "field");
    assert!(!error.uncertain);
    assert_eq!(fake.focused_element.get(), 3);
}

#[test]
fn a_find_field_focus_write_that_does_not_take_retains_the_field_refusal_at_the_deadline() {
    for expires_in_read in [true, false] {
        let mut fake = find_bar();
        fake.after_write = Some(|fake| {
            if fake.writes.last() == Some(&(APP, Write::Flag("AXFrontmost"))) {
                fake.frontmost.set(true);
            }
        });
        let expect = named().in_field("_NS:9").unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        let mut observation = fake.clone();
        let now = fake.now.clone();
        let target = prepare(
            &mut fake,
            &expect,
            Need::Keys,
            deadline,
            &|| Ok(()),
            &mut |_| false,
        )
        .unwrap();
        let mut reads = 0;
        let error = settle(target, &expect, deadline, &|| Ok(()), |_, until| {
            reads += 1;
            assert!(reads <= 2, "settlement must remain bounded");
            if reads == 2 {
                now.set(deadline);
                return Err(Unread::Slow.error());
            }
            let keyboard = read_find_bar(&mut observation, until);
            assert_eq!(keyboard.within, Within::Window);
            assert_eq!(
                keyboard.focused.unwrap().element.role.as_deref(),
                Some("AXTextArea")
            );
            if expires_in_read {
                now.set(deadline);
            }
            Err(refused(
                "field",
                "The field with keyboard focus is not the one named _NS:9",
            ))
        })
        .unwrap_err();
        assert_eq!(error.details["reason"], "field");
        assert!(!error.details.contains_key("unread"));
        assert!(!error.uncertain);
        assert_eq!(reads, if expires_in_read { 1 } else { 2 });
        assert_eq!(fake.focused_element.get(), 3);
    }
}

#[test]
fn a_failed_candidate_title_read_refuses_before_any_write() {
    failed_candidate_name("AXTitle");
}

#[test]
fn a_failed_candidate_description_read_refuses_before_any_write() {
    failed_candidate_name("AXDescription");
}

#[test]
fn a_failed_candidate_identifier_read_refuses_before_any_write() {
    failed_candidate_name("AXIdentifier");
}

fn failed_candidate_name(attribute: &'static str) {
    // Visit the readable match first, then the candidate whose name is unknown.
    let mut fake = Fake::new();
    fake.lists.insert((WINDOW, "AXChildren"), vec![3, FIELD]);
    fake.answers
        .insert((3, "AXRole"), Answer::Text("AXTextField".into()));
    fake.answers
        .insert((3, attribute), Answer::Failed(AXError::CannotComplete));
    let error = fake.run(&named().in_field("Email").unwrap()).unwrap_err();
    assert_eq!(error.details["unread"], "failed");
    assert!(fake.writes.is_empty());
}

#[test]
fn duplicate_disabled_and_incomplete_field_searches_never_write() {
    let target = named().in_field("Email").unwrap();
    let mut fake = Fake::new();
    fake.lists.insert((WINDOW, "AXChildren"), vec![FIELD, 3]);
    fake.answers
        .insert((3, "AXRole"), Answer::Text("AXTextField".into()));
    fake.answers
        .insert((3, "AXDescription"), Answer::Text("Email".into()));
    reason(fake.run(&target), "field");
    assert!(fake.writes.is_empty());
    fake = Fake::new();
    fake.answers
        .insert((FIELD, "AXEnabled"), Answer::Flag(false));
    reason(fake.run(&target), "disabled");
    assert!(fake.writes.is_empty());
    fake = Fake::new();
    fake.malformed = true;
    assert!(fake.run(&target).is_err());
    assert!(fake.writes.is_empty());
}

#[test]
fn unsupported_operations_are_skipped_and_a_lost_write_answer_is_uncertain() {
    let mut fake = Fake::new();
    fake.supported = false;
    fake.run(&named()).unwrap();
    assert!(fake.writes.is_empty());
    fake.supported = true;
    fake.status = AXError::CannotComplete;
    assert!(fake.run(&named()).unwrap_err().uncertain);
    assert_eq!(fake.writes.len(), 1);
}

#[test]
fn cancellation_during_a_write_prevents_the_next_write() {
    let mut fake = Fake::new();
    let calls = Cell::new(0);
    let deadline = fake.now.get() + Duration::from_millis(450);
    let result = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() >= 5 {
                Err(Error::new(Code::Cancelled, "Input cancelled"))
            } else {
                Ok(())
            }
        },
        &mut |_| false,
    );
    assert_eq!(result.err().unwrap().code, Code::Cancelled);
    assert_eq!(fake.writes.len(), 1);
}

#[test]
fn a_successful_guard_that_spends_the_remaining_budget_prevents_the_write() {
    let mut fake = Fake::new();
    let now = fake.now.clone();
    let deadline = now.get() + Duration::from_millis(450);
    let calls = Cell::new(0);
    let error = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| {
            calls.set(calls.get() + 1);
            if calls.get() == 4 {
                now.set(deadline);
            }
            Ok(())
        },
        &mut |_| false,
    )
    .err()
    .unwrap();
    assert_eq!(calls.get(), 4);
    assert_eq!(error.details["unread"], "slow");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_target_can_be_prepared_when_no_app_or_window_has_focus() {
    let mut fake = Fake::new();
    fake.answers.remove(&(APP, "AXFocusedWindow"));
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    let role = settle(target, &named(), deadline, &|| Ok(()), |number, _| {
        assert_eq!(number, Some(42));
        Ok(Some("AXTextField"))
    })
    .unwrap();
    assert_eq!(role, Some("AXTextField"));
    assert_eq!(fake.writes[0], (APP, Write::Flag("AXFrontmost")));
}

#[test]
fn an_app_only_point_selects_its_own_frontmost_window_before_raising() {
    let mut fake = Fake::new();
    let mut covering = fake.windows[0].clone();
    covering.pid = 999;
    covering.number = 43;
    fake.windows.insert(0, covering);
    let expect = Expect::new(812).unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &expect,
        Need::Point(30.0, 60.0),
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    assert_eq!(target.number, Some(42));
    settle(target, &expect, deadline, &|| Ok(()), |number, _| {
        assert_eq!(number, Some(42));
        Ok(None)
    })
    .unwrap();
    assert!(fake.writes.contains(&(WINDOW, Write::Raise)));
}

#[test]
fn an_app_only_point_without_a_candidate_makes_no_writes() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let error = prepare(
        &mut fake,
        &Expect::new(812).unwrap(),
        Need::Point(900.0, 700.0),
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .err()
    .unwrap();
    assert_eq!(error.details["reason"], "point");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_point_three_pixels_outside_the_named_document_refuses_before_readiness_or_writes() {
    for need in [
        Need::Point(701.0, 310.0),
        Need::Drag {
            from: (701.0, 310.0),
            to: (300.0, 310.0),
        },
    ] {
        let mut fake = Fake::new();
        fake.windows = [
            (41633, 210.0, 128.0),
            (41631, 181.0, 99.0),
            (41629, 152.0, 70.0),
        ]
        .into_iter()
        .map(|(number, x, y)| Window {
            number,
            bounds: Bounds {
                x,
                y,
                width: 517.0,
                height: 422.0,
            },
            ..fake.windows[0].clone()
        })
        .collect();
        for (element, x, y) in [(3, 210.0, 128.0), (WINDOW, 181.0, 99.0), (4, 152.0, 70.0)] {
            fake.window(element);
            fake.answers
                .insert((element, "AXPosition"), Answer::Point(x, y));
            fake.answers
                .insert((element, "AXSize"), Answer::Size(517.0, 422.0));
        }
        fake.lists.insert((APP, "AXWindows"), vec![3, WINDOW, 4]);
        let expect = Expect::new(812).unwrap().in_window(41631).unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        let error = prepare(&mut fake, &expect, need, deadline, &|| Ok(()), &mut |_| {
            panic!("verification must not consume the preparation deadline")
        })
        .err()
        .unwrap();
        assert_eq!(error.code, Code::FocusChanged);
        assert_eq!(error.details["reason"], "window");
        assert!(error.error.contains("41631 is not at that point"));
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_window_refusal_is_preserved_when_the_next_admission_scan_runs_out_of_time() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    let now = target.reader.ask.now.clone();
    target.reader.ask.delay = Duration::from_millis(2);
    let mut reads = 0;
    let error = settle(target, &named(), deadline, &|| Ok(()), |_, _| {
        reads += 1;
        assert_eq!(reads, 1);
        // Leave less than one admission read, which advances the fake clock.
        now.set(deadline - Duration::from_millis(1));
        Err(refused(
            "window",
            "Window 43 has keyboard focus, not window 42",
        ))
    });
    reason(error.map(|_| ()), "window");
}

#[test]
fn a_menu_bar_point_activates_only_the_app() {
    let mut fake = Fake::new();
    fake.windows.insert(
        0,
        Window {
            number: 50,
            pid: 1,
            owner: Some("Window Server".into()),
            layer: windows::MENU_BAR_LAYER,
            bounds: Bounds {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 24.0,
            },
            ..fake.windows[0].clone()
        },
    );
    let deadline = fake.now.get() + Duration::from_millis(450);
    prepare(
        &mut fake,
        &Expect::new(812).unwrap(),
        Need::Point(100.0, 10.0),
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    assert_eq!(fake.writes, [(APP, Write::Flag("AXFrontmost"))]);
}

#[test]
fn a_sheet_appearing_during_activation_prevents_any_raise() {
    let mut fake = Fake::new();
    fake.after_write = Some(|fake| {
        fake.lists.insert((WINDOW, "AXSheets"), vec![3]);
    });
    reason(fake.run(&named()), "sheet");
    assert_eq!(fake.writes, [(APP, Write::Flag("AXFrontmost"))]);
}

#[test]
fn field_search_prefers_a_whole_name_and_stays_in_its_selected_window() {
    let mut fake = Fake::new();
    fake.lists.insert((WINDOW, "AXChildren"), vec![FIELD, 3, 4]);
    fake.answers
        .insert((3, "AXRole"), Answer::Text("AXTextField".into()));
    fake.answers
        .insert((3, "AXDescription"), Answer::Text("Email, Work".into()));
    fake.answers
        .insert((4, "AXRole"), Answer::Text("AXWindow".into()));
    fake.lists.insert((4, "AXChildren"), vec![5]);
    fake.answers
        .insert((5, "AXRole"), Answer::Text("AXTextField".into()));
    fake.answers
        .insert((5, "AXDescription"), Answer::Text("Email".into()));
    fake.run(&named().in_field("Email").unwrap()).unwrap();
    assert_eq!(fake.writes.last(), Some(&(FIELD, Write::Flag("AXFocused"))));
    assert!(
        !fake
            .writes
            .iter()
            .any(|(element, _)| *element == 3 || *element == 5)
    );
}

#[test]
fn over_limit_lists_cut_names_and_late_reads_prove_nothing() {
    let mut fake = Fake::new();
    fake.lists
        .insert((APP, "AXWindows"), vec![WINDOW; WINDOWS + 1]);
    assert!(fake.run(&named()).is_err());
    assert!(fake.writes.is_empty());
    fake = Fake::new();
    fake.answers.insert((WINDOW, "AXTitle"), Answer::Cut);
    assert!(fake.run(&named()).is_err());
    assert!(fake.writes.is_empty());
    fake = Fake::new();
    fake.delay = Duration::from_millis(251);
    assert_eq!(fake.run(&named()).unwrap_err().details["unread"], "slow");
    assert!(fake.writes.is_empty());
}

#[test]
fn an_epoch_or_permission_change_stops_before_the_next_write() {
    for code in [Code::Identity, Code::Permission] {
        let mut fake = Fake::new();
        let calls = Cell::new(0);
        let deadline = fake.now.get() + Duration::from_millis(450);
        let error = prepare(
            &mut fake,
            &named(),
            Need::Keys,
            deadline,
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() == 5 {
                    Err(Error::new(code, "The input session or permission changed"))
                } else {
                    Ok(())
                }
            },
            &mut |_| false,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, code);
        assert_eq!(fake.writes.len(), 1);
    }
}

#[test]
fn a_confirmed_write_needs_fresh_proof_and_does_not_expand_the_reply() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    let error = settle(target, &named(), deadline, &|| Ok(()), |_, _| {
        Err(refused("sheet", "A sheet blocks the target"))
    })
    .unwrap_err();
    assert_eq!(error.details["reason"], "sheet");
    assert!(!error.uncertain);
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("private-title-sentinel")
    );
}

#[test]
fn an_open_menu_uses_the_app_check_without_resolving_or_raising_its_window() {
    let mut fake = Fake::new();
    fake.windows[0].layer = windows::MENU_LAYER;
    fake.lists.insert((APP, "AXWindows"), Vec::new());
    let deadline = fake.now.get() + Duration::from_millis(450);
    prepare(
        &mut fake,
        &Expect::new(812).unwrap(),
        Need::Point(30.0, 60.0),
        deadline,
        &|| Ok(()),
        &mut |number| {
            assert_eq!(number, None);
            true
        },
    )
    .unwrap();
    assert!(fake.writes.is_empty());
}

#[test]
fn a_sheet_discovered_during_final_verification_still_refuses() {
    let mut fake = Fake::new();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    target
        .reader
        .ask
        .lists
        .insert((WINDOW, "AXSheets"), vec![3]);
    let mut observed = false;
    let error = settle(target, &named(), deadline, &|| Ok(()), |_, _| {
        observed = true;
        Ok(None)
    })
    .unwrap_err();
    assert!(!observed);
    assert_eq!(error.details["reason"], "sheet");
}

#[test]
fn no_current_window_does_not_hide_a_modal_window_from_admission() {
    let mut fake = Fake::new();
    fake.answers.remove(&(APP, "AXFocusedWindow"));
    fake.answers.insert((WINDOW, "AXModal"), Answer::Flag(true));
    reason(fake.run(&Expect::new(812).unwrap()), "sheet");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_focused_sheet_blocks_pointer_and_drag_preparation_in_an_inactive_app() {
    for need in [
        Need::Point(20.0, 260.0),
        Need::Drag {
            from: (20.0, 260.0),
            to: (60.0, 80.0),
        },
    ] {
        for expect in [named(), Expect::new(812).unwrap()] {
            let mut fake = Fake::new();
            fake.answers
                .insert((APP, "AXFocusedUIElement"), Answer::Element(FIELD));
            fake.answers.insert((FIELD, "AXParent"), Answer::Element(3));
            fake.sheet(3, WINDOW);
            fake.lists.remove(&(WINDOW, "AXSheets"));
            let deadline = fake.now.get() + Duration::from_millis(450);
            let error = prepare(&mut fake, &expect, need, deadline, &|| Ok(()), &mut |_| {
                false
            })
            .err()
            .unwrap();
            assert_eq!(error.details["reason"], "sheet");
            assert!(fake.writes.is_empty());
        }
    }
}

#[test]
fn field_focus_is_resolved_again_for_each_document_without_remembered_values() {
    let mut fake = Fake::new();
    let mut other = fake.windows[0].clone();
    other.number = 43;
    other.title = Some("another-private-title".into());
    other.bounds.x = 500.0;
    fake.windows.push(other);
    fake.window(3);
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("another-private-title".into()));
    fake.answers
        .insert((3, "AXPosition"), Answer::Point(500.0, 30.0));
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
    fake.lists.insert((3, "AXChildren"), vec![4]);
    fake.answers
        .insert((4, "AXRole"), Answer::Text("AXTextField".into()));
    fake.answers
        .insert((4, "AXDescription"), Answer::Text("Email".into()));
    fake.answers.insert((4, "AXParent"), Answer::Element(3));
    for (number, field) in [(42, FIELD), (43, 4), (42, FIELD)] {
        fake.writes.clear();
        fake.run(
            &Expect::new(812)
                .unwrap()
                .in_window(number)
                .unwrap()
                .in_field("Email")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fake.writes.last(), Some(&(field, Write::Flag("AXFocused"))));
    }
}

#[test]
fn field_readiness_is_polled_after_a_confirmed_focus_request() {
    let mut fake = Fake::new();
    fake.answers
        .insert((APP, "AXFocusedUIElement"), Answer::Element(FIELD));
    let expect = named().in_field("Email").unwrap();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &expect,
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| false,
    )
    .unwrap();
    let mut observations = 0;
    let role = settle(target, &expect, deadline, &|| Ok(()), |_, _| {
        observations += 1;
        assert!(observations <= 3);
        if observations == 1 {
            Err(refused("field", "The field has not gained focus yet"))
        } else {
            Ok(Some("AXTextField"))
        }
    })
    .unwrap();
    assert_eq!(observations, 3);
    assert_eq!(role, Some("AXTextField"));
}

#[test]
fn application_admission_requires_a_live_process_owned_by_the_helper() {
    let info = process_info(812, [100, 200]);
    assert_eq!(
        application_with(812, 502, |pid| {
            assert_eq!(pid, 812);
            Some(info.clone())
        }),
        Some(info.instance)
    );
    for facts in [
        None,
        Some(Info {
            uid: 501,
            ..info.clone()
        }),
        Some(Info {
            zombie: true,
            ..info.clone()
        }),
        Some(process_info(813, [100, 200])),
    ] {
        assert_eq!(application_with(812, 502, |_| facts), None);
    }
    for pid in [0, -1, i64::MAX] {
        assert_eq!(
            application_with(pid, 502, |_| panic!(
                "invalid pid reached the kernel lookup"
            )),
            None
        );
    }
}

#[test]
fn a_sheet_over_another_document_does_not_block_the_selected_document() {
    let mut fake = Fake::new();
    fake.window(3);
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Other document".into()));
    fake.answers
        .insert((3, "AXPosition"), Answer::Point(600.0, 30.0));
    fake.sheet(4, 3);
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3, 4]);
    fake.answers
        .insert((APP, "AXFocusedWindow"), Answer::Element(4));
    fake.answers
        .insert((APP, "AXFocusedUIElement"), Answer::Element(5));
    fake.answers
        .insert((5, "AXRole"), Answer::Text("AXButton".into()));
    fake.answers.insert((5, "AXParent"), Answer::Element(4));
    fake.run(&named()).unwrap();
    assert!(fake.writes.contains(&(WINDOW, Write::Raise)));
    fake.writes.clear();
    reason(fake.run(&Expect::new(812).unwrap()), "sheet");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_separately_listed_sheet_with_an_unknown_parent_refuses_before_writing() {
    let mut fake = Fake::new();
    fake.window(3);
    fake.answers
        .insert((3, "AXRole"), Answer::Text("AXSheet".into()));
    fake.answers
        .insert((3, "AXTitle"), Answer::Text("Page Setup".into()));
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 3]);
    assert!(fake.run(&named()).is_err());
    assert!(fake.writes.is_empty());
}

fn listed_sheet(fake: &mut Fake) {
    fake.sheet(3, WINDOW);
    fake.windows.insert(
        0,
        Window {
            number: 43,
            title: Some("Page Setup".into()),
            bounds: Bounds {
                x: 30.0,
                y: 50.0,
                width: 500.0,
                height: 180.0,
            },
            ..fake.windows[0].clone()
        },
    );
}

fn child_only_sheet(fake: &mut Fake) {
    listed_sheet(fake);
    fake.lists.remove(&(WINDOW, "AXSheets"));
    fake.lists.insert((WINDOW, "AXChildren"), vec![FIELD, 3]);
    fake.windows[0].title = None;
    fake.answers
        .insert((APP, "AXFocusedUIElement"), Answer::Element(4));
    fake.answers
        .insert((4, "AXRole"), Answer::Text("AXButton".into()));
    fake.answers.insert((4, "AXParent"), Answer::Element(3));
}

#[test]
fn a_numbered_sheet_reachable_only_through_children_resolves_before_keyboard_preparation() {
    let mut fake = Fake::new();
    child_only_sheet(&mut fake);
    let deadline = fake.now.get() + Duration::from_millis(450);
    let mut reader = Reader {
        ask: &mut fake,
        deadline,
    };
    assert_eq!(reader.resolve(&APP, 812, 43).unwrap(), 3);
    fake.run(&Expect::new(812).unwrap().in_window(43).unwrap())
        .unwrap();
    assert!(fake.writes.contains(&(3, Write::Raise)));
    assert!(!fake.writes.contains(&(WINDOW, Write::Raise)));
}

#[test]
fn a_child_only_sheet_button_requires_its_exact_sheet_expectation() {
    for (separate, requested) in [(true, 43), (true, 42), (false, 42)] {
        let mut fake = Fake::new();
        child_only_sheet(&mut fake);
        if !separate {
            fake.windows.remove(0);
        }
        let expect = Expect::new(812).unwrap().in_window(requested).unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        // A separately listed sheet can receive a point outside its document.
        let point = if separate {
            (480.0, 150.0)
        } else {
            (100.0, 150.0)
        };
        let result = prepare(
            &mut fake,
            &expect,
            Need::Point(point.0, point.1),
            deadline,
            &|| Ok(()),
            &mut |_| false,
        );
        if requested == 42 {
            reason(result.map(|_| ()), "sheet");
            assert!(fake.writes.is_empty());
            continue;
        }
        let target = result.unwrap();
        assert_eq!(target.window, Some(3));
        let listed = target.reader.ask.windows.clone();
        let mut verified = 0;
        settle(target, &expect, deadline, &|| Ok(()), |selected, _| {
            assert_eq!(selected, Some(if separate { 43 } else { 42 }));
            let screen = windows::Screen::new(
                Bounds::display(1000.0, 800.0),
                windows::Dock::Icons(Vec::new()),
            );
            assert!(windows::unobstructed(
                &listed,
                selected.unwrap(),
                point.0,
                point.1,
                &screen
            ));
            verified += 1;
            Ok(None)
        })
        .unwrap();
        assert_eq!(verified, 2);
        assert!(!fake.writes.contains(&(WINDOW, Write::Raise)));
    }
}

#[test]
fn a_child_only_sheet_is_rechecked_without_blocking_another_document() {
    let mut fake = Fake::new();
    child_only_sheet(&mut fake);
    fake.window(5);
    fake.answers
        .insert((5, "AXPosition"), Answer::Point(600.0, 30.0));
    fake.windows.push(Window {
        number: 44,
        bounds: Bounds {
            x: 600.0,
            ..fake.windows[1].bounds.clone()
        },
        ..fake.windows[1].clone()
    });
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 5]);
    let expect = Expect::new(812).unwrap().in_window(43).unwrap();
    fake.run(&expect).unwrap();
    fake.writes.clear();
    reason(fake.run(&named()), "sheet");
    assert!(fake.writes.is_empty());
    fake.run(&Expect::new(812).unwrap().in_window(44).unwrap())
        .unwrap();
    assert!(fake.writes.contains(&(5, Write::Raise)));
    fake.writes.clear();

    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &expect,
        Need::Point(100.0, 150.0),
        deadline,
        &|| Ok(()),
        &mut |_| true,
    )
    .unwrap();
    target.reader.ask.lists.remove(&(WINDOW, "AXChildren"));
    let mut verified = false;
    let error = settle(target, &expect, deadline, &|| Ok(()), |_, _| {
        verified = true;
        Ok(None)
    })
    .unwrap_err();
    assert_eq!(error.details["unread"], "unlisted");
    assert!(!verified);
    assert!(fake.writes.is_empty());
}

#[test]
fn child_sheet_discovery_deduplicates_sources_and_rejects_unknown_or_conflicting_relationships() {
    let mut fake = Fake::new();
    child_only_sheet(&mut fake);
    let expect = Expect::new(812).unwrap().in_window(43).unwrap();
    fake.run(&expect).unwrap();
    fake.lists.insert((WINDOW, "AXSheets"), vec![3]);
    fake.run(&expect).unwrap();
    fake.writes.clear();
    fake.window(5);
    fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 5]);
    fake.lists.insert((5, "AXChildren"), vec![3]);
    assert_eq!(fake.run(&expect).unwrap_err().details["unread"], "alike");
    assert!(fake.writes.is_empty());
    fake.lists.remove(&(5, "AXChildren"));
    fake.lists.remove(&(WINDOW, "AXSheets"));
    fake.answers
        .insert((3, "AXRole"), Answer::Failed(AXError::CannotComplete));
    assert_eq!(fake.run(&expect).unwrap_err().details["unread"], "failed");
    assert!(fake.writes.is_empty());
}

#[test]
fn a_sheet_omitted_from_ax_windows_can_be_targeted_by_its_own_number() {
    let mut fake = Fake::new();
    listed_sheet(&mut fake);
    fake.run(&Expect::new(812).unwrap().in_window(43).unwrap())
        .unwrap();
    assert!(fake.writes.contains(&(3, Write::Raise)));
    assert!(!fake.writes.contains(&(WINDOW, Write::Raise)));
}

#[test]
fn a_sheet_button_click_requires_the_sheet_number_and_refuses_a_parent_expectation() {
    for (separate, requested) in [(true, 43), (true, 42), (false, 42)] {
        let mut fake = Fake::new();
        listed_sheet(&mut fake);
        if !separate {
            fake.windows.remove(0);
        }
        let expect = Expect::new(812).unwrap().in_window(requested).unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        let result = prepare(
            &mut fake,
            &expect,
            Need::Point(100.0, 150.0),
            deadline,
            &|| Ok(()),
            &mut |_| false,
        );
        if requested == 42 {
            reason(result.map(|_| ()), "sheet");
            assert!(fake.writes.is_empty());
            continue;
        }
        let target = result.unwrap();
        assert_eq!(target.window, Some(3));
        let number = Some(if separate { 43 } else { 42 });
        assert_eq!(target.number, number);
        settle(target, &expect, deadline, &|| Ok(()), |selected, _| {
            assert_eq!(selected, number);
            Ok(None)
        })
        .unwrap();
        assert!(!fake.writes.contains(&(WINDOW, Write::Raise)));
    }
}

#[test]
fn a_sheet_that_moves_or_closes_during_button_preparation_prevents_the_click() {
    for closes in [true, false] {
        let mut fake = Fake::new();
        listed_sheet(&mut fake);
        let expect = Expect::new(812).unwrap().in_window(43).unwrap();
        let deadline = fake.now.get() + Duration::from_millis(450);
        let target = prepare(
            &mut fake,
            &expect,
            Need::Point(100.0, 150.0),
            deadline,
            &|| Ok(()),
            &mut |_| false,
        )
        .unwrap();
        if closes {
            target.reader.ask.lists.remove(&(WINDOW, "AXSheets"));
        } else {
            target
                .reader
                .ask
                .answers
                .insert((3, "AXPosition"), Answer::Point(600.0, 400.0));
        }
        let mut verified = false;
        assert!(
            settle(target, &expect, deadline, &|| Ok(()), |_, _| {
                verified = true;
                Ok(None)
            })
            .is_err()
        );
        assert!(!verified);
    }
}

#[test]
fn the_live_backend_resolves_a_process_launched_after_it_was_created() {
    let mut backend = Live;
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("1")
        .spawn()
        .unwrap();
    let pid = i64::from(child.id());
    let app = backend.app(pid);
    let _ = child.kill();
    child.wait().unwrap();
    assert_eq!(app.as_ref().and_then(|app| backend.owner(app)), Some(pid));
}

#[test]
fn cyclic_or_conflicting_sheet_parents_refuse_before_writing() {
    for cycle in [true, false] {
        let mut fake = Fake::new();
        fake.sheet(3, WINDOW);
        if cycle {
            fake.lists.insert((3, "AXSheets"), vec![WINDOW]);
        } else {
            fake.window(4);
            fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 4]);
            fake.lists.insert((4, "AXSheets"), vec![3]);
        }
        assert!(fake.run(&named()).is_err());
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_sheet_over_a_previously_selected_document_point_never_receives_the_click() {
    for separate in [true, false] {
        let mut fake = Fake::new();
        listed_sheet(&mut fake);
        if !separate {
            fake.windows.remove(0);
        }
        let deadline = fake.now.get() + Duration::from_millis(450);
        let result = prepare(
            &mut fake,
            &named(),
            Need::Point(100.0, 150.0),
            deadline,
            &|| Ok(()),
            &mut |_| false,
        );
        reason(result.map(|_| ()), "sheet");
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn a_later_admission_scan_cannot_reuse_an_earlier_window_proof() {
    let mut fake = Fake::new();
    fake.window(5);
    fake.answers
        .insert((5, "AXTitle"), Answer::Text("Other document".into()));
    let observed = fake.observed_window.clone();
    let deadline = fake.now.get() + Duration::from_millis(450);
    let target = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &|| Ok(()),
        &mut |_| true,
    )
    .unwrap();
    target.reader.ask.on_ask = Some(|fake, element, name| {
        if element == APP && name == "AXFocusedWindow" && fake.observed_window.get() == 0 {
            fake.observed_window.set(5);
            fake.answers
                .insert((APP, "AXFocusedWindow"), Answer::Element(5));
        }
    });
    let mut verified = 0;
    let result = settle(target, &named(), deadline, &|| Ok(()), |_, _| {
        verified += 1;
        if observed.get() == WINDOW {
            observed.set(0);
            Ok(None)
        } else {
            Err(refused("window", "Another document has focus"))
        }
    });
    reason(result.map(|_| ()), "window");
    assert_eq!(verified, 2);
    assert!(fake.writes.is_empty());
}

#[test]
fn app_only_admission_refuses_a_sheet_focused_after_the_initial_document_read() {
    for selected in [None, Some(42)] {
        let mut fake = Fake::new();
        fake.window(5);
        fake.answers
            .insert((5, "AXTitle"), Answer::Text("Other document".into()));
        fake.lists.insert((APP, "AXWindows"), vec![WINDOW, 5]);
        fake.on_ask = Some(|fake, element, name| {
            if element == APP && name == "AXFocusedWindow" {
                if fake.observed_window.replace(5) == WINDOW {
                    // The initial read still returns A; the next scan sees B's sheet.
                    fake.sheet(3, 5);
                } else {
                    fake.answers
                        .insert((APP, "AXFocusedWindow"), Answer::Element(3));
                    fake.answers
                        .insert((APP, "AXFocusedUIElement"), Answer::Element(3));
                }
            }
        });
        let expect = match selected {
            Some(number) => Expect::new(812).unwrap().in_window(number).unwrap(),
            None => Expect::new(812).unwrap(),
        };
        let deadline = fake.now.get() + Duration::from_millis(450);
        let result = prepare(
            &mut fake,
            &expect,
            Need::Keys,
            deadline,
            &|| Ok(()),
            &mut |_| false,
        );
        if selected.is_some() {
            result.unwrap();
            assert!(fake.writes.contains(&(WINDOW, Write::Raise)));
        } else {
            reason(result.map(|_| ()), "sheet");
            assert!(fake.writes.is_empty());
        }
    }
}

fn process_info(pid: i32, start: crate::process::Start) -> Info {
    Info {
        instance: Instance::new(pid, start),
        ppid: 1,
        pgid: pid,
        uid: 502,
        ruid: 502,
        svuid: 502,
        zombie: false,
        stopped: false,
        name: "Editor".into(),
    }
}

#[test]
fn a_reused_pid_cannot_prepare_or_verify_the_replacement_app() {
    for replaced_before_preparation in [true, false] {
        let mut fake = Fake::new();
        let target = application_with(812, 502, |_| Some(process_info(812, [100, 200]))).unwrap();
        let current = Cell::new(Some(target));
        let replacement =
            application_with(812, 502, |_| Some(process_info(812, [101, 200]))).unwrap();
        let guard = || same_application(target, current.get());
        if replaced_before_preparation {
            current.set(Some(replacement));
        }
        let deadline = fake.now.get() + Duration::from_millis(450);
        let mut verified = false;
        let prepared = prepare(
            &mut fake,
            &named(),
            Need::Keys,
            deadline,
            &guard,
            &mut |_| {
                current.set(Some(replacement));
                true
            },
        );
        let result = prepared.and_then(|prepared| {
            settle(prepared, &named(), deadline, &guard, |_, _| {
                verified = true;
                Ok(None)
            })
        });
        reason(result.map(|_| ()), "app");
        assert!(!verified);
        assert!(fake.writes.is_empty());
    }
}

#[test]
fn replacing_the_process_during_final_verification_invalidates_the_result() {
    let mut fake = Fake::new();
    let target = application_with(812, 502, |_| Some(process_info(812, [100, 200]))).unwrap();
    let current = Cell::new(Some(target));
    let deadline = fake.now.get() + Duration::from_millis(450);
    let guard = || same_application(target, current.get());
    let prepared = prepare(
        &mut fake,
        &named(),
        Need::Keys,
        deadline,
        &guard,
        &mut |_| true,
    )
    .unwrap();
    let result = settle(prepared, &named(), deadline, &guard, |_, _| {
        current.set(Some(Instance::new(812, [100, 201])));
        Ok(None)
    });
    reason(result.map(|_| ()), "app");
    assert!(fake.writes.is_empty());
    for current in [None, Some(Instance::new(813, target.start))] {
        reason(same_application(target, current), "app");
    }
    same_application(target, Some(target)).unwrap();
}

#[test]
fn the_native_target_retains_and_rechecks_the_kernel_start_time() {
    let pid = std::process::id() as i32;
    let target = application(pid.into()).unwrap();
    assert_eq!(target, crate::process::info(pid).unwrap().unwrap().instance);
    check_application(Some(target)).unwrap();
    check_application(None).unwrap();
    let stale = Instance::new(pid, [target.start[0], (target.start[1] + 1) % 1_000_000]);
    reason(check_application(Some(stale)), "app");
}

#[test]
fn a_controller_pinned_instance_is_not_replaced_at_native_admission() {
    let pid = std::process::id() as i32;
    let current = application(pid.into()).unwrap();
    let stale = [current.start[0], current.start[1] ^ 1];
    let expect = Expect::from_request(&serde_json::json!({"pid":pid,"start":stale}))
        .unwrap()
        .unwrap();
    let error = target_application(&expect).unwrap_err();
    assert_eq!(error.code, Code::FocusChanged);
    assert_eq!(error.details["reason"], "app");
    let expect = Expect::from_request(&serde_json::json!({"pid":pid,"start":current.start}))
        .unwrap()
        .unwrap();
    assert_eq!(target_application(&expect).unwrap(), current);
    assert_eq!(target_application(&expect.app_only()).unwrap(), current);
    assert_eq!(
        target_application(&Expect::new(pid.into()).unwrap()).unwrap(),
        current
    );
}
