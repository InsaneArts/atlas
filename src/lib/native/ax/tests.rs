//! Checks of the scan's bookkeeping, the bounds it keeps on text, and how
//! what it reads is placed and reported.
use super::{
    ABSENT, Element, Frame, INCOMPLETE, Limits, Stop,
    batch::{BATCH, Batch, Slot, Source, slot},
    elements, ffi,
    menus::{Step, judge, skip_nested_menu},
    scan::{SELECTORS, Scan},
    telling,
    text::{TEXT_LIMIT, TEXT_ROLES, bounded_string, prefix_range, prefix_text},
    tree::{sheet_window, unreachable},
    why,
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFNumber, CFRetained, CFString, CFType, Type,
};
use std::{
    collections::{BTreeSet, HashSet},
    time::{Duration, Instant},
};

/// What a sheet holds is judged against the sheet, not the window it
/// hangs from; anything else keeps its window.
#[test]
fn a_sheet_is_the_window_of_what_it_holds() {
    let window = Frame {
        x: 0.0,
        y: 0.0,
        width: 600.0,
        height: 400.0,
    };
    let sheet = Frame {
        x: 100.0,
        y: 50.0,
        width: 400.0,
        height: 300.0,
    };

    assert_eq!(
        sheet_window(Some("AXSheet"), Some(&sheet), Some(window.clone())),
        Some(sheet.clone())
    );
    assert_eq!(
        sheet_window(Some("AXButton"), Some(&sheet), Some(window.clone())),
        Some(window.clone())
    );
    assert_eq!(
        sheet_window(Some("AXSheet"), None, Some(window.clone())),
        Some(window)
    );
}

#[test]
fn frames_report_whole_point_centres() {
    let frame = Frame {
        x: 10.0,
        y: 20.0,
        width: 31.0,
        height: 10.0,
    };
    assert_eq!(frame.center(), (26.0, 25.0));
}

#[test]
fn a_hit_is_placed_by_the_frames_above_it() {
    let frame = |x: f64, width: f64| Frame {
        x,
        y: 20.0,
        width,
        height: 100.0,
    };
    let menu = frame(10.0, 200.0);
    let window = frame(300.0, 600.0);
    let read = [window.clone()];
    let step = |is_menu, at: Option<&Frame>, top| judge(is_menu, at, top, &menu, &read);

    assert_eq!(step(true, Some(&menu), false), Step::Found { menu: true });
    assert_eq!(
        step(false, Some(&menu), false),
        Step::Found { menu: false },
        "a popover or list with the window's frame"
    );
    assert_eq!(
        step(true, Some(&frame(50.0, 90.0)), false),
        Step::Up,
        "a submenu over its parent is passed"
    );
    assert_eq!(step(false, None, false), Step::Up);
    assert_eq!(step(false, Some(&window), true), Step::Through);
    assert_eq!(
        step(false, Some(&frame(900.0, 40.0)), true),
        Step::Stop,
        "the top of something not read"
    );
    assert_eq!(step(false, None, true), Step::Stop);
}

#[test]
fn only_closed_or_already_read_nested_menus_are_skipped() {
    let frame = |x: f64, width: f64| Frame {
        x,
        y: 20.0,
        width,
        height: 100.0,
    };
    let open = [frame(10.0, 200.0)];

    assert!(skip_nested_menu(None, &open), "no frame: closed");
    assert!(
        skip_nested_menu(Some(&frame(0.0, 0.0)), &open),
        "no size: closed"
    );
    assert!(
        skip_nested_menu(Some(&frame(10.5, 200.0)), &open),
        "read as its own"
    );
    assert!(
        !skip_nested_menu(Some(&frame(400.0, 200.0)), &open),
        "a menu a view draws is read"
    );
}

#[test]
fn frames_match_within_a_point() {
    let frame = Frame {
        x: 10.0,
        y: 20.0,
        width: 31.0,
        height: 10.0,
    };
    let mut near = frame.clone();
    near.x += 0.5;
    near.height -= 1.0;
    assert!(frame.matches(&near));

    near.width += 2.0;
    assert!(!frame.matches(&near));
}

