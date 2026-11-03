use super::*;

#[test]
fn every_code_has_a_next_step_for_an_agent() {
    for code in Code::ALL {
        let step = next(code, None);
        assert!(
            step.ends_with('.') && !step.contains('\n'),
            "{code}: {step}"
        );
    }
    assert_eq!(next(Code::Elements, None), fix(Code::Elements).unwrap());
    assert!(!step(None, None, false).is_empty());
}

/// Steps only the person may take (resuming agents, logging the helper
/// in, setting Atlas up, choosing to lose work) are never an agent's.
#[test]
fn an_agent_is_never_told_to_do_what_only_the_person_may() {
    let stopped = next(Code::Stopped, None);
    assert!(stopped.contains("ask"), "{stopped}");
    assert!(!stopped.contains("Resume"), "{stopped}");

    let held = next(Code::LogoutHeld, None);
    let force = held.find("--force").unwrap();
    assert!(held[..force].contains("show the person"), "{held}");
    assert!(held.contains("only if they agree"), "{held}");

    for code in Code::ALL {
        let step = next(code, None);
        assert!(!step.starts_with("Run atlas init"), "{code}: {step}");
        assert!(!step.contains("in the viewer"), "{code}: {step}");
        assert!(!step.contains("Resume"), "{code}: {step}");
    }

    // Even when it may or may not have happened, an agent waiting for
    // the person is never told to act (atlas restart is not refused
    // while agents are stopped).
    for (code, holder) in [
        (Code::Stopped, None),
        (Code::Cancelled, None),
        (Code::OnConsole, None),
        (Code::Busy, Some(Holder::Person)),
    ] {
        let said = step(Some(code), holder, true);
        assert_eq!(said, next(code, holder), "{code}");
        assert!(!said.contains("atlas restart"), "{code}: {said}");
        assert!(
            said.contains("person") || said.contains("ask"),
            "{code}: {said}"
        );
    }
    assert_eq!(step(Some(Code::Busy), Some(Holder::Agent), true), UNCERTAIN);
    assert_eq!(step(Some(Code::Transport), None, true), UNCERTAIN);
    assert_eq!(
        step(Some(Code::Transport), None, false),
        next(Code::Transport, None)
    );
}

#[test]
fn busy_says_who_to_wait_for() {
    assert!(next(Code::Busy, Some(Holder::Person)).contains("the person gives control back"));
    let agent = next(Code::Busy, Some(Holder::Agent));
    assert!(agent.contains("Another Atlas command"), "{agent}");
    assert!(agent.contains("waited"), "{agent}");
    let unknown = next(Code::Busy, None);
    assert!(unknown.contains("Never take control"), "{unknown}");
    assert_ne!(
        next(Code::Busy, None),
        fix(Code::Busy).unwrap(),
        "not the person's"
    );
}

#[test]
fn busy_after_waiting_says_atlas_already_waited() {
    assert!(WAITED.ends_with('.'), "{WAITED}");
    assert!(WAITED.contains("already waited"), "{WAITED}");
    assert!(!WAITED.contains("Resume"), "{WAITED}");
    assert!(!WAITED.contains("in the viewer"), "{WAITED}");
}

/// An earlier action's unknown outcome, perhaps another agent's, is
/// recovered without asking only by its owner; an agent's own action
/// that may or may not have happened is its own to recover.
#[test]
fn an_earlier_unknown_outcome_is_recovered_only_by_its_owner() {
    let said = next(Code::Uncertain, None);
    assert!(said.contains("only if that action was yours"), "{said}");
    assert!(said.contains("ask the person"), "{said}");
    assert_eq!(step(Some(Code::Uncertain), None, false), said);
    assert_eq!(step(Some(Code::Uncertain), None, true), UNCERTAIN);
}

