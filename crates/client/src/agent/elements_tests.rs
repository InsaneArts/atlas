use super::*;
use crate::agent::tests::{asked_for, element, named, shown, textedit_reads, window};
use atlas_core::{
    screen::select::NameMatch,
    session::{FOCUS_APP, LOOKS},
};
use std::collections::BTreeMap;

fn frame(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}

fn scanned(elements: Vec<Element>) -> Scanned {
    Scanned {
        look: None,
        elements,
        incomplete: false,
        coverage_told: true,
        reasons: Vec::new(),
        errors: BTreeMap::new(),
    }
}

/// A listing request for elements with `name`.
fn asking_for(name: &str) -> Asked {
    Asked {
        names: vec![name.to_owned()],
        ..Asked::every(50)
    }
}

/// An element of a browser's window at `depth`, with a frame and a window.
fn in_browser(index: usize, depth: usize, role: &str, title: &str) -> Element {
    Element {
        index,
        depth,
        window: Some(frame(0.0, 0.0, 800.0, 600.0)),
        frame: Some(frame(10.0, 10.0 * index as f64, 100.0, 20.0)),
        role: Some(role.into()),
        title: Some(title.into()),
        enabled: Some(true),
        ..Element::default()
    }
}

/// A browser window: its toolbar, a page with a form, and its menu bar.
fn browser() -> Vec<Element> {
    vec![
        in_browser(0, 0, "AXWindow", "Form - Browser"),
        in_browser(1, 1, "AXButton", "Reload"),
        in_browser(2, 1, "AXTextField", "Address and search bar"),
        in_browser(3, 1, "AXWebArea", "Form"),
        in_browser(4, 2, "AXTextField", "Project name"),
        in_browser(5, 3, "AXStaticText", "Project name"),
        in_browser(6, 2, "AXButton", "Submit"),
        in_browser(7, 1, "AXButton", "Bookmark this tab"),
        in_browser(8, 0, "AXMenuBarItem", "File"),
    ]
}

fn indexes(listed: &Value) -> Vec<u64> {
    listed["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|element| element["index"].as_u64().unwrap())
        .collect()
}

#[test]
fn several_names_list_every_element_with_any_of_them() {
    let scan = scanned(browser());
    let asked = Asked {
        names: vec!["project name".into(), "Submit".into()],
        ..Asked::every(50)
    };
    let listed = listing_in(1, &scan, &asked, None);

    assert_eq!(indexes(&listed), [4, 5, 6]);
    assert_eq!(listed["matching"], 3);
    assert!(listed.get("unmatched").is_none(), "{listed}");
    assert!(listed.get("candidates").is_none(), "{listed}");
}

#[test]
fn a_name_of_several_that_matches_nothing_is_said_with_what_it_may_have_meant() {
    let scan = scanned(browser());
    let asked = Asked {
        names: vec!["Submit".into(), "Reloa".into()],
        ..Asked::every(50)
    };
    let listed = listing_in(1, &scan, &asked, None);

    assert_eq!(indexes(&listed), [6]);
    assert_eq!(listed["unmatched"], json!(["Reloa"]));
    assert_eq!(listed["candidates"][0]["name"], "Reload");

    let alone = listing_in(1, &scan, &asking_for("Reloa"), None);
    assert!(
        alone.get("unmatched").is_none(),
        "one name says it by matching none"
    );
    assert_eq!(alone["candidates"][0]["name"], "Reload");
}

#[test]
fn a_page_holds_only_what_its_web_area_holds() {
    let page = scanned(browser()).in_page().unwrap();
    let listed = listing_in(1, &page, &Asked::every(50), None);

    assert_eq!(
        indexes(&listed),
        [4, 5, 6],
        "not the toolbar, tab or menu bar"
    );
    assert_eq!(listed["scanned"], 3);
}

#[test]
fn a_page_is_refused_where_the_scan_found_none() {
    let mut elements = browser();
    elements.retain(|element| element.role.as_deref() != Some("AXWebArea"));
    let error = scanned(elements.clone()).in_page().err().unwrap();
    assert_eq!(error.code, Code::Elements);
    assert_eq!(error.error, "No web page is open in that app's windows");

    let mut cut = scanned(elements);
    cut.incomplete = true;
    let error = cut.in_page().err().unwrap();
    assert!(error.error.contains("stopped before"), "{error}");
}

#[test]
fn a_listing_of_values_names_each_element_without_its_place() {
    let mut elements = browser();
    elements[4].value = Some("Atlas".into());
    elements[6].enabled = Some(false);
    let scan = scanned(elements);
    let asked = Asked {
        names: vec!["Project name".into(), "Submit".into()],
        form: Form::Values,
        ..Asked::every(50)
    };
    let listed = listing_in(1, &scan, &asked, None);

    assert_eq!(
        listed["elements"],
        json!([
            {"index": 4, "role": "AXTextField", "name": "Project name", "value": "Atlas"},
            {"index": 5, "role": "AXStaticText", "name": "Project name"},
            {"index": 6, "role": "AXButton", "name": "Submit", "disabled": true},
        ])
    );
    assert!(listed.get("windows").is_none(), "{listed}");
}

#[test]
fn a_page_of_an_app_that_withheld_its_windows_says_to_unlock_the_screen() {
    let mut withheld = scanned(Vec::new());
    withheld.incomplete = true;
    withheld.reasons = vec![Stop::Withheld];
    let error = withheld.in_page().err().unwrap();

    assert!(error.error.contains("withheld its windows"), "{error}");
    let said = json!(error);
    assert_eq!(said["next"], WITHHELD_NEXT);
    assert_eq!(said["scan"]["reasons"], json!(["withheld"]));
}

/// An app found by name or process ID is looked for in the helper the
/// command verified moments before: the lookup and the scan after it
/// verify the helper once between them, not once for each list read.
#[tokio::test]
async fn finding_an_app_verifies_the_helper_once() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    for target in [Target::Name("TextEdit".into()), Target::Pid(812)] {
        let before = helper.proofs();
        let listed = helper
            .serve(
                elements(&client, &target, &Asked::every(10)),
                textedit_reads,
            )
            .await
            .unwrap();
        assert_eq!(listed["pid"], 812, "{target:?}");
        assert_eq!(listed["look"], "8123abc", "{target:?}");
        assert_eq!(helper.proofs() - before, 1, "{target:?}");
    }
}