/// The named AX errors are the raw codes the scan has always sorted on.
#[test]
fn ax_error_lists_keep_their_codes() {
    assert_eq!(INCOMPLETE.map(|e| e.0), [-25204, -25202, -25211]);
    assert_eq!(ABSENT.map(|e| e.0), [-25212, -25205, -25208]);
    assert_eq!(AXError::Failure.0, -25200);
}

/// Past the deadline nothing more is asked and the scan is incomplete.
#[test]
fn a_scan_closes_at_its_deadline() {
    let mut scan = Scan::new(Instant::now() + Duration::from_secs(60));
    assert!(scan.open() && !scan.incomplete);

    scan.incomplete = true;
    assert!(
        scan.open(),
        "an incomplete scan may still ask until its deadline"
    );

    scan.deadline = Instant::now();
    scan.incomplete = false;
    assert!(!scan.open() && scan.incomplete);
    assert_eq!(scan.stops, BTreeSet::from([Stop::Time]), "and says why");
}

#[test]
fn scan_stops_are_said_in_plain_words() {
    for stop in Stop::ALL {
        assert_eq!(Stop::parse(stop.word()), Some(stop));
        assert_eq!(
            serde_json::to_value(stop).unwrap(),
            serde_json::json!(stop.word())
        );
    }

    assert_eq!(why([Stop::Time]).as_deref(), Some("it ran out of time"));
    assert_eq!(
        why([Stop::Count, Stop::Unread]).as_deref(),
        Some("the app has more elements than Atlas reads and part of the app could not be read")
    );
    assert_eq!(
        why([Stop::Depth, Stop::Count, Stop::Time]).as_deref(),
        Some(
            "some elements are nested too deep, the app has more elements than Atlas reads \
             and it ran out of time"
        )
    );
    assert_eq!(Stop::parse("later"), None, "an unknown reason is not one");
    assert_eq!(why([]), None);
}

#[test]
fn only_failures_other_than_a_missing_attribute_are_telling() {
    for absent in ABSENT {
        assert!(!telling(&format!("AXTitle:{}", absent.0)));
    }
    for code in [AXError::Failure, AXError::CannotComplete] {
        assert!(telling(&format!("AXChildren:{}", code.0)));
    }
}

/// Bound `text` as a CFString would be.
fn bound(text: &str) -> (String, bool) {
    bounded_string(&CFString::from_str(text)).unwrap()
}

#[test]
fn strings_are_bounded_before_conversion() {
    let (text, truncated) = bound(&"x".repeat(5000));
    assert_eq!((text.len(), truncated), (TEXT_LIMIT as usize, true));
    assert_eq!(bound("short"), ("short".into(), false));

    let exact = "y".repeat(TEXT_LIMIT as usize);
    assert_eq!(
        bound(&exact),
        (exact.clone(), false),
        "at the limit is whole"
    );
}

#[test]
fn strings_are_cut_between_characters() {
    // A surrogate pair straddles unit 200: 199 units, then 2 for the emoji.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let (text, truncated) = bound(&format!("{}{family}tail", "a".repeat(199)));
    assert!(truncated);
    assert_eq!(text, "a".repeat(199), "the whole emoji sequence is dropped");

    // A combining mark at unit 200 stays with its base letter.
    let (text, _) = bound(&format!("{}e\u{301}tail", "a".repeat(199)));
    assert_eq!(text, "a".repeat(199));
    let (text, _) = bound(&format!("{}e\u{301}tail", "a".repeat(198)));
    assert_eq!(text, format!("{}e\u{301}", "a".repeat(198)));
}

/// Only a missing permission is called one: a busy app, or a process ID
/// with no app, is named as such.
#[test]
fn an_unreachable_app_is_reported_for_what_it_is() {
    let failed = |error, trusted, exists| {
        let error = unreachable(error, trusted, exists);
        (error.code, error.error)
    };

    assert_eq!(
        failed(Some(AXError::APIDisabled), true, true).0,
        "permission"
    );
    assert_eq!(
        failed(Some(AXError::CannotComplete), false, true).0,
        "permission"
    );

    let (code, message) = failed(Some(AXError::CannotComplete), true, true);
    assert_eq!(code, "elements");
    assert!(message.contains("not responding"));

    let (code, message) = failed(Some(AXError::InvalidUIElement), true, true);
    assert_eq!(code, "elements");
    assert!(message.contains("No app"));
    assert!(
        failed(Some(AXError::CannotComplete), true, false)
            .1
            .contains("No app")
    );

    let (code, message) = failed(Some(AXError::Failure), true, true);
    assert_eq!(code, "elements");
    assert!(message.contains("-25200"));
}

