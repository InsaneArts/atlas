//! The activity rows a client writes for what its commands do: fixed
//! codes and what [`Doing`](crate::doing::Doing) names, never typed text,
//! argv or output. The log itself is [`crate::activity_log`]'s.
use super::{Actor, Client, timestamp};
use crate::{
    activity_log::{self, Failure, Op, Row, Step},
    error::{Code, Error, Result},
    ui::text::same_text,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tokio::time::Instant;

/// How long the person's input through a viewer gathers into one row
/// before the row is written.
pub(crate) const PERSON_INPUT_SPAN: Duration = Duration::from_secs(5);

/// The person's input through a viewer, gathered into one row a span,
/// [`PERSON_INPUT_SPAN`] unless made with another: their mouse and keyboard
/// send a message every few milliseconds, and a row for each would push
/// everyone else's work out of the log's reach. The row counts the actions
/// and takes the newest one's time; like every row, it keeps no coordinates
/// or text. Clones share it.
#[derive(Clone)]
pub(crate) struct PersonInput {
    gathering: Arc<Mutex<Gathering>>,
    span: Duration,
}

impl Default for PersonInput {
    fn default() -> Self {
        Self::with_span(PERSON_INPUT_SPAN)
    }
}

/// The current span and its deadline, rows awaiting a successful append,
/// and which generation has a timer.
#[derive(Default)]
struct Gathering {
    row: Option<(PathBuf, Row, Instant)>,
    pending: VecDeque<(PathBuf, Row)>,
    generation: u64,
    timed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct InputTimer {
    generation: u64,
    deadline: Instant,
}

impl Gathering {
    fn keep_row(&mut self) {
        if let Some((directory, row, _)) = self.row.take() {
            self.pending.push_back((directory, row));
        }
    }

    fn write_pending(&mut self) -> Result<()> {
        while let Some((directory, row)) = self.pending.front() {
            activity_log::append(directory, row)?;
            self.pending.pop_front();
        }
        Ok(())
    }
}

impl PersonInput {
    /// Input gathered into one row each `span`.
    pub(crate) fn with_span(span: Duration) -> Self {
        Self {
            gathering: Arc::default(),
            span,
        }
    }

    /// How long input gathers into one row before it is written.
    #[cfg(test)]
    pub(crate) fn span(&self) -> Duration {
        self.span
    }

    /// Gather `row`, one input that worked, for `directory`'s log: written
    /// once its span ends, with the earlier gathered row first when that
    /// span has ended or it is for another log.
    fn gather(&self, directory: &Path, row: Row) -> Result<()> {
        let mut gathering = self.lock();
        if let Some((into, gathered, deadline)) = gathering.row.as_mut()
            && into == directory
            && Instant::now() < *deadline
        {
            gathered.action_count =
                Some(gathered.action_count.unwrap_or(0) + row.action_count.unwrap_or(0));
            gathered.elapsed_ms =
                Some(gathered.elapsed_ms.unwrap_or(0.0) + row.elapsed_ms.unwrap_or(0.0));
            gathered.at = row.at;
            return gathering.write_pending();
        }

        gathering.keep_row();
        gathering.row = Some((directory.to_owned(), row, Instant::now() + self.span));
        gathering.generation = gathering.generation.wrapping_add(1);
        gathering.timed = false;
        gathering.write_pending()
    }

    /// Write the row gathered so far, when there is one.
    pub(crate) fn flush(&self) -> Result<()> {
        let mut gathering = self.lock();
        gathering.timed = false;
        gathering.keep_row();
        gathering.write_pending()
    }

    /// Time this generation's row once, at the deadline set when it began.
    pub(super) fn untimed(&self) -> Option<InputTimer> {
        let mut gathering = self.lock();
        let deadline = gathering.row.as_ref()?.2;
        if gathering.timed {
            return None;
        }
        gathering.timed = true;
        Some(InputTimer {
            generation: gathering.generation,
            deadline,
        })
    }

    /// A timer only writes the row it was scheduled for, once it is due.
    fn flush_due(&self, timer: InputTimer) -> Result<()> {
        let mut gathering = self.lock();
        if gathering.generation != timer.generation
            || gathering
                .row
                .as_ref()
                .is_none_or(|(_, _, deadline)| *deadline != timer.deadline)
            || Instant::now() < timer.deadline
        {
            return Ok(());
        }
        gathering.timed = false;
        gathering.keep_row();
        gathering.write_pending()
    }

    pub(super) async fn write_at(self, timer: InputTimer) -> Result<()> {
        tokio::time::sleep_until(timer.deadline).await;
        tokio::task::spawn_blocking(move || self.flush_due(timer))
            .await
            .map_err(|_| Error::new(Code::State, "The activity row could not be written"))?
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Gathering> {
        self.gathering
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

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

/// The app `reply` says a command acted in, by its name, with its bundle
/// identifier when that is one: the app input landed in, as the helper saw
/// it, or the app an install placed.
fn bundle_seen(reply: &Value, installs: bool) -> Option<(String, String)> {
    let (app, id) = if installs {
        let installed = crate::environment::installed_app(reply)?;
        (installed.app, installed.bundle_id?)
    } else {
        let app = reply["app"].as_str()?;
        (app.to_owned(), reply["bundleId"].as_str()?.to_owned())
    };
    Some((app, crate::doing::bundle_id(&id)?))
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
        let written =
            tokio::task::spawn_blocking(move || client.write_row(&request, &reply, elapsed, step))
                .await
                .map_err(|_| Error::new(Code::State, "The activity row could not be written"))?;

        // The person's gathered input is written once its span ends, even
        // when no more comes.
        if let Some(gathering) = self.person_input.clone()
            && let Some(timer) = gathering.untimed()
        {
            tokio::spawn(async move {
                let _ = gathering.write_at(timer).await;
            });
        }

        written
    }

    /// Write the person's input gathered so far, as when they give control
    /// back.
    pub(crate) async fn flush_person_input(&self) -> Result<()> {
        let Some(gathering) = self.person_input.clone() else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || gathering.flush())
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
            // Typed text names its field only by the role the helper found
            // it of, checking it had keyboard focus.
            doing.fill(&mut row, ok, reply["field"]["role"].as_str());
            if start {
                row.started = Some(true);
            }
            // Input names the app it landed in, as the helper saw it.
            if row.app.is_none()
                && let Some(app) = reply["app"].as_str()
            {
                row.app = Some(crate::doing::bounded(app));
            }
            // So does its bundle identifier, as an install's does the app it
            // placed: only an identifier, and only the one of the app the
            // row names, never another's.
            if let Some((app, id)) = bundle_seen(reply, doing.verb() == "install")
                && row
                    .app
                    .as_deref()
                    .is_some_and(|named| same_text(named, &crate::doing::bounded(&app)))
            {
                row.bundle_id = Some(id);
            }
        }

        // The person's input through a viewer is gathered; a failure is
        // written at once, after the input gathered before it.
        if let Some(gathering) = &self.person_input
            && row.persons_input()
            && !start
        {
            if ok && row.code.is_none() {
                return gathering.gather(directory, row);
            }
            gathering.flush()?;
        }

        activity_log::append(directory, &row)
    }
}