/// A look is asked of the input service in the session just verified, for
/// the very process found; one that cannot keep looks is asked nothing.
#[tokio::test]
async fn a_look_is_asked_of_an_input_service_that_keeps_looks() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[FOCUS_APP]);
    let error = helper
        .serve(
            elements(&client, &Target::Pid(812), &Asked::every(10)),
            |request| {
                assert_ne!(request["op"], "elements", "no look is asked for");
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Startup);
    assert!(error.error.contains("atlas start-input"), "{error}");

    for (reply, code) in [
        (
            json!({"ok": true, "elements": [], "truncated": false}),
            "elements",
        ),
        (
            json!({"ok": true, "look": "8123ABC", "elements": [], "truncated": false}),
            "elements",
        ),
        (
            json!({"ok": false, "code": "session-changed", "uncertain": true,
                "error": "The helper changed since Atlas checked it; look again"}),
            "session-changed",
        ),
        (
            json!({"ok": false, "code": "permission", "uncertain": false,
                "error": "The app's accessibility tree is unavailable; check Accessibility for the helper"}),
            "elements",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.serving_input(&client, &[LOOKS]);
        let error = helper
            .serve(
                elements(&client, &Target::Pid(812), &Asked::every(10)),
                |request| {
                    if request["op"] == "elements" {
                        assert_eq!(request["epoch"]["processId"], 456);
                        return Some(reply.clone());
                    }
                    textedit_reads(request)
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, code, "{reply}");
        assert!(!error.uncertain, "a read changes nothing");
        if let Some(said) = reply["error"].as_str() {
            assert_eq!(error.error, said, "the read's own words");
        }
    }
}

/// A daemon that cannot be told which session a read is for is not
/// trusted to be the one verified: nothing is read from it.
#[tokio::test]
async fn a_daemon_that_cannot_pin_a_read_is_refused() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.older();
    let target = Target::Name("TextEdit".into());
    let error = helper
        .serve(elements(&client, &target, &Asked::every(10)), |request| {
            panic!("nothing is read: {request}")
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Daemon);
    assert!(error.error.contains("run atlas restart"), "{error}");
}

/// An app found in one helper session is never captured in another:
/// its process ID means nothing there, so no capture is asked for.
#[tokio::test]
async fn an_app_is_captured_only_in_the_session_it_was_found_in() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.moving_after(1);
    let mut captures = 0;
    let error = helper
        .serve(
            screenshot(
                &client,
                Some(&Target::Name("TextEdit".into())),
                None,
                None,
                crate::client::Size::Standard,
                false,
            ),
            |request| {
                captures += usize::from(request["op"] == "screenshot");
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "session-changed", "{error}");
    assert_eq!(captures, 0, "no capture was asked for");
}

#[test]
fn element_filters_match_role_and_exact_name() {
    let save = element("AXButton", Some("Save"), None);
    let seven = element("AXButton", None, Some("7"));
    let label = element("AXStaticText", Some("Save"), None);
    let filter = |role: Option<&str>, name: Option<&str>| ElementFilter {
        role: role.map(Into::into),
        name: name.map(Into::into),
    };

    assert!(filter(Some("axbutton"), Some("save")).matches(&save));
    assert!(!filter(Some("AXButton"), Some("Save")).matches(&label));
    assert!(
        filter(None, Some("7")).matches(&seven),
        "description counts"
    );
    assert!(!filter(None, Some("Sav")).matches(&save), "names are exact");
    assert!(filter(None, None).matches(&label));

    let mut unread = element("AXButton", None, None);
    unread.unreadable = vec!["AXTitle".into()];
    assert!(!filter(Some("AXButton"), Some("Save")).matches(&unread));
    assert!(filter(Some("AXButton"), Some("Save")).might_match(&unread));
    assert!(!filter(Some("AXStaticText"), Some("Save")).might_match(&unread));
}

#[test]
fn a_label_is_named_by_its_text_but_a_field_is_not() {
    let filter = ElementFilter {
        role: None,
        name: Some("Main".into()),
    };
    let mut label = element("AXStaticText", None, None);
    label.value = Some("Main".into());
    assert!(filter.matches(&label) && filter.might_match(&label));

    let mut field = element("AXTextField", None, None);
    field.value = Some("Main".into());
    assert!(!filter.matches(&field), "typed text is not a name");
}

#[test]
fn a_named_click_reports_only_role_name_and_point() {
    let mut button = element("AXButton", Some("Save"), Some("Save the file"));
    button.identifier = Some("save".into());
    assert_eq!(
        clicked(&button, (25.0, 25.0)),
        json!({"at": [25, 25], "name": "Save", "role": "AXButton"})
    );

    let mut label = element("AXStaticText", Some(""), None);
    label.value = Some("Main".into());
    assert_eq!(
        clicked(&label, (3.0, 4.0)),
        json!({"at": [3, 4], "name": "Main", "role": "AXStaticText"})
    );

    let mut field = element("AXTextField", None, None);
    field.value = Some("typed".into());
    assert_eq!(
        clicked(&field, (3.0, 4.0)),
        json!({"at": [3, 4], "role": "AXTextField"})
    );
}

/// A label that joins several parts (SwiftUI's combined rows) is
/// named by its first part too, but a whole name is the better match.
#[test]
fn a_combined_label_is_named_by_its_first_part() {
    let filter = ElementFilter {
        role: None,
        name: Some("qa eject".into()),
    };
    let row = element("AXButton", None, Some("QA Eject, Option-Space, 6 Slices"));
    assert_eq!(filter.match_of(&row), Some(NameMatch::Leading));
    let whole = element("AXButton", Some("QA Eject"), None);
    assert_eq!(filter.match_of(&whole), Some(NameMatch::Exact));
    assert!(NameMatch::Exact > NameMatch::Leading);

    for other in [
        "QA Ejected, 6 Slices",
        "Eject, QA Eject",
        "QA Eject 6 Slices",
    ] {
        assert!(
            !filter.matches(&element("AXButton", None, Some(other))),
            "{other}"
        );
    }

    let mut field = element("AXTextField", None, None);
    field.value = Some("QA Eject, typed".into());
    assert!(!filter.matches(&field), "typed text is still not a name");

    let mut identified = element("AXButton", None, None);
    identified.identifier = Some("QA Eject, more".into());
    assert!(!filter.matches(&identified), "identifiers match whole");
}

#[test]
fn a_click_chooses_one_visible_element() {
    let whole = shown(0, "AXButton", Some("Delete"), None);
    let row = shown(1, "AXButton", None, Some("Delete, 3 files"));
    let both = [row.clone(), whole.clone()];
    let chosen = choose(&both, &named("delete")).unwrap();
    assert_eq!(chosen.index, 0, "a whole name wins over a first part");
    assert_eq!(
        choose(std::slice::from_ref(&row), &named("delete"))
            .unwrap()
            .index,
        1,
        "a first part alone is enough"
    );

    // A whole match without a frame does not hand the click to a row
    // named only in part.
    let mut hidden = whole.clone();
    hidden.frame = None;
    let error = choose(&[hidden, row.clone()], &named("delete")).unwrap_err();
    assert!(
        error.error.contains("No visible element"),
        "{}",
        error.error
    );

    let other = shown(2, "AXButton", None, Some("Delete, 9 files"));
    let error = choose(&[row.clone(), other], &named("delete")).unwrap_err();
    assert!(
        error.error.starts_with("2 elements match"),
        "{}",
        error.error
    );
    assert_eq!(error.details["candidates"].as_array().unwrap().len(), 2);

    let mut unread = shown(3, "AXButton", None, None);
    unread.unreadable = vec!["AXTitle".into()];
    let error = choose(&[whole.clone(), unread.clone()], &named("delete")).unwrap_err();
    assert!(error.error.contains("could not be read"), "{}", error.error);
    assert!(error.details.is_empty(), "a doubtful match lists none");
}

#[test]
fn disabled_controls_refuse_without_guessing_native_panel_identity() {
    for name in ["Save", "Open", "Export"] {
        let mut element = shown(0, "AXButton", Some(name), None);
        element.enabled = Some(false);
        let error = choose(&[element], &named(name)).unwrap_err();
        assert_eq!(
            json!(error),
            json!({
                "ok":false,"code":"input","uncertain":false,
                "error":"The selected control is disabled; no click was sent",
                "next":"Choose an enabled control. If this is Save/Open, cancel with Escape and use atlas run to create files or atlas open to open them; do not retry an inert confirmation."
            })
        );
    }
}

#[test]
fn enabled_and_unknown_controls_including_save_and_sheets_remain_selectable() {
    for role in ["AXButton", "AXSheet", "AXMenuItem"] {
        for enabled in [Some(true), None] {
            let mut element = shown(0, role, Some("Save"), None);
            element.enabled = enabled;
            assert_eq!(
                choose(std::slice::from_ref(&element), &named("Save")).unwrap(),
                &element
            );
        }
    }
}

#[test]
fn a_disabled_match_does_not_hide_ambiguity_or_unreadable_names() {
    let mut disabled = shown(0, "AXButton", Some("Save"), None);
    disabled.enabled = Some(false);
    let other = shown(1, "AXButton", Some("Save"), None);
    let error = choose(&[disabled.clone(), other], &named("Save")).unwrap_err();
    assert_eq!(error.code, Code::Elements);
    let mut unread = shown(1, "AXButton", None, None);
    unread.unreadable.push("AXTitle".into());
    let error = choose(&[disabled, unread], &named("Save")).unwrap_err();
    assert_eq!(error.code, Code::Elements);
}

#[tokio::test]
async fn a_disabled_named_control_refuses_before_any_input_or_native_preparation() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut reads = 0;
    let error = helper
        .serve(
            click_element(
                &client,
                &Target::Pid(i64::from(std::process::id())),
                &named("Save"),
                Button::Left,
                1,
                &[],
            ),
            |request| {
                if request["op"] == "apps" {
                    assert_eq!(reads, 0, "no app lookup after scanning");
                    return Some(json!({"ok": true, "apps": [{"pid": std::process::id(), "name": "TextEdit"}],
                        "starts": {std::process::id().to_string(): [123, 456]}}));
                }
                assert_eq!(
                    request["op"], "run",
                    "no input or other mutation: {request}"
                );
                assert_eq!(request["argv"][1], "elements-info", "no native preparation");
                reads += 1;
                let mut element = shown(0, "AXButton", Some("Save"), None);
                element.enabled = Some(false);
                Some(completed(
                    &json!({"ok":true,"elements":[element],"truncated":false}).to_string(),
                ))
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        json!(error),
        json!({
            "ok":false,"code":"input","uncertain":false,
            "error":"The selected control is disabled; no click was sent",
            "next":"Choose an enabled control. If this is Save/Open, cancel with Escape and use atlas run to create files or atlas open to open them; do not retry an inert confirmation."
        })
    );
    assert_eq!(reads, 1);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire_beside_commands().is_ok());
}

#[tokio::test]
async fn a_named_click_keeps_an_unknown_mutation_pending_instead_of_scanning_or_posting() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let abandoned = client.controller.quarantine().unwrap();
    let generation = abandoned.to_string();
    drop(abandoned);
    let error = click_element(
        &client,
        &Target::Pid(812),
        &named("Save"),
        Button::Left,
        1,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert_eq!(client.controller.pending().unwrap(), vec![generation]);
}

#[test]
fn the_default_listing_names_each_element_by_its_click_point_and_size() {
    let window = frame(10.2, 38.0, 800.0, 600.0);
    let mut bold = shown(3, "AXCheckBox", Some("Bold"), None);
    bold.frame = Some(frame(388.0, 51.0, 24.0, 22.4));
    bold.window = Some(window.clone());
    bold.value = Some("0".into());
    bold.depth = 4;
    bold.identifier = Some("bold".into());
    bold.description = Some("Bold text".into());

    let mut text = shown(17, "AXTextArea", None, None);
    text.frame = Some(frame(20.0, 50.0, 780.0, 560.0));
    text.window = Some(frame(10.0, 38.5, 800.0, 600.0));
    text.value = Some("Hello".into());

    let mut label = shown(18, "AXStaticText", None, None);
    label.value = Some("Status".into());
    label.window = Some(frame(900.0, 0.0, 300.0, 200.0));

    let mut bare = element("AXButton", None, None);
    bare.index = 19;

    let listed = listing(
        812,
        &scanned(vec![bold, text, label, bare]),
        &ElementFilter::default(),
        50,
        false,
    );
    assert_eq!(
        listed,
        json!({
            "ok": true, "pid": 812, "scanned": 4, "scanIncomplete": false,
            "matching": 4, "more": 0,
            "elements": [
                {"index": 3, "role": "AXCheckBox", "name": "Bold", "value": "0",
                 "at": [400, 62], "size": [24, 22], "window": 0},
                {"index": 17, "role": "AXTextArea", "value": "Hello",
                 "at": [410, 330], "size": [780, 560], "window": 0},
                {"index": 18, "role": "AXStaticText", "name": "Status",
                 "at": [20, 20], "size": [20, 20], "window": 1},
                {"index": 19, "role": "AXButton"},
            ],
            "windows": [
                {"x": 10, "y": 38, "width": 800, "height": 600},
                {"x": 900, "y": 0, "width": 300, "height": 200},
            ],
        })
    );
}

/// A kept look adds its name to the listing and nothing to its elements:
/// a reference is that name and an element's index.
#[test]
fn a_kept_look_adds_only_its_name_to_the_listing() {
    let elements = vec![
        shown(3, "AXButton", Some("Save"), None),
        shown(12, "AXButton", Some("Open"), None),
    ];
    let mut scan = scanned(elements.clone());
    scan.look = Look::parse("8123abc");
    for full in [false, true] {
        let mut kept = listing(812, &scan, &ElementFilter::default(), 50, full);
        let plain = scanned(elements.clone());
        let unkept = listing(812, &plain, &ElementFilter::default(), 50, full);
        assert_eq!(kept["look"], "8123abc");
        assert_eq!(kept["elements"][1]["index"], 12);
        kept.as_object_mut().unwrap().remove("look");
        assert_eq!(kept, unkept, "full: {full}");
    }
}

/// A reference that is not one a listing gives is refused before control
/// is taken or the helper is asked anything.
#[tokio::test]
async fn a_malformed_reference_is_refused_before_anything_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    for reference in ["", "8123abc", "8123abc.012", "8123ABC.1", "save"] {
        let error = click_reference(&client, None, reference, Button::Left, 1, &[])
            .await
            .unwrap_err();
        assert_eq!(
            json!(error),
            json!({"ok": false, "code": "elements", "uncertain": false,
                "error": "That is not a ref from atlas elements",
                "next": "Run atlas elements again.", "performed": 0}),
            "{reference:?}"
        );
    }
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire_beside_commands().is_ok());
}