/// The advice for a focus refusal with `reason` and `unread`, after
/// something of the command was sent or before.
fn focus_changed(sent: bool, reason: Option<&str>, unread: Option<&str>) -> String {
    let result = serde_json::json!({
        "ok": false,
        "code": "focus-changed",
        "reason": reason,
        "unread": unread,
        "performed": u64::from(sent),
    });
    focus_advice(&result).unwrap()
}

/// Opening an app again does not bring it forward in the helper, and
/// takes focus from whoever moved it; a click where the app shows does.
#[test]
fn a_focus_refusal_never_sends_an_agent_to_open_the_app() {
    for sent in [false, true] {
        for (reason, unread) in EVERY_WAY {
            let said = focus_changed(sent, reason, unread);
            assert!(!said.contains("atlas open"), "{said}");
        }
        for reason in ["app", "unreadable"] {
            let said = focus_changed(sent, Some(reason), None);
            assert!(
                said.to_lowercase().contains("click where your app shows"),
                "{said}"
            );
        }
    }
    assert_eq!(
        next(Code::FocusChanged, None),
        focus_changed(false, Some("app"), None)
    );
    assert_eq!(
        step(Some(Code::FocusChanged), None, false),
        focus_changed(false, Some("app"), None)
    );
}

/// Every kind of thing a focus refusal can find in the way, as its
/// `reason` and `unread`.
const EVERY_WAY: [(Option<&str>, Option<&str>); 12] = [
    (Some("app"), None),
    (Some("window"), None),
    (Some("sheet"), None),
    (Some("field"), None),
    (Some("disabled"), None),
    (Some("unreadable"), None),
    (Some("unreadable"), Some("alike")),
    (Some("unreadable"), Some("unlisted")),
    (Some("unreadable"), Some("withheld")),
    (Some("unreadable"), Some("cut")),
    (Some("unreadable"), Some("deep")),
    (Some("unreadable"), Some("slow")),
];

/// A refusal for the window, a sheet or the field, or one Atlas could
/// not tell, says what was in the way in its own words.
#[test]
fn a_focus_refusal_says_what_was_in_the_way() {
    for (reason, unread, said) in [
        (Some("app"), None, "Another app"),
        (Some("point"), None, "Another app"),
        (Some("window"), None, "Another window of your app"),
        (Some("sheet"), None, "A sheet or dialog"),
        (Some("field"), None, "Another field"),
        (Some("disabled"), None, "That field is disabled"),
        (
            Some("unreadable"),
            None,
            "Atlas couldn't tell what was in front",
        ),
        (
            Some("unreadable"),
            Some("failed"),
            "Atlas couldn't tell what",
        ),
        (Some("unreadable"), Some("alike"), "look the same"),
        (Some("unreadable"), Some("unlisted"), "not listed yet"),
        (Some("unreadable"), Some("withheld"), "screen is locked"),
        (Some("unreadable"), Some("cut"), "too long to read whole"),
        (Some("unreadable"), Some("deep"), "deeper in your app"),
        (
            Some("unreadable"),
            Some("slow"),
            "did not say where its focus is",
        ),
        (Some("made-up"), None, "Another app"),
        (Some("app"), Some("slow"), "Another app"),
        (None, None, "Another app"),
    ] {
        let hint = focus_changed(false, reason, unread);
        assert!(hint.contains(said), "{reason:?} {unread:?}: {hint}");
    }
}

/// Only a refusal after something of the command was sent says so, and
/// asks for the rest alone; one that sent nothing is run again whole.
#[test]
fn a_focus_refusal_says_earlier_actions_were_sent_only_when_they_were() {
    for (reason, unread) in EVERY_WAY {
        let sent = focus_changed(true, reason, unread);
        assert!(sent.contains("Earlier actions were sent"), "{sent}");
        assert!(sent.contains("send only the rest"), "{sent}");
        assert!(!sent.contains("run it again"), "{sent}");

        let nothing = focus_changed(false, reason, unread);
        assert!(!nothing.contains("Earlier actions"), "{nothing}");
        assert!(nothing.contains("nothing"), "{nothing}");
        assert!(nothing.contains("again"), "{nothing}");
        let remedy = nothing.split(". ").nth(1).unwrap_or_default();
        assert!(
            remedy.starts_with(char::is_uppercase),
            "a sentence: {nothing}"
        );
    }
}