fn button() -> Element {
    Element {
        index: 4,
        depth: 2,
        window: None,
        role: Some("AXButton".into()),
        subrole: None,
        title: None,
        description: None,
        value: None,
        identifier: None,
        enabled: None,
        focused: None,
        selected: None,
        frame: None,
        unreadable: Vec::new(),
        truncated: Vec::new(),
        menu: false,
        mark: None,
    }
}

#[test]
fn an_element_leaves_out_what_the_scan_did_not_read() {
    assert_eq!(
        serde_json::to_value(button()).unwrap(),
        serde_json::json!({"index": 4, "depth": 2, "role": "AXButton"})
    );
}

#[test]
fn an_element_reads_back_without_its_absent_fields() {
    let element = button();
    let written = serde_json::to_string(&element).unwrap();
    assert_eq!(serde_json::from_str::<Element>(&written).unwrap(), element);

    let older = serde_json::json!({
        "index": 4,
        "depth": 2,
        "window": null,
        "role": "AXButton",
        "subrole": null,
        "title": null,
        "description": null,
        "value": null,
        "identifier": null,
        "enabled": null,
        "focused": null,
        "frame": null,
    });
    assert_eq!(serde_json::from_value::<Element>(older).unwrap(), element);

    let full = Element {
        window: Some(Frame {
            x: 0.0,
            y: 0.0,
            width: 600.0,
            height: 400.0,
        }),
        title: Some("Save".into()),
        enabled: Some(true),
        focused: Some(false),
        frame: Some(Frame {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 10.0,
        }),
        ..button()
    };
    let written = serde_json::to_string(&full).unwrap();
    assert_eq!(serde_json::from_str::<Element>(&written).unwrap(), full);
}

#[test]
fn an_absent_process_yields_no_elements() {
    // PID 0 is the kernel; it has no accessible windows. The call must
    // fail closed or return nothing, never crash.
    if let Ok(snapshot) = elements(0, Limits::default()) {
        assert!(snapshot.elements.is_empty());
    }
}

use super::ffi::made::{error as error_value, null, point, size};

fn string(text: &str) -> CFRetained<CFType> {
    CFString::from_str(text).into()
}

/// A batch read as an app would answer it: these attributes as given,
/// every other one with no value.
fn answered(answers: Vec<(&str, CFRetained<CFType>)>) -> Batch {
    let values: Vec<CFRetained<CFType>> = BATCH
        .iter()
        .map(|name| {
            answers
                .iter()
                .find(|(answered, _)| answered == name)
                .map_or_else(null, |(_, value)| value.clone())
        })
        .collect();
    Batch::from_values(&CFArray::from_retained_objects(&values)).unwrap()
}

fn open_scan() -> Scan {
    Scan::new(Instant::now() + Duration::from_secs(60))
}

/// What a scan has recorded, for comparing two scans.
fn recorded(scan: &Scan) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        scan.errors.clone(),
        scan.unreadable.clone(),
        scan.truncated.clone(),
        scan.incomplete,
        scan.stops.clone(),
        scan.last_error.map(|error| error.0),
    )
}

/// What one slot holds, in words.
fn held(slot: &Slot) -> String {
    match slot {
        Slot::Value(_) => "value".into(),
        Slot::Failed(status) => format!("failed {}", status.0),
        Slot::Empty => "empty".into(),
        Slot::Unread => "unread".into(),
    }
}

#[test]
fn an_error_value_reads_as_the_ax_error_it_holds() {
    assert_eq!(
        held(&slot(error_value(AXError::CannotComplete))),
        "failed -25204"
    );
    assert_eq!(held(&slot(error_value(AXError::NoValue))), "failed -25212");
    assert_eq!(
        held(&slot(error_value(AXError::Success))),
        "failed -25200",
        "an error that claims success is taken as a failure"
    );
    assert_eq!(held(&slot(null())), "empty");
    assert_eq!(held(&slot(string("Save"))), "value");
    assert_eq!(
        held(&slot(point(1.0, 2.0))),
        "value",
        "a point is not an error"
    );
}

