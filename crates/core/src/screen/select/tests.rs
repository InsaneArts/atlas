use super::*;

fn with_role(role: &str) -> Element {
    Element {
        role: Some(role.to_owned()),
        title: Some("Smart links".to_owned()),
        ..Element::default()
    }
}

fn role(role: &str) -> ElementFilter {
    ElementFilter {
        role: Some(role.to_owned()),
        name: None,
    }
}

#[test]
fn a_role_matches_with_or_without_its_ax_and_whatever_its_case() {
    let checkbox = with_role("AXCheckBox");
    for asked in ["AXCheckBox", "axcheckbox", "checkbox", "CheckBox"] {
        assert!(role(asked).matches(&checkbox), "{asked}");
    }
    for asked in ["check", "AXCheck", "box", "AX", ""] {
        assert!(!role(asked).matches(&checkbox), "{asked}");
    }
    assert!(role("textfield").matches(&with_role("AXTextField")));
    assert!(
        !role("button").matches(&with_role("AXPopUpButton")),
        "a role names it whole"
    );
    assert!(role("button").matches(&with_role("AXButton")));
}