/// An app that does not say where its focus is may keep not saying it, so
/// a refusal that sent nothing never promises the same command will work;
/// after one more try the agent tells the person instead of repeating it.
#[test]
fn an_unreadable_focus_result_does_not_promise_that_repeating_the_command_will_work() {
    let said = focus_changed(false, Some("unreadable"), Some("slow"));
    assert_eq!(
        said,
        "Your app did not say where its focus is, and nothing was sent. Wait a moment and try once more; if it refuses again, tell the person the app is not answering instead of repeating the command."
    );
    assert!(!said.contains("Run the same command again"), "{said}");
    assert!(
        !said.contains("in time"),
        "it may have answered at once: {said}"
    );

    let sent = focus_changed(true, Some("unreadable"), Some("slow"));
    assert!(
        sent.starts_with("Your app did not say where its focus is. Earlier actions were sent"),
        "{sent}"
    );
}

/// When what is in front cannot be read, often because the active app
/// has no window, the agent is told to bring its app up first.
#[test]
fn an_unreadable_front_asks_for_the_app_to_be_clicked_or_opened_first() {
    for sent in [false, true] {
        let said = focus_changed(sent, Some("unreadable"), None);
        assert!(
            said.starts_with("Atlas couldn't tell what was in front"),
            "{said}"
        );
        assert!(said.contains("open the app if none of it shows"), "{said}");
        let other = focus_changed(sent, Some("app"), None);
        assert!(!other.contains("open the app"));
        assert!(other.starts_with("Another app was in front"), "{other}");
    }
}

/// The advice for a refused result reads what it says was sent and what
/// was in the way; anything else is advised by its code.
#[test]
fn a_focus_refusals_advice_reads_its_reason_and_what_was_sent() {
    use serde_json::json;

    let advice = |fields: serde_json::Value| {
        let mut result = json!({"ok": false, "code": "focus-changed"});
        if let Some(fields) = fields.as_object() {
            result.as_object_mut().unwrap().extend(fields.clone());
        }
        focus_advice(&result)
    };
    assert_eq!(
        advice(json!({"reason": "field", "typed": 16, "partial": true})),
        Some(focus_changed(true, Some("field"), None))
    );
    assert_eq!(
        advice(json!({"reason": "field", "partial": true})),
        Some(focus_changed(true, Some("field"), None))
    );
    assert_eq!(
        advice(json!({"reason": "unreadable", "unread": "alike", "performed": 0})),
        Some(focus_changed(false, Some("unreadable"), Some("alike")))
    );
    assert_eq!(advice(json!({"uncertain": true})), None);
    assert_eq!(focus_advice(&json!({"ok": false, "code": "busy"})), None);
}

/// A command a signal stopped while its change was under way left that
/// change unknown: it is the agent's own, to recover.
#[test]
fn an_interrupted_command_that_was_cut_is_told_to_recover_it() {
    assert_eq!(step(Some(Code::Interrupted), None, true), UNCERTAIN);
    let said = step(Some(Code::Interrupted), None, false);
    assert_eq!(said, next(Code::Interrupted, None));
    assert!(said.contains("nothing it did is left unknown"), "{said}");
    assert!(!said.contains("atlas restart"), "{said}");
}

/// Every code an agent can meet has a step for a person too.
#[test]
fn every_documented_code_has_a_fix() {
    for code in Code::ALL {
        assert!(
            fix(code).is_some() || [Code::Cancelled, Code::Terminal].contains(&code),
            "no fix for {code}"
        );
    }
}