#[test]
fn a_batched_read_keeps_the_bookkeeping_of_single_reads() {
    let batch = answered(vec![
        ("AXRole", string("AXTextField")),
        ("AXSubrole", error_value(AXError::AttributeUnsupported)),
        ("AXTitle", error_value(AXError::Failure)),
        ("AXDescription", error_value(AXError::IllegalArgument)),
        ("AXIdentifier", error_value(AXError::CannotComplete)),
        ("AXEnabled", CFBoolean::new(true).retain().into()),
        ("AXPosition", point(10.0, 20.0)),
        ("AXSize", size(30.0, 40.0)),
    ]);
    let from = Source::Batch(&batch);
    let mut scan = open_scan();

    assert_eq!(scan.text(from, "AXRole").as_deref(), Some("AXTextField"));
    assert_eq!(
        scan.frame(from),
        Some(Frame {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        })
    );
    assert_eq!(scan.text(from, "AXSubrole"), None);
    assert_eq!(scan.text(from, "AXTitle"), None);
    assert_eq!(scan.text(from, "AXDescription"), None);
    assert_eq!(scan.text(from, "AXIdentifier"), None);
    assert_eq!(scan.flag(from, "AXEnabled"), Some(true));
    assert_eq!(scan.flag(from, "AXFocused"), None);

    let mut single = open_scan();
    single.failed(AXError::AttributeUnsupported, "AXSubrole");
    single.failed(AXError::Failure, "AXTitle");
    single.failed(AXError::IllegalArgument, "AXDescription");
    single.failed(AXError::CannotComplete, "AXIdentifier");

    assert_eq!(recorded(&scan), recorded(&single));
    assert_eq!(scan.unreadable, ["AXDescription"]);
    assert!(scan.incomplete);
}

#[test]
fn a_failed_batch_fails_every_attribute_taken_from_it() {
    let batch = Batch::failed(AXError::CannotComplete);
    let from = Source::Batch(&batch);
    let mut scan = open_scan();
    scan.text(from, "AXRole");
    scan.frame(from);
    for name in ["AXSubrole", "AXTitle", "AXDescription", "AXIdentifier"] {
        scan.text(from, name);
    }
    for name in ["AXEnabled", "AXFocused"] {
        scan.flag(from, name);
    }

    let mut single = open_scan();
    for name in [
        "AXRole",
        "AXPosition",
        "AXSubrole",
        "AXTitle",
        "AXDescription",
        "AXIdentifier",
        "AXEnabled",
        "AXFocused",
    ] {
        single.failed(AXError::CannotComplete, name);
    }

    assert_eq!(recorded(&scan), recorded(&single));
}

#[test]
fn a_size_is_not_read_when_the_position_failed() {
    let batch = answered(vec![
        ("AXPosition", error_value(AXError::Failure)),
        ("AXSize", error_value(AXError::CannotComplete)),
    ]);
    let mut scan = open_scan();
    assert_eq!(scan.frame(Source::Batch(&batch)), None);

    let mut single = open_scan();
    single.failed(AXError::Failure, "AXPosition");
    assert_eq!(recorded(&scan), recorded(&single));
    assert!(!scan.incomplete);
}

#[test]
fn slots_the_walk_does_not_read_leave_no_trace() {
    let batch = answered(vec![
        ("AXRole", string("AXButton")),
        ("AXSelected", error_value(AXError::CannotComplete)),
        ("AXMenuItemMarkChar", error_value(AXError::Failure)),
        ("AXNumberOfCharacters", error_value(AXError::CannotComplete)),
        ("AXSize", error_value(AXError::CannotComplete)),
    ]);
    let from = Source::Batch(&batch);
    let mut scan = open_scan();
    scan.text(from, "AXRole");
    assert_eq!(scan.frame(from), None, "no position, so no size is read");
    assert_eq!(batch.characters(), None);

    assert_eq!(recorded(&scan), recorded(&open_scan()));
}

