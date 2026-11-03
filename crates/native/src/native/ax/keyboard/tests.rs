use super::*;
use std::collections::HashMap;

/// The app's element in a test app.
const APP: usize = 0;
const WINDOW: usize = 1;
const SHEET: usize = 2;
const GROUP: usize = 3;
const FIELD: usize = 4;

/// A test app: answers by element and attribute, each taking its time
/// on a clock that moves only as it answers. It keeps every timeout it
/// was asked to set, and the attributes asked for.
struct Fake {
    started: Instant,
    elapsed: Duration,
    answers: HashMap<(usize, &'static str), (Answer<usize>, Duration)>,
    bounds: Vec<Duration>,
    asked: Vec<(usize, &'static str)>,
    bounded: bool,
    owners: HashMap<usize, i64>,
}

impl Fake {
    /// A window holding a group holding a field named Email, focused.
    fn app() -> Self {
        let mut fake = Self {
            started: Instant::now(),
            elapsed: Duration::ZERO,
            answers: HashMap::new(),
            bounds: Vec::new(),
            asked: Vec::new(),
            bounded: true,
            owners: HashMap::new(),
        };
        let text = |text: &str| Answer::Text(text.to_owned());
        fake.set(APP, "AXFocusedWindow", Answer::Element(WINDOW));
        fake.set(APP, "AXFocusedUIElement", Answer::Element(FIELD));
        fake.set(WINDOW, "AXRole", text("AXWindow"));
        fake.set(WINDOW, "AXSubrole", text("AXStandardWindow"));
        fake.set(WINDOW, "AXTitle", text("Letter.txt"));
        fake.set(WINDOW, "AXPosition", Answer::Point(100.0, 100.0));
        fake.set(WINDOW, "AXSize", Answer::Size(400.0, 300.0));
        fake.set(FIELD, "AXRole", text("AXTextField"));
        fake.set(FIELD, "AXDescription", text("Email"));
        fake.set(FIELD, "AXEnabled", Answer::Flag(true));
        fake.set(FIELD, "AXFocused", Answer::Flag(true));
        fake.set(FIELD, "AXParent", Answer::Element(GROUP));
        fake.set(GROUP, "AXRole", text("AXGroup"));
        fake.set(GROUP, "AXParent", Answer::Element(WINDOW));
        fake
    }

    fn set(&mut self, element: usize, attribute: &'static str, answer: Answer<usize>) {
        self.answers
            .insert((element, attribute), (answer, Duration::from_millis(1)));
    }

    /// Make `attribute` of `element` take `delay` to answer.
    fn slow(&mut self, element: usize, attribute: &'static str, delay: Duration) {
        if let Some(entry) = self.answers.get_mut(&(element, attribute)) {
            entry.1 = delay;
        }
    }
}

impl Ask for &mut Fake {
    type Element = usize;

    fn now(&self) -> Instant {
        self.started + self.elapsed
    }

    fn bound(&mut self, _: &usize, timeout: Duration) -> bool {
        self.bounds.push(timeout);
        self.bounded
    }

    fn ask(&mut self, element: &usize, attribute: &'static str) -> Answer<usize> {
        self.asked.push((*element, attribute));
        let (answer, delay) = self
            .answers
            .get(&(*element, attribute))
            .cloned()
            .unwrap_or((Answer::Absent, Duration::from_millis(1)));
        self.elapsed += delay;
        answer
    }

    fn owner(&mut self, element: &usize) -> Option<i64> {
        Some(*self.owners.get(element).unwrap_or(&812))
    }
}

/// Both the window and the field judged.
const BOTH: Judged = Judged {
    window: true,
    field: true,
};

/// A read of `fake` with `budget` to spend, judging both.
fn read_with(fake: &mut Fake, budget: Duration) -> Result<Keyboard> {
    let deadline = fake.started + budget;
    read(&mut *fake, &APP, deadline, BOTH)
}

fn unread(outcome: Result<Keyboard>) -> serde_json::Value {
    outcome.unwrap_err().details["unread"].clone()
}

fn budget() -> Duration {
    Duration::from_millis(300)
}

#[test]
fn a_focused_field_is_read_whole_with_where_it_sits() {
    let mut fake = Fake::app();
    let keyboard = read_with(&mut fake, budget()).unwrap();
    let window = keyboard.window.unwrap();
    assert_eq!(window.subrole.as_deref(), Some("AXStandardWindow"));
    assert_eq!(
        window.frame,
        Some(Bounds {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0
        })
    );
    let focused = keyboard.focused.unwrap();
    assert_eq!(focused.pid, 812);
    assert_eq!(focused.element.description.as_deref(), Some("Email"));
    assert_eq!(focused.element.enabled, Some(true));
    assert_eq!(keyboard.within, Within::Window);
    assert!(
        !fake
            .asked
            .iter()
            .any(|(_, attribute)| *attribute == "AXValue"),
        "no value is ever read"
    );
}

#[test]
fn every_request_is_bounded_by_what_is_left_just_before_it() {
    let mut fake = Fake::app();
    fake.slow(WINDOW, "AXTitle", Duration::from_millis(120));
    read_with(&mut fake, budget()).unwrap();

    assert_eq!(fake.bounds.len(), fake.asked.len(), "one bound per request");
    assert_eq!(fake.bounds[0], Duration::from_millis(250));
    assert!(
        fake.bounds.windows(2).all(|pair| pair[1] <= pair[0]),
        "each bound is what is left, never the first one again: {:?}",
        fake.bounds
    );
    let after_title = fake
        .asked
        .iter()
        .position(|asked| *asked == (WINDOW, "AXTitle"))
        .unwrap();
    assert!(fake.bounds[after_title + 1] < budget() - Duration::from_millis(120));
}

#[test]
fn a_read_that_runs_past_its_deadline_is_refused_as_slow() {
    let mut fake = Fake::app();
    fake.slow(FIELD, "AXDescription", Duration::from_millis(400));
    assert_eq!(unread(read_with(&mut fake, budget())), "slow");
    assert_eq!(
        fake.asked.last(),
        Some(&(FIELD, "AXDescription")),
        "nothing more is asked once the time is spent"
    );
}

#[test]
fn a_timeout_that_cannot_be_set_stops_the_read_at_once() {
    let mut fake = Fake::app();
    fake.bounded = false;
    assert_eq!(unread(read_with(&mut fake, budget())), "failed");
    assert!(fake.asked.is_empty(), "nothing is asked unbounded");
}

#[test]
fn a_failed_read_of_anything_judged_refuses_where_absence_would_not() {
    for (element, attribute) in [
        (WINDOW, "AXRole"),
        (WINDOW, "AXSubrole"),
        (WINDOW, "AXTitle"),
        (WINDOW, "AXSize"),
        (FIELD, "AXRole"),
        (FIELD, "AXTitle"),
        (FIELD, "AXIdentifier"),
        (FIELD, "AXEnabled"),
        (FIELD, "AXFocused"),
        (GROUP, "AXParent"),
    ] {
        let mut failed = Fake::app();
        failed.set(element, attribute, Answer::Failed(AXError::CannotComplete));
        assert_eq!(
            unread(read_with(&mut failed, budget())),
            "slow",
            "{element} {attribute}"
        );
    }

    for attribute in ["AXRole", "AXEnabled", "AXFocused"] {
        let mut failed = Fake::app();
        failed.set(FIELD, attribute, Answer::Failed(AXError::Failure));
        assert_eq!(
            unread(read_with(&mut failed, budget())),
            "failed",
            "{attribute}: a failed getter of anything judged against refuses"
        );
    }

    let mut absent = Fake::app();
    absent.set(FIELD, "AXEnabled", Answer::Absent);
    let keyboard = read_with(&mut absent, budget()).unwrap();
    assert_eq!(
        keyboard.focused.unwrap().element.enabled,
        None,
        "an app that does not say is not taken to say enabled or not"
    );
}

#[test]
fn a_failed_read_keeps_which_attribute_failed_and_how_but_not_its_value() {
    let mut failed = Fake::app();
    failed.set(GROUP, "AXParent", Answer::Failed(AXError::CannotComplete));
    let error = read_with(&mut failed, budget()).unwrap_err();
    let asked = std::error::Error::source(&error)
        .and_then(|cause| cause.downcast_ref::<Asked>())
        .unwrap();
    assert_eq!(
        (asked.call, asked.status),
        ("AXParent", AXError::CannotComplete)
    );
    assert_eq!(
        asked.to_string(),
        "AXParent failed with Accessibility error -25204"
    );
    assert_eq!(unread(Err(error)), "slow");
}

/// An AppKit text view answers its description with a plain failure
/// every time: that is no name, not a refusal, since a name can only
/// make a field match.
#[test]
fn a_name_whose_getter_fails_outright_is_no_name() {
    let mut fake = Fake::app();
    fake.set(FIELD, "AXDescription", Answer::Failed(AXError::Failure));
    fake.set(
        FIELD,
        "AXIdentifier",
        Answer::Text("First Text View".to_owned()),
    );
    let focused = read_with(&mut fake, budget()).unwrap().focused.unwrap();
    assert_eq!(focused.element.description, None);
    assert_eq!(
        focused.element.identifier.as_deref(),
        Some("First Text View")
    );

    let mut stalled = Fake::app();
    stalled.set(
        FIELD,
        "AXDescription",
        Answer::Failed(AXError::CannotComplete),
    );
    assert_eq!(unread(read_with(&mut stalled, budget())), "slow");
}

#[test]
fn a_read_asks_for_what_is_judged_and_nothing_more() {
    let mut window_only = Fake::app();
    window_only.set(
        FIELD,
        "AXDescription",
        Answer::Failed(AXError::CannotComplete),
    );
    let deadline = window_only.started + budget();
    let keyboard = read(
        &mut window_only,
        &APP,
        deadline,
        Judged {
            window: true,
            field: false,
        },
    )
    .unwrap();
    assert!(keyboard.focused.is_none());
    assert_eq!(keyboard.within, Within::Window);
    assert!(
        !window_only.asked.contains(&(FIELD, "AXDescription")),
        "the field's names are not asked for"
    );

    let mut field_only = Fake::app();
    field_only.set(WINDOW, "AXTitle", Answer::Failed(AXError::CannotComplete));
    let deadline = field_only.started + budget();
    let keyboard = read(
        &mut field_only,
        &APP,
        deadline,
        Judged {
            window: false,
            field: true,
        },
    )
    .unwrap();
    assert!(keyboard.window.is_none());
    assert!(keyboard.focused.is_some());
    assert!(
        !field_only
            .asked
            .iter()
            .any(|(element, _)| *element == WINDOW),
        "the window is not asked about"
    );
}

#[test]
fn an_answer_of_the_wrong_kind_or_cut_short_refuses() {
    let mut wrong = Fake::app();
    wrong.set(FIELD, "AXEnabled", Answer::Text("yes".to_owned()));
    assert_eq!(unread(read_with(&mut wrong, budget())), "malformed");

    let mut half = Fake::app();
    half.set(WINDOW, "AXSize", Answer::Absent);
    assert_eq!(unread(read_with(&mut half, budget())), "malformed");

    let mut cut = Fake::app();
    cut.set(WINDOW, "AXTitle", Answer::Cut);
    assert_eq!(unread(read_with(&mut cut, budget())), "cut");
}

#[test]
fn a_sheet_between_the_field_and_its_window_is_found_on_the_way_up() {
    let mut fake = Fake::app();
    fake.set(GROUP, "AXParent", Answer::Element(SHEET));
    fake.set(SHEET, "AXRole", Answer::Text("AXSheet".to_owned()));
    fake.set(SHEET, "AXPosition", Answer::Point(80.0, 120.0));
    fake.set(SHEET, "AXSize", Answer::Size(500.0, 180.0));
    let keyboard = read_with(&mut fake, budget()).unwrap();
    assert_eq!(keyboard.within, Within::Window);
    let window = keyboard.window.unwrap();
    assert_eq!(window.role.as_deref(), Some("AXSheet"));
    assert_eq!(window.frame.unwrap().width, 500.0);

    let mut orphan = Fake::app();
    orphan.set(GROUP, "AXParent", Answer::Absent);
    assert_eq!(
        read_with(&mut orphan, budget()).unwrap().within,
        Within::Elsewhere
    );
}

#[test]
fn a_walk_longer_than_any_app_nests_is_refused_as_too_deep() {
    let mut fake = Fake::app();
    fake.set(GROUP, "AXParent", Answer::Element(GROUP));
    for answer in fake.answers.values_mut() {
        answer.1 = Duration::ZERO;
    }
    assert_eq!(unread(read_with(&mut fake, budget())), "deep");
}

#[test]
fn the_walk_stops_at_its_window_a_sheet_or_another_window() {
    assert_eq!(place(true, Some("AXSheet")), Some(Within::Window));
    assert_eq!(place(false, Some("AXSheet")), Some(Within::Sheet));
    for role in ["AXWindow", "AXApplication"] {
        assert_eq!(place(false, Some(role)), Some(Within::Elsewhere));
    }
    for role in [Some("AXGroup"), Some("AXScrollArea"), None] {
        assert_eq!(place(false, role), None);
    }
}

#[test]
fn a_reference_is_an_element_or_absent_and_nothing_else() {
    assert_eq!(reference(Answer::Element(7)).unwrap(), Some(7));
    assert_eq!(reference::<usize>(Answer::Absent).unwrap(), None);
    for answer in [
        Answer::Text("x".to_owned()),
        Answer::Flag(true),
        Answer::Other,
    ] {
        let error = reference::<usize>(answer).unwrap_err();
        assert_eq!(error.details["unread"], "malformed");
    }
}
#[test]
fn direct_app_focus_requires_both_frontmost_and_an_owned_focus_reference() {
    for attribute in ["AXFocusedWindow", "AXFocusedUIElement"] {
        let mut fake = Fake::app();
        fake.set(APP, "AXFrontmost", Answer::Flag(true));
        fake.set(APP, "AXFocusedWindow", Answer::Absent);
        fake.set(APP, "AXFocusedUIElement", Answer::Absent);
        fake.set(APP, attribute, Answer::Element(WINDOW));
        let deadline = fake.started + budget();
        assert!(
            app_focus(
                &mut Reader {
                    ask: &mut fake,
                    deadline
                },
                &APP,
                812
            )
            .unwrap()
        );
        fake.owners.insert(WINDOW, 999);
        assert!(
            app_focus(
                &mut Reader {
                    ask: &mut fake,
                    deadline
                },
                &APP,
                812
            )
            .is_err()
        );
    }
    for frontmost in [Answer::Absent, Answer::Flag(false), Answer::Flag(true)] {
        let mut fake = Fake::app();
        fake.set(APP, "AXFrontmost", frontmost);
        fake.set(APP, "AXFocusedWindow", Answer::Absent);
        fake.set(APP, "AXFocusedUIElement", Answer::Absent);
        let deadline = fake.started + budget();
        assert!(
            !app_focus(
                &mut Reader {
                    ask: &mut fake,
                    deadline
                },
                &APP,
                812
            )
            .unwrap()
        );
    }
    let mut fake = Fake::app();
    fake.set(APP, "AXFrontmost", Answer::Failed(AXError::CannotComplete));
    let deadline = fake.started + budget();
    assert!(
        app_focus(
            &mut Reader {
                ask: &mut fake,
                deadline
            },
            &APP,
            812
        )
        .is_err()
    );
}
#[test]
fn a_focused_element_proves_its_window_when_the_app_omits_the_window_attribute() {
    let mut fake = Fake::app();
    fake.set(APP, "AXFocusedWindow", Answer::Absent);
    let keyboard = read_with(&mut fake, budget()).unwrap();
    assert_eq!(keyboard.window.unwrap().role.as_deref(), Some("AXWindow"));
    assert_eq!(keyboard.within, Within::Window);
}
