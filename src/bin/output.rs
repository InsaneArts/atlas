//! Result presentation, output streams and process result classification.
use atlas::{
    error::{Error, Holder},
    ui::{
        Format, hints,
        render::{self, View},
    },
};
use serde_json::{Value, json};
use std::io::Write;

/// Write a result: JSON for programs, its view for a person. A failure for
/// programs says what to do next when `advised`. A reader that went away
/// (`| head`) ends the output quietly.
pub(super) fn emit(format: Format, view: &View, value: &Value, advised: bool) {
    if value.is_null() {
        return;
    }

    match format {
        Format::Json => {
            let text = if advised && value["ok"] == false {
                let mut advised = value.clone();
                advise(&mut advised);
                program_text(&advised)
            } else {
                program_text(value)
            };
            let _ = writeln!(std::io::stdout().lock(), "{text}");
        }
        Format::Human => {
            let shown = render::render(view, value);
            if !shown.out.is_empty() {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(shown.out.as_bytes());
                if !shown.out.ends_with('\n') {
                    let _ = out.write_all(b"\n");
                }
                let _ = out.flush();
            }

            if !shown.err.is_empty() {
                eprintln!("{}", shown.err);
            }
        }
    }
}

/// A result as programs read it: one line of JSON.
fn program_text(value: &Value) -> String {
    value.to_string()
}

/// A failure as programs read it, with `next` (what to do about it) when
/// `advised`.
pub(super) fn failure(error: &Error, advised: bool) -> Value {
    let mut value = json!(error);
    if advised {
        advise(&mut value);
    }
    value
}

/// Add `next` to a failed result (`ok: false` with a code) that has none,
/// as [`hints::step`] says, or for a focus refusal [`hints::focus_advice`]
/// from what was in the way and what the result says was sent before it.
fn advise(value: &mut Value) {
    if value["ok"] != false || value.get("next").is_some() {
        return;
    }

    let Some(code) = value["code"].as_str() else {
        return;
    };

    let next = hints::focus_advice(value).unwrap_or_else(|| {
        let holder = value["holder"].as_str().and_then(Holder::parse);
        hints::step(code, holder, value["uncertain"] == true).to_owned()
    });
    value["next"] = json!(next);
}