/// A referenced click is sent as one referenced action for the input
/// service to find again, with the app's expectation only when one is named.
#[tokio::test]
async fn a_referenced_click_asks_the_input_service_to_find_its_element_again() {
    use crate::test_support::Helper;
    use atlas_core::{
        session::{FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW},
        test_support::UID,
    };

    for target in [None, Some(Target::Pid(812))] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.serving_input(
            &client,
            &[LOOKS, FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW, FOCUS_FIELD],
        );
        let mut sent = Vec::new();
        let reply = helper
            .serve(
                click_reference(&client, target.as_ref(), "8123abc.12", Button::Left, 2, &[]),
                |request| match request["op"].as_str() {
                    Some("acquire" | "release") => Some(json!({"ok": true})),
                    Some("input") => {
                        sent.push(request.clone());
                        let refused = json!({"ok": false, "code": "elements",
                            "error": "That look is no longer kept", "uncertain": false,
                            "next": "Run atlas elements again.", "performed": 0,
                            "partial": false});
                        Some(refused)
                    }
                    _ => textedit_reads(request),
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["code"], "elements");
        assert_eq!(reply["next"], "Run atlas elements again.");
        assert_eq!(reply["performed"], 0);
        let [request] = sent.as_slice() else {
            panic!("one input request: {sent:?}");
        };
        assert_eq!(
            request["actions"],
            json!([{"type": "reference", "ref": "8123abc.12", "button": "left", "count": 2,
                "modifiers": []}])
        );
        let expected = target
            .as_ref()
            .map_or(Value::Null, |_| json!({"pid": 812, "start": [123, 456]}));
        assert_eq!(
            request.get("expect").cloned().unwrap_or(Value::Null),
            expected
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

/// An action on an element goes to the input service as the one action
/// it is, and its reply says only how many were performed, how, and how
/// it reads now. A service that cannot act on elements is never sent one.
#[tokio::test]
async fn an_element_action_is_sent_whole_and_answered_briefly() {
    use crate::test_support::Helper;
    use atlas_core::{session::ELEMENT_ACTIONS, test_support::UID};

    let action = json!({"type": "set-value", "ref": "8123abc.12", "value": "draft"});
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS, ELEMENT_ACTIONS]);
    let mut sent = Vec::new();
    let reply = helper
        .serve(
            super::element(&client, None, &action),
            |request| match request["op"].as_str() {
                Some("acquire" | "release") => Some(json!({"ok": true})),
                Some("input") => {
                    sent.push(request.clone());
                    Some(json!({"ok": true, "performed": 1, "app": "TextEdit",
                        "bundleId": "com.apple.TextEdit", "mechanism": "accessibility",
                        "verification": "mismatch"}))
                }
                _ => textedit_reads(request),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        reply,
        json!({"ok": true, "performed": 1, "mechanism": "accessibility",
            "verification": "mismatch"})
    );
    let [request] = sent.as_slice() else {
        panic!("one input request: {sent:?}");
    };
    assert_eq!(request["actions"], json!([action]));
    assert!(client.controller.pending().unwrap().is_empty());

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    let error = helper
        .serve(super::element(&client, None, &action), |request| {
            assert_ne!(request["op"], "input", "nothing is sent");
            match request["op"].as_str() {
                Some("acquire" | "release") => Some(json!({"ok": true})),
                _ => textedit_reads(request),
            }
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Startup);
    assert!(!error.uncertain);
}

/// A pick's option must be a ref too, and an action that sends events is
/// no action on an element: both are refused before anything is asked.
#[tokio::test]
async fn a_malformed_option_or_a_physical_action_is_refused_before_anything_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let action = json!({"type": "pick", "ref": "8123abc.12", "option": "Large"});
    let error = super::element(&client, None, &action).await.unwrap_err();
    assert_eq!(error.code, Code::Elements);
    assert_eq!(error.details["performed"], 0);

    for physical in [
        json!({"type": "key", "key": "cmd+q"}),
        json!({"type": "reference", "ref": "8123abc.12"}),
    ] {
        let error = super::element(&client, None, &physical).await.unwrap_err();
        assert_eq!(error.code, Code::Input, "{physical}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn the_default_listing_omits_only_groups_without_information() {
    let bare = shown(0, "AXGroup", None, None);
    let mut groups = vec![bare.clone()];
    let mut empty = bare.clone();
    empty.index = 1;
    empty.title = Some(String::new());
    empty.value = Some(String::new());
    groups.push(empty);
    for index in 2..13 {
        let mut group = bare.clone();
        group.index = index;
        match index {
            2 => group.title = Some("Section".into()),
            3 => group.description = Some("Settings".into()),
            4 => group.identifier = Some("panel".into()),
            5 => group.value = Some("0".into()),
            6 => group.focused = Some(true),
            7 => group.selected = Some(true),
            8 => group.unreadable = vec!["AXTitle".into()],
            9 => group.truncated = vec!["AXDescription".into()],
            10 => group.enabled = Some(false),
            11 => group.menu = true,
            _ => group.mark = Some("✓".into()),
        }
        groups.push(group);
    }
    groups.push(shown(13, "AXButton", None, None));
    let scan = scanned(groups);
    let all = ElementFilter::default();
    let listed = listing(1, &scan, &all, 50, false);
    let indices: Vec<_> = listed["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|element| element["index"].as_u64().unwrap())
        .collect();
    assert_eq!(indices, (2..14).collect::<Vec<_>>());
    assert_eq!(listed["scanned"], 14);
    assert_eq!(listed["matching"], 12);
    let short = listing(1, &scan, &all, 2, false);
    assert_eq!(short["more"], 10);
    assert_eq!(
        short["elements"],
        json!(&listed["elements"].as_array().unwrap()[..2])
    );
    assert_eq!(
        listing(1, &scan, &all, 50, true)["elements"],
        json!(scan.elements)
    );
    assert_eq!(
        listing(
            1,
            &scan,
            &ElementFilter {
                role: Some("AXGroup".into()),
                name: None
            },
            50,
            false
        )["elements"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert_eq!(
        listing(1, &scan, &named("panel"), 50, false)["elements"][0]["index"],
        4
    );
}

#[test]
fn empty_groups_do_not_use_the_limit_or_hide_later_controls() {
    let scan = scanned(vec![
        shown(0, "AXGroup", None, None),
        shown(1, "AXButton", Some("Save"), None),
        shown(2, "AXGroup", None, None),
        shown(3, "AXButton", Some("Cancel"), None),
        shown(4, "AXGroup", None, None),
    ]);
    for (max, indices, more) in [(0, vec![], 2), (1, vec![1], 1), (2, vec![1, 3], 0)] {
        let listed = listing(1, &scan, &ElementFilter::default(), max, false);
        assert_eq!(listed["matching"], 2);
        assert_eq!(listed["more"], more);
        assert_eq!(
            listed["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|element| element["index"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            indices
        );
    }
}

#[test]
fn flags_are_listed_only_when_they_are_set() {
    let mut off = shown(0, "AXButton", Some("Save"), None);
    off.enabled = Some(false);
    off.focused = Some(false);
    off.selected = Some(false);
    off.value = Some(String::new());

    let mut on = shown(1, "AXMenuItem", Some("Ruler"), None);
    on.enabled = Some(true);
    on.focused = Some(true);
    on.selected = Some(true);
    on.menu = true;
    on.mark = Some("✓".into());
    on.truncated = vec!["AXTitle".into()];
    on.unreadable = vec!["AXDescription".into()];

    let listed = listing(
        1,
        &scanned(vec![off, on]),
        &ElementFilter::default(),
        50,
        false,
    );
    let elements = listed["elements"].as_array().unwrap();
    assert_eq!(elements[0]["disabled"], true);
    for absent in [
        "focused",
        "selected",
        "menu",
        "mark",
        "value",
        "truncated",
        "unreadable",
    ] {
        assert!(elements[0].get(absent).is_none(), "{absent}");
    }

    assert!(elements[1].get("disabled").is_none());
    assert_eq!(elements[1]["focused"], true);
    assert_eq!(elements[1]["selected"], true);
    assert_eq!(elements[1]["menu"], true);
    assert_eq!(elements[1]["mark"], "✓");
    assert_eq!(elements[1]["truncated"], json!(["AXTitle"]));
    assert_eq!(elements[1]["unreadable"], json!(["AXDescription"]));
    for dropped in [
        "depth",
        "subrole",
        "title",
        "description",
        "identifier",
        "frame",
    ] {
        assert!(elements[1].get(dropped).is_none(), "{dropped}");
    }
}

#[test]
fn elements_listed_in_full_are_as_the_scan_read_them() {
    let mut save = shown(0, "AXButton", Some("Save"), None);
    save.window = Some(frame(0.0, 0.0, 600.0, 400.0));
    save.identifier = Some("save".into());
    let scan = scanned(vec![save.clone(), shown(1, "AXButton", Some("Open"), None)]);

    let listed = listing(1, &scan, &named("save"), 50, true);
    assert_eq!(listed["elements"], json!([save]));
    assert!(listed.get("windows").is_none());
    assert_eq!(listed["matching"], 1);
    assert_eq!(listed["more"], 0);
}

#[test]
fn a_short_listing_says_how_many_more_matched() {
    let five: Vec<Element> = (0..5)
        .map(|index| shown(index, "AXButton", Some("Go"), None))
        .collect();
    let scan = scanned(five);
    let all = ElementFilter::default();

    for (max, shown, more) in [(2, 2, 3), (0, 0, 5), (50, 5, 0)] {
        for full in [false, true] {
            let listed = listing(1, &scan, &all, max, full);
            assert_eq!(listed["elements"].as_array().unwrap().len(), shown);
            assert_eq!(listed["matching"], 5);
            assert_eq!(listed["more"], more, "--max {max}");
            assert_eq!(listed["scanIncomplete"], false);
        }
    }
}

#[test]
fn a_scan_says_why_it_stopped_early() {
    let probe = |reasons: Value, errors: Value| {
        let mut reply = json!({"ok": true, "elements": [], "truncated": true, "errors": errors});
        if !reasons.is_null() {
            reply["reasons"] = reasons;
        }
        Scanned::from_reply(reply).unwrap()
    };

    let every = probe(
        json!(["depth", "count", "time", "unread", "withheld", "later"]),
        json!({}),
    );
    assert_eq!(
        every.reasons,
        [
            Stop::Depth,
            Stop::Count,
            Stop::Time,
            Stop::Unread,
            Stop::Withheld
        ]
    );
    assert!(every.incomplete);
    assert_eq!(
        every.diagnostics(),
        Some(json!({"reasons": ["depth", "count", "time", "unread", "withheld"]}))
    );

    let older = probe(Value::Null, json!({}));
    assert!(older.reasons.is_empty() && older.incomplete);
    assert_eq!(older.diagnostics(), None, "an older probe gives none");

    let mut errors = serde_json::Map::new();
    errors.insert("AXTitle:-25212".into(), json!(40));
    errors.insert("AXValue:-25205".into(), json!(40));
    errors.insert(format!("{}:-25200", "A".repeat(70)), json!(50));
    for attribute in 0..10 {
        errors.insert(
            format!("AXChildren{attribute}:-25204"),
            json!(attribute + 1),
        );
    }
    let failed = probe(json!(["unread"]), Value::Object(errors));
    let kept: Vec<(&str, usize)> = failed
        .errors
        .iter()
        .map(|(key, count)| (key.as_str(), *count))
        .collect();
    assert_eq!(kept.len(), 8, "at most eight");
    assert!(
        kept.iter().all(|(key, _)| key.starts_with("AXChildren")),
        "{kept:?}"
    );
    assert_eq!(
        kept.iter().map(|(_, count)| count).min(),
        Some(&3),
        "the most often"
    );
    assert_eq!(
        listing(1, &failed, &ElementFilter::default(), 50, false)["scan"]["reasons"],
        json!(["unread"])
    );

    let quiet = listing(
        1,
        &scanned(Vec::new()),
        &ElementFilter::default(),
        50,
        false,
    );
    assert!(quiet.get("scan").is_none());

    let mut complete = failed;
    complete.incomplete = false;
    let listed = listing(1, &complete, &ElementFilter::default(), 50, false);
    assert!(
        listed.get("scan").is_none(),
        "a complete scan has nothing to say"
    );
}

#[test]
fn a_refused_click_on_an_incomplete_scan_points_to_its_diagnostics() {
    let mut scan = scanned(Vec::new());
    scan.incomplete = true;
    scan.reasons = vec![Stop::Time, Stop::Unread];
    scan.errors = BTreeMap::from([("AXChildren:-25204".to_owned(), 2)]);

    let error = scan.refusal();
    assert_eq!(error.code, "elements");
    assert_eq!(
        error.error,
        "The accessibility scan was incomplete (it ran out of time and part of the app could \
         not be read), so a unique match cannot be established"
    );
    assert_eq!(
        error.details["scan"],
        json!({"reasons": ["time", "unread"], "errors": {"AXChildren:-25204": 2}})
    );
    assert!(
        error.details["next"]
            .as_str()
            .unwrap()
            .starts_with("Try again.")
    );

    scan.reasons.clear();
    scan.errors.clear();
    let error = scan.refusal();
    assert_eq!(
        error.error,
        "The accessibility scan was incomplete, so a unique match cannot be established"
    );
    assert!(error.details.get("scan").is_none());
}

#[test]
fn an_app_that_withheld_its_windows_is_reported_never_listed_as_empty() {
    let probe = json!({
        "ok": true,
        "elements": [{"index": 0, "depth": 0, "role": "AXMenuBarItem", "title": "Edit"}],
        "truncated": true,
        "reasons": ["withheld"],
        "errors": {},
    });
    let scan = Scanned::from_reply(probe).unwrap();

    let listed = listing(1, &scan, &ElementFilter::default(), 50, false);
    assert_eq!(listed["scanIncomplete"], true);
    assert_eq!(listed["scan"]["reasons"], json!(["withheld"]));
    assert_eq!(listed["next"], WITHHELD_NEXT);

    let error = scan.refusal();
    assert!(
        error.error.contains("the app withheld its windows"),
        "{}",
        error.error
    );
    assert_eq!(error.details["next"], WITHHELD_NEXT);

    let mut complete = scanned(Vec::new());
    complete.incomplete = true;
    complete.reasons = vec![Stop::Unread];
    let listed = listing(1, &complete, &ElementFilter::default(), 50, false);
    assert!(listed.get("next").is_none(), "only a withheld scan says so");
}

/// The names a refused click lists, in order.
fn candidate_names(error: &Error) -> Vec<&str> {
    error.details["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|candidate| candidate["name"].as_str())
        .collect()
}

#[test]
fn a_name_that_matches_nothing_lists_the_closest_names() {
    let at = |index, x: f64, role: &str, title: &str| {
        let mut element = shown(index, role, Some(title), None);
        element.frame = Some(frame(x, 0.0, 40.0, 22.0));
        element
    };
    let bar = [
        at(0, 10.0, "AXMenuBarItem", "Edit"),
        at(1, 50.0, "AXMenuBarItem", "Format"),
        at(2, 90.0, "AXMenuBarItem", "Window"),
        at(3, 130.0, "AXMenuBarItem", "Zoom"),
    ];
    let error = choose(&bar, &named("Formt")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Format"],
        "unrelated names are left out"
    );
    assert!(
        error
            .error
            .ends_with(" The closest names are in candidates.")
    );
    assert_eq!(
        error.details["candidates"][0],
        json!({"role": "AXMenuBarItem", "name": "Format", "at": [70, 11]})
    );

    let error = choose(&bar, &named("Nothing like it")).unwrap_err();
    assert!(error.details.get("candidates").is_none());
    assert!(error.error.ends_with("what is there."), "{}", error.error);

    let close = [
        at(0, 10.0, "AXButton", "Sane"),
        at(1, 50.0, "AXButton", "Save As…"),
        at(2, 90.0, "AXButton", "Sav"),
        at(3, 130.0, "AXButton", "Cave"),
        at(4, 170.0, "AXButton", "Sage"),
        at(5, 210.0, "AXButton", "Wave"),
        at(6, 250.0, "AXButton", "Have"),
    ];
    let error = choose(&close, &named("save")).unwrap_err();
    let names = candidate_names(&error);
    assert_eq!(names.len(), 5, "at most five: {names:?}");
    assert_eq!(
        names[..2],
        ["Sav", "Save As…"],
        "a part, nearest first, before a few letters off"
    );

    let error = choose(&[at(0, 10.0, "AXButton", "Sofe")], &named("save")).unwrap_err();
    assert!(
        error.details.get("candidates").is_none(),
        "two letters off is too far for a short name"
    );

    let dialog = [
        at(0, 10.0, "AXButton", "No"),
        at(1, 50.0, "AXButton", "Go"),
        at(2, 90.0, "AXButton", "Okay"),
    ];
    let error = choose(&dialog, &named("OK")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Okay"],
        "a short name is not matched a letter or two off"
    );

    let mut hidden = at(7, 0.0, "AXButton", "Safe");
    hidden.frame = None;
    let error = choose(&[hidden], &named("save")).unwrap_err();
    assert!(
        error.details.get("candidates").is_none(),
        "unframed is skipped"
    );

    let window = at(0, 10.0, "AXButton", "Copied");
    let mut item = at(1, 50.0, "AXMenuItem", "Copy Style");
    item.menu = true;
    let error = choose(&[window, item], &named("Copy")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Copy Style"],
        "only the open menu"
    );

    let by_role = ElementFilter {
        role: Some("AXButton".into()),
        name: Some("format".into()),
    };
    let mut button = at(4, 170.0, "AXButton", "Formats");
    button.index = 0;
    let error = choose(&[button, bar[1].clone()], &by_role).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Format", "Formats"],
        "the exact name under another role first"
    );

    let listed = listing(1, &scanned(bar.to_vec()), &named("Formt"), 50, false);
    assert_eq!(listed["matching"], 0);
    assert_eq!(listed["candidates"][0]["name"], "Format");
    let listed = listing(1, &scanned(bar.to_vec()), &named("Format"), 50, false);
    assert!(listed.get("candidates").is_none());
}

#[test]
fn listed_and_candidate_points_aim_at_a_checkbox_box() {
    let detectors = |index| {
        let mut element = shown(index, "AXCheckBox", Some("Data detectors"), None);
        element.frame = Some(frame(100.0, 200.0 + 30.0 * index as f64, 237.0, 18.0));
        element
    };

    let listed = listing(
        1,
        &scanned(vec![detectors(0)]),
        &named("Data detectors"),
        50,
        false,
    );
    assert_eq!(listed["elements"][0]["at"], json!([109, 209]));

    let error = choose(&[detectors(0), detectors(1)], &named("Data detectors")).unwrap_err();
    assert_eq!(
        error.details["candidates"],
        json!([
            {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 209]},
            {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 239]},
        ])
    );
}

#[test]
fn a_name_that_matches_several_lists_each_match() {
    let close = |index| {
        let mut element = shown(index, "AXButton", None, Some("close button"));
        element.frame = Some(frame(10.0 + 100.0 * index as f64, 0.0, 14.0, 14.0));
        element
    };

    let error = choose(&[close(0), close(1)], &named("close button")).unwrap_err();
    assert!(
        error.error.starts_with("2 elements match"),
        "{}",
        error.error
    );
    assert_eq!(
        error.details["candidates"],
        json!([
            {"role": "AXButton", "name": "close button", "at": [17, 7]},
            {"role": "AXButton", "name": "close button", "at": [117, 7]},
        ])
    );

    let seven: Vec<Element> = (0..7).map(close).collect();
    let error = choose(&seven, &named("close button")).unwrap_err();
    assert_eq!(error.details["candidates"].as_array().unwrap().len(), 5);
    assert_eq!(error.details["candidates"][4]["at"], json!([417, 7]));
}

/// Names fold case beyond ASCII; an attribute shown only in part is never
/// an exact match but may hide one; a collapsed element is not on
/// screen; a menu bar title gives way to a window element of its name.
#[test]
fn matching_is_honest_about_what_it_cannot_see() {
    let editer = shown(0, "AXButton", Some("Éditer"), None);
    assert_eq!(
        choose(std::slice::from_ref(&editer), &named("ÉDITER"))
            .unwrap()
            .index,
        0
    );

    let mut cut = shown(1, "AXButton", Some("Delete"), None);
    cut.truncated = vec!["AXTitle".into()];
    assert!(!named("delete").matches(&cut), "a cut title proves nothing");
    assert!(named("delete").might_match(&cut), "but it may hide a match");

    let whole = shown(2, "AXButton", Some("Delete"), None);
    let error = choose(&[whole.clone(), cut], &named("delete")).unwrap_err();
    assert!(error.error.contains("could not be read"), "{}", error.error);

    let mut collapsed = shown(3, "AXButton", Some("Delete"), None);
    collapsed.frame.as_mut().unwrap().height = 0.0;
    assert_eq!(
        choose(&[collapsed, whole.clone()], &named("delete"))
            .unwrap()
            .index,
        2,
        "a collapsed element is not on screen"
    );

    // A long message cut short cannot hide a short name; what is shown
    // of a cut name can.
    let mut message = shown(6, "AXStaticText", None, None);
    message.value = Some("This document explains what the button does".into());
    message.truncated = vec!["AXValue".into()];
    let ok = shown(7, "AXButton", Some("OK"), None);
    assert_eq!(
        choose(&[message, ok.clone()], &named("ok")).unwrap().index,
        7
    );

    let mut long = shown(8, "AXButton", Some("Delete every"), None);
    long.truncated = vec!["AXTitle".into()];
    assert!(named("delete everything").might_match(&long));
    assert!(!named("delete").might_match(&long));

    // The whole first part of a cut label is still judged.
    let mut parts = shown(
        9,
        "AXMenuItem",
        Some("QA Eject, Option-Space, 6 Slices"),
        None,
    );
    parts.truncated = vec!["AXTitle".into()];
    assert!(matches!(
        named("qa eject").match_of(&parts),
        Some(NameMatch::Leading)
    ));
    assert!(!named("eject").might_match(&parts));

    // A static text whose value failed to read may be the label, unless
    // it is collapsed.
    let mut unread = shown(10, "AXStaticText", None, None);
    unread.unreadable = vec!["AXValue".into()];
    assert!(named("ok").might_match(&unread));
    assert!(choose(&[unread.clone(), ok.clone()], &named("ok")).is_err());

    unread.frame.as_mut().unwrap().width = 0.0;
    assert_eq!(choose(&[unread, ok], &named("ok")).unwrap().index, 7);

    let help = shown(4, "AXButton", Some("Help"), None);
    let title = shown(5, "AXMenuBarItem", Some("Help"), None);
    assert_eq!(
        choose(&[title.clone(), help], &named("help"))
            .unwrap()
            .index,
        4,
        "the window's Help button, not the menu"
    );

    let by_role = ElementFilter {
        role: Some("AXMenuBarItem".into()),
        name: Some("help".into()),
    };
    assert_eq!(choose(&[title], &by_role).unwrap().index, 5);
}

#[test]
fn with_a_menu_open_only_its_items_are_chosen() {
    let button = shown(0, "AXButton", Some("Copy"), None);
    let mut unread = shown(1, "AXTextArea", None, None);
    unread.unreadable = vec!["AXDescription".into()];
    let mut item = shown(2, "AXMenuItem", Some("Copy"), None);
    item.menu = true;

    let elements = [button.clone(), unread.clone(), item.clone()];
    assert_eq!(
        choose(&elements, &named("copy")).unwrap().index,
        2,
        "the window's button and its unreadable neighbour are out of play"
    );

    let error = choose(&[button, unread, item], &named("paste")).unwrap_err();
    assert!(error.error.contains("open menu"), "{}", error.error);
}

#[tokio::test]
async fn incomplete_or_ambiguous_element_scans_never_dispatch_input() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;
    for incomplete in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut reads = 0;
        let error = helper
            .serve(
                click_element(
                    &client,
                    &Target::Pid(i64::from(std::process::id())),
                    &named("Save"),
                    Button::Left,
                    1,
                    &[],
                ),
                |request| {
                    if request["argv"][1] == "elements-info" {
                        reads += 1;
                        let elements = vec![
                            shown(0, "AXButton", Some("Save"), None),
                            shown(1, "AXButton", Some("Save"), None),
                        ];
                        Some(completed(
                            &json!({"ok":true,"elements":elements,"truncated":incomplete})
                                .to_string(),
                        ))
                    } else if request["op"] == "apps" {
                        Some(json!({"ok": true, "apps": [{"pid": std::process::id(), "name": "TextEdit"}],
                            "starts": {std::process::id().to_string(): [123, 456]}}))
                    } else {
                        textedit_reads(request)
                    }
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Elements);
        assert_eq!(reads, 1);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

/// A listing of `rows` buttons, as the element reply-cost fixtures read it.
fn element_rows(rows: usize) -> Value {
    let elements: Vec<Value> = (0..rows)
        .map(|index| {
            json!({
                "index": index,
                "depth": 1,
                "role": "AXButton",
                "title": atlas_core::test_support::reply_cost_text(index),
                "enabled": true,
                "frame": {"x": 10 + index, "y": 20, "width": 100, "height": 30},
                "window": {"x": 0, "y": 0, "width": 640, "height": 480},
            })
        })
        .collect();
    json!({"ok": true, "elements": elements, "truncated": false, "look": "8123abc"})
}

fn element_fixtures() -> Vec<Value> {
    atlas_core::test_support::reply_cost_fixtures(
        include_str!("../../../../tests/fixtures/reply-cost/elements.json"),
        element_rows,
    )
}

#[test]
fn element_reply_cost_preserves_listing_and_refusal_words() {
    for fixture in element_fixtures() {
        let scanned = Scanned::from_reply(fixture["input"].clone()).unwrap();
        let listed = listing(
            812,
            &scanned,
            &ElementFilter::default(),
            fixture["max"].as_u64().unwrap() as usize,
            fixture["full"] == true,
        );
        atlas_core::test_support::reply_cost(&listed, &fixture);
    }
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/choose.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let elements: Vec<Element> = serde_json::from_value(fixture["input"].clone()).unwrap();
        let error = choose(&elements, &named(fixture["name"].as_str().unwrap())).unwrap_err();
        atlas_core::test_support::reply_cost(&serde_json::to_value(error).unwrap(), &fixture);
    }
}

#[tokio::test]
async fn representative_element_reads_keep_their_helper_request_counts() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    for fixture in element_fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(dir.path(), UID);
        let helper = helper.serving_input(&client, &[LOOKS]);
        let mut operations = Vec::new();
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Name("TextEdit".into()),
                    &Asked {
                        form: if fixture["full"] == true {
                            Form::Full
                        } else {
                            Form::Compact
                        },
                        ..Asked::every(fixture["max"].as_u64().unwrap() as usize)
                    },
                ),
                |request| {
                    operations.push(request["op"].as_str().unwrap().to_owned());
                    if request["op"] == "elements" {
                        Some(fixture["input"].clone())
                    } else {
                        textedit_reads(request)
                    }
                },
            )
            .await
            .unwrap();
        atlas_core::test_support::reply_cost(&listed, &fixture);
        assert_eq!(json!(operations), fixture["helperOperations"]);
        assert_eq!(json!(helper.proofs()), fixture["helperProofs"]);
        assert_eq!(json!(helper.requests()), fixture["helperRequests"]);
    }
}

/// The screenshot TextEdit's stand-in captures: the request it was asked
/// for, refused so that nothing more is needed of the stand-in.
fn captured_asking(asked: &mut Vec<Value>) -> impl FnMut(&Value) -> Option<Value> {
    move |request: &Value| {
        if request["op"] == "screenshot" {
            asked.push(request.clone());
            return Some(json!({"ok": false, "code": "capture", "error": "not in this test"}));
        }
        textedit_reads(request)
    }
}

#[tokio::test]
async fn an_apps_part_of_a_region_is_asked_of_the_helper_and_one_showing_none_is_refused() {
    use crate::test_support::Helper;
    use atlas_core::{protocol::Region, test_support::UID};

    let asked = Region::from_points(50.0, 50.0, 200.0, 200.0).unwrap();
    for shows in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut requests = Vec::new();
        let outcome = helper
            .serve(
                screenshot(
                    &client,
                    Some(&Target::Name("TextEdit".into())),
                    Some(asked),
                    None,
                    crate::client::Size::Standard,
                    false,
                ),
                |request| {
                    if request["op"] == "screenshot" {
                        requests.push(request.clone());
                        return Some(if shows {
                            json!({"ok": false, "code": "capture", "error": "not in this test"})
                        } else {
                            json!({"ok": false, "code": "no-window",
                                "error": "Process 812 has no window on screen"})
                        });
                    }
                    textedit_reads(request)
                },
            )
            .await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["pid"], 812);
        assert_eq!(
            requests[0]["within"],
            json!(asked),
            "the helper cuts it as it captures"
        );
        assert!(requests[0].get("region").is_none());
        let error = outcome.unwrap_err();
        if !shows {
            assert_eq!(error.code, Code::NoWindow);
            assert!(
                error.error.contains("shows none of TextEdit's windows"),
                "{error}"
            );
        }
    }
}

