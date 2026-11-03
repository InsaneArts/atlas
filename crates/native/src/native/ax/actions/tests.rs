use super::*;
use atlas_core::screen::elements::LOOK_AGAIN;

#[test]
fn a_state_already_as_asked_is_left_and_an_unread_one_is_never_toggled_blind() {
    for on in [true, false] {
        assert!(!needed(Some(on), on, true).unwrap());
        assert!(needed(Some(!on), on, true).unwrap());
        assert!(needed(None, on, false).unwrap(), "a pick selects again");

        let error = needed(None, on, true).unwrap_err();
        assert_eq!(error.code, Code::Input);
        assert!(!error.uncertain);
        assert_eq!(error.details["next"], INSTEAD);
    }
}

#[test]
fn only_a_flag_or_a_zero_or_one_is_a_two_way_state() {
    assert_eq!(two_way(CFBoolean::new(true)), Some(true));
    assert_eq!(two_way(CFBoolean::new(false)), Some(false));
    assert_eq!(two_way(&CFNumber::new_i64(1)), Some(true));
    assert_eq!(two_way(&CFNumber::new_i64(0)), Some(false));
    assert_eq!(two_way(&CFNumber::new_i64(2)), None, "a mixed checkbox");
    assert_eq!(two_way(&CFString::from_static_str("1")), None);
}

/// What an element's value reads as now.
fn holding(value: &CFType) -> Option<&CFType> {
    Some(value)
}

#[test]
fn a_value_is_set_as_the_number_or_text_the_element_holds() {
    let old = CFString::from_static_str("old");
    let text = value_for("draft", holding(&old)).unwrap();
    assert_eq!(
        text.downcast_ref::<CFString>().unwrap().to_string(),
        "draft"
    );
    let empty = value_for("", None).unwrap();
    assert_eq!(empty.downcast_ref::<CFString>().unwrap().to_string(), "");

    let stepper = CFNumber::new_i64(3);
    let whole = value_for("9007199254740993", holding(&stepper)).unwrap();
    assert_eq!(
        whole.downcast_ref::<CFNumber>().unwrap().as_i64(),
        Some(9_007_199_254_740_993),
        "a whole number is set exactly"
    );

    let slider = CFNumber::new_f64(10.0);
    let number = value_for(" 42.5 ", holding(&slider)).unwrap();
    assert_eq!(
        number.downcast_ref::<CFNumber>().unwrap().as_f64(),
        Some(42.5)
    );
    for refused in ["forty", "NaN", "inf", ""] {
        let error = value_for(refused, holding(&slider)).unwrap_err();
        assert_eq!(error.code, Code::Input, "{refused}");
        assert_eq!(
            error.error, "The element holds a number: give a number; nothing was sent",
            "the value is never echoed"
        );
    }

    let error = value_for("on", holding(CFBoolean::new(true))).unwrap_err();
    assert_eq!(error.code, Code::Input);
    assert_eq!(error.details["next"], INSTEAD);
}

#[test]
fn a_password_field_is_told_by_its_role_or_subrole() {
    assert!(secure(Some("AXSecureTextField"), None));
    assert!(secure(Some("AXTextField"), Some("AXSecureTextField")));
    assert!(!secure(Some("AXTextField"), Some("AXSearchField")));
    assert!(!secure(None, None));
}

/// Only an answer that says the app took the request is done; one that
/// says it refused it changed nothing; any other may have been carried out.
#[test]
fn an_action_the_app_did_not_acknowledge_is_uncertain() {
    acknowledged(AXError::Success).unwrap();
    for refused in [
        AXError::ActionUnsupported,
        AXError::AttributeUnsupported,
        AXError::IllegalArgument,
        AXError::NotImplemented,
    ] {
        let error = acknowledged(refused).unwrap_err();
        assert_eq!(error.code, Code::Input);
        assert!(!error.uncertain);
    }
    for unknown in [
        AXError::CannotComplete,
        AXError::Failure,
        AXError::InvalidUIElement,
        AXError::APIDisabled,
    ] {
        let error = acknowledged(unknown).unwrap_err();
        assert_eq!(error.code, Code::Uncertain);
        assert!(error.uncertain);
    }
}

#[test]
fn refusals_before_the_action_send_nothing_and_say_what_to_do() {
    for error in [slow(), unreadable(AXError::CannotComplete)] {
        assert_eq!(error.code, Code::Elements);
        assert!(!error.uncertain);
        assert_eq!(error.details["next"], LOOK_AGAIN);
    }
    let error = disabled();
    assert_eq!(error.code, Code::Input);
    assert_eq!(error.details["next"], DISABLED_CONTROL);
    assert_eq!(
        unsupported("pressed").error,
        "The element cannot be pressed; nothing was sent"
    );
}

#[test]
fn nothing_is_read_once_the_deadline_has_passed() {
    let error = left(Instant::now()).unwrap_err();
    assert_eq!(error.code, Code::Elements);
    assert!(left(Instant::now() + ACT).is_ok());
}