/// Write `text` as it is. A reader that went away (`| head`) ends the
/// output quietly, as in [`emit`].
pub(super) fn print(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// Whether a reported result gives the invoking process a failure exit status.
pub(super) fn failed(result: &Value) -> bool {
    result["ok"] == false
        || result["ready"] == false
        || result["timedOut"] == true
        || result.get("exitCode").is_some_and(|v| v != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_failure_for_programs_says_what_to_do_next() {
        for code in atlas::error::CODES {
            let error = Error::new(code, "It failed");
            assert_eq!(
                failure(&error, true)["next"],
                hints::next(code, None),
                "{code}"
            );
        }

        let uncertain = Error::new("transport", "No reply").uncertain();
        assert_eq!(failure(&uncertain, true)["next"], hints::UNCERTAIN);

        let held = Error::new("busy", "Wait").held_by(Holder::Person);
        assert_eq!(
            failure(&held, true)["next"],
            hints::next("busy", Some(Holder::Person))
        );

        let waited = Error::new("busy", "Kept")
            .held_by(Holder::Agent)
            .with("waited", 30)
            .with("next", hints::WAITED);
        assert_eq!(
            failure(&waited, true)["next"],
            hints::WAITED,
            "a wait already made is kept"
        );

        let yours = Error::new("uncertain", "Unknown")
            .with("yours", true)
            .with("next", hints::UNKNOWN_OWN);
        assert_eq!(
            failure(&yours, true)["next"],
            hints::UNKNOWN_OWN,
            "an unknown action of this session's own keeps its step"
        );

        let said = Error::new("elements", "Incomplete").with("next", "Try again.");
        assert_eq!(
            failure(&said, true)["next"],
            "Try again.",
            "its own is kept"
        );

        let mut result = json!({"ok": false, "code": "wait-failed", "uncertain": false});
        advise(&mut result);
        assert_eq!(result["next"], hints::next("wait-failed", None));

        let mut typed = json!({"ok": false, "code": "busy", "holder": "agent", "uncertain": true});
        advise(&mut typed);
        assert_eq!(typed["next"], hints::UNCERTAIN);

        // Waiting for the person keeps its own step, even when it may have
        // happened: never atlas restart.
        for (mut waiting, code, holder) in [
            (
                json!({"ok": false, "code": "stopped", "uncertain": true}),
                "stopped",
                None,
            ),
            (
                json!({"ok": false, "code": "cancelled", "uncertain": true}),
                "cancelled",
                None,
            ),
            (
                json!({"ok": false, "code": "busy", "holder": "person", "uncertain": true}),
                "busy",
                Some(Holder::Person),
            ),
        ] {
            advise(&mut waiting);
            assert_eq!(waiting["next"], hints::next(code, holder), "{code}");
        }
        let stopped = Error::new("stopped", "Stopped").uncertain();
        assert_eq!(
            failure(&stopped, true)["next"],
            hints::next("stopped", None)
        );

        for mut untouched in [
            json!({"ok": true, "code": "elements"}),
            json!({"ok": true, "next": "atlas init"}),
            json!({"ok": false}),
        ] {
            let before = untouched.clone();
            advise(&mut untouched);
            assert_eq!(untouched, before);
        }
    }

    /// A focus refusal says earlier actions were sent only when the result
    /// counts some, and asks for the app first when what was in front could
    /// not be read.
    #[test]
    fn a_focus_refusal_is_advised_by_what_was_sent_before_it() {
        let refusal = |fields: Value| {
            let mut value = json!({"ok": false, "code": "focus-changed", "uncertain": false});
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            advise(&mut value);
            value["next"].clone()
        };

        for (fields, sent, in_the_way) in [
            (
                json!({"reason": "app", "performed": 0, "partial": false}),
                false,
                hints::InTheWay::App,
            ),
            (json!({"reason": "point"}), false, hints::InTheWay::App),
            (
                json!({"reason": "unreadable", "performed": 0}),
                false,
                hints::InTheWay::Unknown(hints::Unsure::Front),
            ),
            (
                json!({"reason": "app", "performed": 2}),
                true,
                hints::InTheWay::App,
            ),
            (
                json!({"reason": "app", "typed": 48, "partial": true}),
                true,
                hints::InTheWay::App,
            ),
            (
                json!({"reason": "point", "typed": 0, "partial": true}),
                true,
                hints::InTheWay::App,
            ),
            (
                json!({"reason": "unreadable", "performed": 1}),
                true,
                hints::InTheWay::Unknown(hints::Unsure::Front),
            ),
            (
                json!({"reason": "sheet", "performed": 0}),
                false,
                hints::InTheWay::Sheet,
            ),
            (
                json!({"reason": "field", "typed": 16, "partial": true}),
                true,
                hints::InTheWay::Field,
            ),
            (
                json!({"reason": "window", "performed": 0}),
                false,
                hints::InTheWay::Window,
            ),
        ] {
            assert_eq!(
                refusal(fields.clone()),
                hints::focus_changed(sent, in_the_way),
                "{fields}"
            );
        }

        let error = Error::new("focus-changed", "Calculator has keyboard focus")
            .with("reason", "unreadable");
        assert_eq!(
            failure(&error, true)["next"],
            hints::focus_changed(false, hints::InTheWay::Unknown(hints::Unsure::Front))
        );
    }

    #[test]
    fn a_failure_another_atlas_process_reads_has_no_next_step() {
        let error = Error::new("transport", "No reply");
        assert!(failure(&error, false).get("next").is_none());
        assert_eq!(failure(&error, false), json!(error));
    }

    #[test]
    fn programs_get_each_result_on_one_line() {
        let value = json!({
            "ok": true,
            "text": "first line\nsecond line",
            "nested": {"list": [1, {"deep": "a\nb"}], "empty": {}},
        });
        let text = program_text(&value);

        assert!(!text.contains('\n'), "{text}");
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
    }

    #[test]
    fn null_and_ordinary_success_results_keep_a_successful_exit_status() {
        for result in [
            Value::Null,
            json!({}),
            json!(true),
            json!(false),
            json!({"ok": true}),
            json!({"ready": true}),
            json!({"timedOut": false}),
            json!({"exitCode": 0}),
            json!({"ok": 0, "ready": "", "timedOut": 1}),
        ] {
            assert!(!failed(&result), "{result}");
        }
    }

    #[test]
    fn any_explicit_failure_result_sets_a_failure_exit_status() {
        for result in [
            json!({"ok": false}),
            json!({"ready": false}),
            json!({"timedOut": true}),
            json!({"exitCode": 1}),
            json!({"exitCode": -1}),
            json!({"ok": true, "ready": false, "exitCode": 0}),
        ] {
            assert!(failed(&result), "{result}");
        }
    }

    #[test]
    fn present_noninteger_exit_codes_retain_their_failure_status() {
        for code in [
            Value::Null,
            json!("0"),
            json!(true),
            json!([]),
            json!({}),
            json!(0.0),
        ] {
            let result = json!({"exitCode": code});
            assert!(failed(&result), "{result}");
        }
    }
}
