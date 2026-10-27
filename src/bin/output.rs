//! Result presentation, output streams and process result classification.
use super::cli::{AgentsCommand, Command, EnvironmentCommand};
use atlas::{
    error::Error,
    ui::{
        Format, hints,
        render::{self, View},
        text,
    },
};
use serde_json::{Value, json};
use std::io::Write;

/// How a command's result reads for a person.
pub(super) fn view(command: &Command) -> View {
    let at = |x: f64, y: f64| format!("{}, {}", text::number(x), text::number(y));

    match command {
        Command::Doctor {
            login_timing: true, ..
        } => View::LoginTiming,
        Command::Doctor { verbose, .. } => View::Doctor { verbose: *verbose },
        Command::Status => View::Status,
        Command::Config => View::Config,
        Command::Screenshot { .. } => View::Capture,
        Command::Apps { .. } => View::Apps,
        Command::Windows { .. } => View::Windows,
        Command::Elements { .. } => View::Elements,
        Command::Readiness { .. } => View::Readiness,
        Command::Click {
            x: Some(x),
            y: Some(y),
            ..
        } => View::Action(format!("Clicked at {}", at(*x, *y))),
        Command::Click { .. } => View::Action("Clicked".into()),
        Command::Move { x, y } => View::Action(format!("Moved the pointer to {}", at(*x, *y))),
        Command::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            ..
        } => View::Action(format!(
            "Dragged from {} to {}",
            at(*from_x, *from_y),
            at(*to_x, *to_y)
        )),
        Command::Scroll { x, y, .. } => View::Action(format!("Scrolled at {}", at(*x, *y))),
        Command::Key { keys } => View::Action(format!("Pressed {}", keys.join(" "))),
        Command::Input { .. } => View::Action("Performed the actions".into()),
        Command::Type { .. } => View::Type,
        Command::Run { .. }
        | Command::Environment {
            command: Some(EnvironmentCommand::Run { .. }),
            ..
        } => View::Run,
        Command::Environment {
            command:
                None
                | Some(
                    EnvironmentCommand::Apply { .. }
                    | EnvironmentCommand::Rollback { .. }
                    | EnvironmentCommand::Profile { .. },
                ),
            ..
        } => View::Environment,
        Command::Close { .. } => View::Close,
        Command::Environment {
            command: Some(EnvironmentCommand::Discover { .. }),
            ..
        } => View::Discover,
        Command::Setup {
            apply: false,
            rollback: false,
            ..
        } => View::SetupPreview,
        Command::Open { app, targets, .. } => View::Open(
            app.iter()
                .chain(targets)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Command::Wait { gone, .. } => View::Wait { gone: *gone },
        Command::Restart => View::Restart,
        Command::Stop => View::Action("Stopped the agent".into()),
        Command::Resume => View::Action("Resumed the agent".into()),
        Command::Logout { .. } => View::Logout,
        Command::StartInput => View::StartInput,
        Command::Install => View::Install,
        Command::Agents {
            action: Some(AgentsCommand::Install { .. } | AgentsCommand::Remove { .. }),
        } => View::AgentsChange,
        Command::Agents {
            action: Some(AgentsCommand::Project { .. } | AgentsCommand::Forget { .. }),
        } => View::AgentsProject,
        Command::Agents { .. } => View::Agents,
        Command::Switch { .. } => View::Switch,
        Command::Update { check: true, .. } => View::Generic,
        Command::Update { .. } => View::Update,
        Command::Uninstall { .. } => View::Uninstall,
        Command::Menubar { login: Some(_), .. } => View::Login,
        Command::Init
        | Command::Monitor { .. }
        | Command::Watch { .. }
        | Command::Mcp { .. }
        | Command::Menubar { .. } => View::Quiet,
        _ => View::Generic,
    }
}

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
/// as [`hints::step`] says.
fn advise(value: &mut Value) {
    if value["ok"] != false || value.get("next").is_some() {
        return;
    }

    let Some(code) = value["code"].as_str() else {
        return;
    };

    let next = hints::step(code, value["holder"].as_str(), value["uncertain"] == true);
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
    use atlas::error::Holder;

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
            hints::next("busy", Some("person"))
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
                Some("person"),
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