#[tokio::test]
async fn one_window_is_asked_of_the_helper_by_its_number_and_its_owner() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    for target in [Some(Target::Name("TextEdit".into())), None] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut requests = Vec::new();
        let outcome = helper
            .serve(
                screenshot(
                    &client,
                    target.as_ref(),
                    None,
                    Some(5),
                    crate::client::Size::Standard,
                    false,
                ),
                captured_asking(&mut requests),
            )
            .await;
        assert!(outcome.is_err(), "the stand-in refuses the capture itself");
        assert_eq!(requests.len(), 1, "{target:?}");
        assert_eq!(
            (requests[0]["pid"].clone(), requests[0]["window"].clone()),
            (json!(812), json!(5))
        );
    }

    // A window no app shows is refused before any capture.
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut requests = Vec::new();
    let error = helper
        .serve(
            screenshot(
                &client,
                None,
                None,
                Some(9),
                crate::client::Size::Standard,
                false,
            ),
            captured_asking(&mut requests),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::NoWindow);
    assert!(error.error.contains("atlas windows"), "{error}");
    assert!(requests.is_empty());
}

/// TextEdit's stand-in with two buttons in its one window.
fn two_buttons(request: &Value) -> Option<Value> {
    if request["op"] == "elements" {
        let button = |index: usize, name: &str| {
            json!({"index": index, "depth": 2, "role": "AXButton", "title": name,
                "window": {"x": 0, "y": 0, "width": 100, "height": 100},
                "frame": {"x": 10, "y": 10, "width": 30, "height": 20}})
        };
        return Some(json!({"ok": true, "look": "8123abc",
            "elements": [button(1, "Save"), button(2, "Cancel")], "truncated": false}));
    }
    textedit_reads(request)
}

