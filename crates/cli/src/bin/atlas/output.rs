//! Result presentation, output streams and process result classification.
use atlas_cli::ui::{
    Format,
    render::{self, View},
};
use atlas_core::error::advice as hints;
use atlas_core::error::{Code, Error, Holder};
use serde_json::{Value, json};
use std::io::Write;

/// Report startup only after the viewer has recorded its redacted readiness facts.
pub(super) fn serving_menu_bar(shown: &Value) {
    if atlas_cli::ui::human() {
        println!("The menu bar is running. Quit it from its menu, or press Ctrl-C here.");
    } else {
        println!("{shown}");
    }
}

/// Report a detached menu bar using only public process facts.
pub(super) fn started_menu_bar(already: bool, record: &Value) {
    if atlas_cli::ui::human() {
        let what = if already {
            "is already running"
        } else {
            "is running"
        };
        println!(
            "{} The menu bar {what}. Click Atlas at the top of the screen; quit it from its menu.",
            atlas_cli::ui::style::ok()
        );
    } else {
        println!(
            "{}",
            json!({"ok": true, "running": true, "alreadyRunning": already, "pid": record["pid"]})
        );
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
                advised.to_string()
            } else {
                value.to_string()
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
        hints::step(Code::parse(code), holder, value["uncertain"] == true).to_owned()
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
#[path = "output_tests.rs"]
mod tests;