#[test]
fn a_closed_scan_sends_no_batch_and_takes_nothing() {
    let system = system_wide();
    let mut scan = open_scan();
    scan.deadline = Instant::now();

    let batch = scan.batch(&system).unwrap();
    assert!(batch.slots.iter().all(|slot| matches!(slot, Slot::Unread)));
    let from = Source::Batch(&batch);
    assert_eq!(scan.text(from, "AXRole"), None);
    assert_eq!(scan.frame(from), None);
    assert_eq!(scan.value(&system, Some(&batch), None, None), None);

    assert!(scan.errors.is_empty() && scan.unreadable.is_empty());
    assert_eq!(scan.stops, BTreeSet::from([Stop::Time]));
}

#[test]
fn a_batch_of_the_wrong_length_leaves_the_scan_incomplete() {
    let short = CFArray::from_retained_objects(&[string("AXButton"), null()]);
    for reply in [Some(&*short), None] {
        let mut scan = open_scan();
        let batch = scan.received(AXError::Success, reply).unwrap();
        assert!(batch.slots.iter().all(|slot| matches!(slot, Slot::Unread)));
        assert_eq!(scan.text(Source::Batch(&batch), "AXRole"), None);

        assert_eq!(
            scan.errors,
            std::collections::BTreeMap::from([("AXAttributes:-25200".to_owned(), 1)])
        );
        assert_eq!(scan.stops, BTreeSet::from([Stop::Unread]));
    }
}

#[test]
fn the_batch_asks_for_every_attribute_an_element_reports() {
    let asked: HashSet<&str> = BATCH.into_iter().collect();
    assert_eq!(asked.len(), BATCH.len(), "no attribute is asked twice");
    assert_eq!(open_scan().names.len(), BATCH.len());

    for selector in SELECTORS.into_iter().filter(|name| *name != "AXValue") {
        assert!(asked.contains(selector), "{selector}");
    }
    for name in [
        "AXSubrole",
        "AXEnabled",
        "AXFocused",
        "AXSelected",
        "AXPosition",
        "AXSize",
        "AXMenuItemMarkChar",
        "AXNumberOfCharacters",
    ] {
        assert!(asked.contains(name), "{name}");
    }
    assert!(
        !asked.contains("AXValue"),
        "a long value is never copied in a batch"
    );
}

fn system_wide() -> CFRetained<AXUIElement> {
    ffi::system_wide().unwrap()
}

fn application(pid: libc::pid_t) -> CFRetained<AXUIElement> {
    ffi::application(pid).unwrap()
}

#[test]
fn a_window_that_is_the_app_itself_is_withheld_not_listed() {
    let pid = libc::pid_t::try_from(std::process::id()).unwrap();
    let app = application(pid);
    let mut scan = open_scan();

    let shown = scan.shown(&app, vec![application(pid), system_wide()]);

    assert_eq!(shown.len(), 1);
    assert!(*shown[0] == *system_wide());
    assert!(scan.incomplete, "a withheld window might hold a match");
    assert_eq!(scan.stops, BTreeSet::from([Stop::Withheld]));
}

#[test]
fn windows_an_app_shows_leave_the_scan_complete() {
    let pid = libc::pid_t::try_from(std::process::id()).unwrap();
    let app = application(pid);
    let mut scan = open_scan();

    assert_eq!(scan.shown(&app, vec![system_wide()]).len(), 1);
    assert!(scan.shown(&app, Vec::new()).is_empty());
    assert!(!scan.incomplete && scan.stops.is_empty());
}

#[test]
fn an_element_that_refuses_the_batch_is_read_one_attribute_at_a_time() {
    for status in [
        AXError::NotImplemented,
        AXError::AttributeUnsupported,
        AXError::Failure,
        AXError::IllegalArgument,
    ] {
        let mut scan = open_scan();
        assert!(scan.received(status, None).is_none(), "{}", status.0);
        assert_eq!(
            scan.errors,
            std::collections::BTreeMap::from([(format!("AXAttributes:{}", status.0), 1)])
        );
        assert!(scan.unreadable.is_empty());
        assert!(!scan.incomplete);
    }

    let system = system_wide();
    assert!(matches!(
        Source::of(&system, None),
        Source::Element(element) if *element == *system
    ));
    let batch = Batch::unread();
    assert!(matches!(
        Source::of(&system, Some(&batch)),
        Source::Batch(_)
    ));
}