#[tokio::test]
async fn a_listing_names_its_windows_by_number_and_lists_only_the_window_asked_for() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for window in [None, Some(5), Some(9)] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Name("TextEdit".into()),
                    &Asked {
                        window,
                        ..Asked::every(50)
                    },
                ),
                two_buttons,
            )
            .await;
        match window {
            Some(9) => assert_eq!(listed.unwrap_err().code, Code::NoWindow),
            _ => {
                let listed = listed.unwrap();
                assert_eq!(listed["windows"][0]["number"], 5, "{listed}");
                assert_eq!(listed["matching"], 2);
            }
        }
    }
}

/// TextEdit with windows 5 and 6 at these bounds and a button in each.
fn two_windows(first: (f64, f64), second: (f64, f64)) -> impl Fn(&Value) -> Option<Value> {
    move |request: &Value| {
        let frame = |(x, y): (f64, f64)| json!({"x": x, "y": y, "width": 100, "height": 100});
        if request["op"] == "windows" {
            let window = |number: i64, at: (f64, f64)| {
                json!({"number": number, "pid": 812, "owner": "TextEdit", "title": null,
                    "layer": 0, "alpha": 1.0, "bounds": frame(at)})
            };
            return Some(json!({"ok": true, "starts": {"812": [123, 456]},
                "windows": [window(5, first), window(6, second)]}));
        }
        if request["op"] == "elements" {
            let button = |index: usize, at: (f64, f64)| {
                json!({"index": index, "depth": 2, "role": "AXButton", "title": "OK",
                    "window": frame(at),
                    "frame": {"x": at.0 + 10.0, "y": at.1 + 10.0, "width": 30, "height": 20}})
            };
            return Some(json!({"ok": true, "look": "8123abc",
                "elements": [button(1, first), button(2, second)], "truncated": false}));
        }
        textedit_reads(request)
    }
}

