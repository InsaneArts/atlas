use super::*;
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
fn named(name: &str) -> Asked {
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

    let alone = listing_in(1, &scan, &named("Reloa"), None);
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