#[test]
fn a_batch_that_could_not_complete_stalls_its_element() {
    for status in INCOMPLETE {
        let mut scan = open_scan();
        let batch = scan.received(status, None).unwrap();
        assert!(batch.stalled);
        assert!(
            batch
                .slots
                .iter()
                .all(|slot| matches!(slot, Slot::Failed(failed) if *failed == status))
        );
        assert!(scan.errors.is_empty(), "nothing is recorded until taken");
    }
}

#[test]
fn an_element_whose_batch_stalled_is_not_asked_for_its_value() {
    let system = system_wide();
    let batch = Batch::failed(AXError::CannotComplete);
    let mut scan = open_scan();

    assert_eq!(
        scan.value(&system, Some(&batch), Some("AXTextArea"), None),
        None
    );
    assert!(scan.errors.is_empty(), "no request was sent");
    assert_eq!(scan.stops, BTreeSet::from([Stop::Unread]));

    let mut answered = open_scan();
    assert!(!answered.stalled(Some(&Batch::unread())));
    assert!(!answered.stalled(None));
    assert!(!answered.incomplete);
}

#[test]
fn only_a_long_text_role_that_is_surely_no_password_is_read_from_its_start() {
    let long = || CFNumber::new_isize(5000).into();
    let batch = answered(vec![("AXNumberOfCharacters", long())]);
    for role in TEXT_ROLES {
        assert_eq!(batch.long_text(Some(role), None), Some(5000), "{role}");
    }
    for role in [None, Some("AXGroup"), Some("AXWebArea"), Some("AXButton")] {
        assert_eq!(batch.long_text(role, None), None, "{role:?}");
    }
    assert_eq!(
        batch.long_text(Some("AXTextField"), Some("AXSecureTextField")),
        None
    );

    let unread_subrole = answered(vec![
        ("AXNumberOfCharacters", long()),
        ("AXSubrole", error_value(AXError::Failure)),
    ]);
    assert_eq!(unread_subrole.long_text(Some("AXTextField"), None), None);

    let no_subrole = answered(vec![
        ("AXNumberOfCharacters", long()),
        ("AXSubrole", error_value(AXError::AttributeUnsupported)),
    ]);
    assert_eq!(no_subrole.long_text(Some("AXTextField"), None), Some(5000));
    assert_eq!(
        batch.long_text(Some("AXTextField"), Some("AXSearchField")),
        Some(5000)
    );

    let short = answered(vec![(
        "AXNumberOfCharacters",
        CFNumber::new_isize(TEXT_LIMIT).into(),
    )]);
    assert_eq!(short.long_text(Some("AXTextArea"), None), None);
}

#[test]
fn a_long_text_prefix_is_cut_at_the_limit() {
    let prefix = string(&"x".repeat(232));
    assert_eq!(prefix_text(&prefix), Some("x".repeat(200)));
}

#[test]
fn a_prefix_no_longer_than_the_limit_reads_the_whole_value_instead() {
    assert_eq!(prefix_text(&string(&"y".repeat(200))), None);
    assert_eq!(prefix_text(&string("short")), None);
    let number: CFRetained<CFType> = CFNumber::new_i32(7).into();
    assert_eq!(prefix_text(&number), None);
}

/// The first units of `text` that a prefix read asks for.
fn asked_prefix(text: &str) -> CFRetained<CFType> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let range = prefix_range(units.len() as CFIndex);
    string(&String::from_utf16(&units[..range.length as usize]).unwrap())
}

#[test]
fn a_prefix_is_cut_between_characters() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let tail = "z".repeat(5000);
    for text in [
        format!("{}{family}{tail}", "a".repeat(199)),
        format!("{}e\u{301}{tail}", "a".repeat(199)),
        format!("{}e\u{301}{tail}", "a".repeat(198)),
    ] {
        let (whole, cut) = bound(&text);
        assert!(cut);
        assert_eq!(
            prefix_text(&asked_prefix(&text)),
            Some(whole),
            "the prefix is cut where the whole text is"
        );
    }
}

#[test]
fn the_prefix_range_covers_the_limit_and_a_little_more() {
    for (characters, length) in [(5_000_000, 232), (233, 232), (232, 232), (201, 201)] {
        let range = prefix_range(characters);
        assert_eq!((range.location, range.length), (0, length));
    }
}