#[tokio::test]
async fn a_window_lists_only_its_own_elements_and_one_sharing_its_frame_is_refused() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for (second, apart) in [((300.0, 0.0), true), ((0.0, 0.0), false)] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Pid(812),
                    &Asked {
                        window: Some(6),
                        ..Asked::every(50)
                    },
                ),
                two_windows((0.0, 0.0), second),
            )
            .await;
        if apart {
            let listed = listed.unwrap();
            assert_eq!(listed["matching"], 1, "{listed}");
            assert_eq!(listed["elements"][0]["index"], 2);
            assert_eq!(listed["windows"][0]["number"], 6);
        } else {
            let error = listed.unwrap_err();
            assert_eq!(error.code, Code::Elements);
            assert!(error.error.contains("cannot be told apart"), "{error}");
        }
    }
}

#[tokio::test]
async fn windows_that_move_during_a_scan_or_share_a_frame_on_another_layer_are_not_told_apart() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    // The two windows swap places between the reads either side of the
    // scan; or a panel of the app's sits on window 5's frame.
    for case in ["swap", "panel"] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Pid(812),
                    &Asked {
                        window: Some(5),
                        ..Asked::every(50)
                    },
                ),
                |request| {
                    if request["op"] == "windows" {
                        let window = |number: i64, x: f64, layer: i64| {
                            json!({"number": number, "pid": 812, "owner": "TextEdit",
                                "title": null, "layer": layer, "alpha": 1.0,
                                "bounds": {"x": x, "y": 0, "width": 100, "height": 100}})
                        };
                        let before = request["dock"] == true;
                        let windows = match (case, before) {
                            ("swap", true) => vec![window(5, 0.0, 0), window(6, 300.0, 0)],
                            ("swap", false) => vec![window(5, 300.0, 0), window(6, 0.0, 0)],
                            _ => vec![window(6, 0.0, 3), window(5, 0.0, 0)],
                        };
                        return Some(json!({"ok": true, "starts": {"812": [123, 456]},
                            "windows": windows}));
                    }
                    two_windows((0.0, 0.0), (300.0, 0.0))(request)
                },
            )
            .await;
        let error = listed.unwrap_err();
        assert_eq!(error.code, Code::Elements, "{case}");
        let said = if case == "swap" {
            "moved while it was read"
        } else {
            "cannot be told apart"
        };
        assert!(error.error.contains(said), "{case}: {error}");
    }
}

