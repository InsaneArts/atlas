//! The activity rows a client writes for what its commands do: fixed
//! codes and what [`Doing`](crate::doing::Doing) names, never typed text,
//! argv or output. The log itself is [`crate::activity_log`]'s.
use super::{Actor, Client, timestamp};
use crate::{
    activity_log::{self, Failure, Op, Row, Step},
    error::{Code, Error, Result},
};
use serde_json::{Value, json};
use std::time::Duration;

/// The code a failure is recorded with: `uncertain` when it may have
/// finished in the helper all the same, so the activity never says it
/// didn't happen.
pub(super) fn recorded_code(error: &Error) -> Code {
    if error.uncertain {
        Code::Uncertain
    } else {
        error.code
    }
}

/// Why a mutation failed, as a fixed code: the helper's error code, or
/// `timeout` and `exit` for a command that ran too long or failed. Only a
/// short lowercase code is kept, so no message text reaches the log.
fn failure_code(reply: &Value) -> Option<Failure> {
    if let Some(code) = reply["code"].as_str() {
        let fixed = !code.is_empty()
            && code.len() <= 32
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-');
        return fixed.then(|| Failure::parse(code));
    }
    if reply["timedOut"] == true {
        return Some(Failure::Timeout);
    }
    (!reply["signal"].is_null() || reply["exitCode"].as_i64().is_some_and(|code| code != 0))
        .then_some(Failure::Exit)
}

/// Why a focus refusal in `reply` was refused, when it is one of the input
/// service's fixed words: another app had keyboard focus (`app`), another
/// app's window was at the point (`point`), another window of the app had
/// focus or was there (`window`), a sheet over it had focus (`sheet`),
/// another field had focus (`field`) or the field was disabled
/// (`disabled`), or Atlas couldn't tell (`unreadable`).
fn focus_reason(reply: &Value) -> Option<&'static str> {
    match reply["reason"].as_str()? {
        "app" => Some("app"),
        "point" => Some("point"),
        "window" => Some("window"),
        "sheet" => Some("sheet"),
        "field" => Some("field"),
        "disabled" => Some("disabled"),
        "unreadable" => Some("unreadable"),
        _ => None,
    }
}

/// Whether a mutation did what it was for: the helper accepted it and, for
/// a command, the command ran to its end and succeeded.
pub(super) fn worked(reply: &Value) -> bool {
    reply["ok"] == true
        && reply["exitCode"].as_i64().is_none_or(|code| code == 0)
        && reply["timedOut"] != true
        && reply["signal"].is_null()
}

impl Client {
    /// Record that a command began, so what it does shows while it runs;
    /// the row it ends with stands for it after.
    pub(super) async fn record_start(&self, request: &Value) -> Result<()> {
        self.recorded(request, &json!({"ok": null}), Duration::ZERO, Step::Start)
            .await
    }

    /// Write the activity row of a command that ended as `reply` says.
    pub(crate) async fn record(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
    ) -> Result<()> {
        self.recorded(request, reply, elapsed, Step::End).await
    }

    /// [`Client::write_row`] on a blocking thread: a row waits its turn
    /// behind another command's, which must not hold up the runtime.
    async fn recorded(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
        step: Step,
    ) -> Result<()> {
        let (client, request, reply) = (self.clone(), request.clone(), reply.clone());
        tokio::task::spawn_blocking(move || client.write_row(&request, &reply, elapsed, step))
            .await
            .map_err(|_| Error::new(Code::State, "The activity row could not be written"))?
    }

    /// [`Client::record_start`] on this thread, for tests.
    #[cfg(test)]
    pub(super) fn record_start_blocking(&self, request: &Value) -> Result<()> {
        self.write_row(request, &json!({"ok": null}), Duration::ZERO, Step::Start)
    }

    /// [`Client::record`] on this thread, for tests.
    #[cfg(test)]
    pub(super) fn record_blocking(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
    ) -> Result<()> {
        self.write_row(request, reply, elapsed, Step::End)
    }

    /// [`Client::record`] a finished command's row, for tests elsewhere in
    /// the crate.
    #[cfg(test)]
    pub(crate) fn record_for_test(&self, request: &Value, reply: &Value) {
        self.record_blocking(request, reply, Duration::ZERO)
            .unwrap();
    }

    /// Append the activity row for `step` of `request`, which `reply`
    /// answered, after `elapsed`.
    fn write_row(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
        step: Step,
    ) -> Result<()> {
        let directory = self.output_dir()?;
        let start = step == Step::Start;
        let operation = Op::of(request);

        // No input text, argv, coordinates, window titles, output or image
        // payloads in metadata: only what `Doing` names.
        let ok = worked(reply);
        let mut row = Row {
            at: Some(timestamp().to_string()),
            operation: operation.clone(),
            ok: (!start).then_some(ok),
            elapsed_ms: Some(elapsed.as_secs_f64() * 1000.0),
            action_count: request["actions"]
                .as_array()
                .map(|actions| actions.len() as u64),
            actor: Some(self.actor.as_str().to_owned()),
            ..Row::default()
        };

        // A failure keeps its fixed code (never its message), so the app can
        // say why in plain words. A refusal for another app in front says
        // whether some of the command was sent before it, and why it was
        // refused.
        if !start
            && !ok
            && let Some(code) = failure_code(reply)
        {
            if code == Failure::Code(Code::FocusChanged) {
                let sent = reply["performed"]
                    .as_u64()
                    .is_some_and(|performed| performed > 0);
                row.partial = Some(sent || reply["partial"] == true);
                row.reason = focus_reason(reply).map(str::to_owned);
            }
            row.code = Some(code);
        }

        // Only an agent's own command names its host session; the person's,
        // through a viewer, never does.
        if self.actor == Actor::Agent
            && let Some(session) = &self.host_session
        {
            row.session = Some(session.key().as_str().to_owned());
        }

        // A task ATLAS_TASK split from its session names its key, never its
        // name; a session's default task adds nothing.
        if self.actor == Actor::Agent
            && let Some(task) = &self.task
            && task.name().is_some()
        {
            row.task = Some(task.key().as_str().to_owned());
        }

        // A collection says how many files and bytes it kept, never which.
        if operation == Some(Op::Collect) {
            row.count = reply["count"].as_u64();
            row.bytes = reply["bytes"].as_u64();
        }

        // A screenshot names the file it gave: Atlas's own timestamp name,
        // never a path or a person's file.
        if operation == Some(Op::Screenshot)
            && let Some(capture) = reply["capture"].as_str()
            && crate::helper_info::capture_time(capture).is_some()
        {
            row.capture = Some(capture.to_owned());
        }

        if let Some(doing) = &self.doing {
            doing.fill(&mut row, ok);
            if start {
                row.started = Some(true);
            }
            // Input names the app it landed in, as the helper saw it.
            if row.app.is_none()
                && let Some(app) = reply["app"].as_str()
            {
                row.app = Some(crate::doing::bounded(app));
            }
        }

        activity_log::append(directory, &row)
    }
}