#[tokio::test]
async fn an_older_helper_is_never_asked_for_a_part_it_cannot_crop() {
    use crate::test_support::Helper;
    use atlas_core::{protocol::Region, test_support::UID};

    for (region, window) in [
        (Region::from_points(0.0, 0.0, 50.0, 50.0), None),
        (None, Some(5)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.without_parts();
        let mut requests = Vec::new();
        let error = helper
            .serve(
                screenshot(
                    &client,
                    Some(&Target::Name("TextEdit".into())),
                    region,
                    window,
                    crate::client::Size::Standard,
                    false,
                ),
                captured_asking(&mut requests),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Daemon, "{error}");
        assert!(error.error.contains("cannot capture one window"), "{error}");
        assert!(requests.is_empty(), "nothing is captured");
    }
}

#[test]
fn a_listed_window_is_numbered_by_the_very_frame_the_scan_gave_it() {
    let frame = |x: f64| Bounds {
        x,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    let shown = [5, 6].map(|number| Window {
        bounds: frame(if number == 5 { 99.4 } else { 101.4 }),
        ..window(number, 812, "Editor", 0)
    });
    // Both frames round to 100: only the second, of window 6, is listed.
    let button = |index: usize, name: &str, x: f64| Element {
        index,
        role: Some("AXButton".into()),
        title: Some(name.into()),
        window: Some(frame(x)),
        frame: Some(frame(x + 10.0)),
        ..Element::default()
    };
    let scan = scanned(vec![button(1, "Open", 99.51), button(2, "Save", 100.49)]);
    let listed = listing_in(812, &scan, &asked_for(&named("Save")), Some(&shown));
    assert_eq!(listed["windows"][0]["x"], 100, "shown rounded");
    assert_eq!(listed["windows"][0]["number"], 6);
    let both = listing_in(812, &scan, &Asked::every(50), Some(&shown));
    assert_eq!(
        both["windows"].as_array().unwrap().len(),
        1,
        "listed as one window"
    );
    assert!(
        both["windows"][0].get("number").is_none(),
        "its elements are in windows 5 and 6, so it names neither"
    );
    let unnumbered = listing(812, &scan, &named("Save"), 50, false);
    assert!(unnumbered["windows"][0].get("number").is_none());
}

#[test]
fn a_window_filter_refuses_an_element_whose_frame_could_be_another_window() {
    let frame = |x: f64| Bounds {
        x,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    for panel_layer in [3, 0] {
        let mut panel = window(6, 812, "Editor", panel_layer);
        panel.bounds = frame(101.5);
        let mut ordinary = window(5, 812, "Editor", 0);
        ordinary.bounds = frame(100.0);
        let shown = [ordinary, panel];
        let element = |index: usize, x: f64| Element {
            index,
            role: Some("AXButton".into()),
            title: Some("OK".into()),
            window: Some(frame(x)),
            frame: Some(frame(x + 10.0)),
            ..Element::default()
        };
        // 100.75 is within a point of both windows.
        let Err(error) =
            scanned(vec![element(1, 100.0), element(2, 100.75)]).in_window(&shown, 812, 5)
        else {
            panic!("an element that could be window 6 is refused");
        };
        assert_eq!(error.code, Code::Elements, "panel at layer {panel_layer}");
        let kept = scanned(vec![element(1, 100.0), element(2, 102.5)])
            .in_window(&shown, 812, 5)
            .unwrap();
        assert_eq!(kept.elements.len(), 1, "only window 5's own element");
    }
}
