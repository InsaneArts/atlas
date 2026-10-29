//! Background jobs an agent keeps running in the helper: started and
//! stopped under control like any change, each held only until the helper
//! acknowledges it, and listed and read without control.
//!
//! The helper daemon's record of a job is the truth about its process; the
//! task's record holds the person's side's copy of its identity, which a
//! stop must agree with. A job of another task is stopped only when that
//! task is named.
use crate::{
    client::Client,
    daemon::{
        job::{How, JobState},
        protocol::{
            Expected, JOB_GRACE, JOB_LIFETIME_MAX, JOBS, JobId, SessionPin, StopReason, job_name,
        },
    },
    error::{Error, Result},
    host_session::{Key, now_ms},
    task::{record::JobEntry, store::Store},
    transport::Transport,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::time::{Duration, Instant};

/// What an older helper daemon is told it cannot do.
const OLDER: &str = "The helper is running an older Atlas that cannot keep background jobs; run atlas restart, then try again";
/// How often a stop looks at the job while it waits for it to end.
const POLL: Duration = Duration::from_millis(200);
/// How long a stop waits beyond its grace for the job to end.
const BEYOND_GRACE: Duration = Duration::from_secs(5);
/// What a stop says of processes that left the job's group.
const LEFT_RUNNING: &str =
    "These processes left the job's group and are still running; Atlas did not stop them.";

/// Start `argv` as a background job named `name` (its program's name
/// unless given) that Atlas stops after `lifetime`. Control is held until
/// the helper acknowledges it with the job's identity, which goes into the
/// task's record before this returns.
pub async fn start(
    client: &Client,
    argv: Vec<String>,
    lifetime: Duration,
    name: Option<String>,
) -> Result<Value> {
    if argv.is_empty() || argv[0].is_empty() || argv.iter().any(|a| a.contains('\0')) {
        return Err(Error::new(
            "input",
            "Provide a command and arguments after job start --",
        ));
    }
    if lifetime < Duration::from_secs(1) || lifetime > JOB_LIFETIME_MAX {
        return Err(Error::new("usage", "A job may run for 1 second to a day"));
    }
    if name.as_deref().is_some_and(|name| !job_name(name)) {
        return Err(Error::new(
            "usage",
            "Give a name of 1 to 64 characters, none of them a control character",
        ));
    }

    // The helper ends every job while agents are stopped, the person's
    // own included, so none is started then.
    if client.stop_marked() {
        return Err(Error::new("stopped", crate::error::STOPPED));
    }
    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify_offering(JOBS, OLDER).await?;
    let started = started(client, verified.pin(), argv, lifetime, name).await;
    drop(ownership);
    started
}

/// [`start`] once the helper is verified, under control the caller holds:
/// the request, pinned to the session verified, the job noted in the
/// task's record, and the reply.
async fn started(
    client: &Client,
    pin: Option<SessionPin>,
    argv: Vec<String>,
    lifetime: Duration,
    name: Option<String>,
) -> Result<Value> {
    let mut request = json!({
        "op": "job",
        "action": "start",
        "argv": argv,
        "lifetimeMs": u64::try_from(lifetime.as_millis()).unwrap_or(u64::MAX),
        "uid": client.config.helper_uid,
    });
    if let Some(name) = &name {
        request["name"] = json!(name);
    }
    if let Some(task) = &client.task {
        request["task"] = json!(task.key());
        request["hostSession"] = json!(task.session());
    }
    if let Some(pin) = pin {
        request["session"] = json!(pin);
    }
    let reply = client.change(request).await?;
    if reply["ok"] != true {
        return Err(refused(&reply));
    }

    let job = &reply["job"];
    let mut warnings = carried_warnings(&reply);
    if let Some(entry) = entry(job)
        && note(client, None, entry).is_err()
    {
        warnings.push(
            "Atlas could not note this job in the task's record; atlas job list --all shows it"
                .into(),
        );
    }
    let mut out = json!({
        "ok": true,
        "id": job["id"],
        "name": job["name"],
        "pid": job["pid"],
        "state": job["state"],
        "endsAt": job["endsAt"],
    });
    if job["state"] == "ended" {
        out["how"] = job["how"].clone();
        out["exitCode"] = job["exitCode"].clone();
    }
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    Ok(out)
}

/// Stop job `id`: TERM to its group, then KILL after `grace`, once the
/// helper's record, the task's record and the live process agree. Control
/// is held only until the helper acknowledges the TERM; the wait for the
/// job to end takes none. A job of another task is stopped only when
/// `task` names that task; one of no task by the helper's record alone.
pub async fn stop(
    client: &Client,
    id: &JobId,
    grace: Duration,
    task: Option<&Key>,
) -> Result<Value> {
    client.verify_offering(JOBS, OLDER).await?;
    let found = expected_for(client, id, task).await?;

    let ownership = client.controller.acquire_beside_commands()?;
    client.verify_offering(JOBS, OLDER).await?;
    let acknowledged = stop_held(client, id, &found.expected, grace).await?;
    drop(ownership);

    Ok(waited_for(client, id, grace, &acknowledged, found).await)
}

/// The reply to a stop the helper acknowledged as `acknowledged`, once the
/// job ended or the wait for it ran out, with no control held: the person,
/// other agents and a signal all go on meanwhile. How it ended is noted in
/// the record of the task it belongs to.
async fn waited_for(
    client: &Client,
    id: &JobId,
    grace: Duration,
    acknowledged: &Value,
    found: Found,
) -> Value {
    let started = Instant::now();
    let mut shown = acknowledged["job"].clone();
    if acknowledged["alreadyEnded"] != true {
        let until = grace + BEYOND_GRACE;
        while shown["state"] != "ended" && started.elapsed() < until {
            tokio::time::sleep(POLL).await;
            if let Ok(now) = show(client, id).await {
                shown = now;
            }
        }
    }

    if let Some(entry) = entry(&shown) {
        let _ = note(client, found.owner.as_ref(), entry);
    }
    let mut reply = stopped(id, &shown, started.elapsed());
    if acknowledged["alreadyEnded"] == true {
        reply["alreadyEnded"] = json!(true);
    }
    if found.checked == Checked::Helper {
        reply["checkedAgainst"] = json!("helper");
    }
    reply
}

/// What a stop checks a job against, and where it notes how it ended.
#[derive(Debug)]
struct Found {
    expected: Expected,
    /// The task whose record lists the job; none for a job of no task.
    owner: Option<Key>,
    checked: Checked,
}

/// Whose copy of a job's identity a stop is checked against.
#[derive(Debug, PartialEq, Eq)]
enum Checked {
    /// A task's record.
    Task,
    /// The helper's record alone: no task's record lists the job.
    Helper,
}

/// What job `id` is, as a task's record says (the one `task` names, else
/// this command's), else as the helper's says when the job is this task's
/// or no task's; and which task's record to note how it ended in.
async fn expected_for(client: &Client, id: &JobId, task: Option<&Key>) -> Result<Found> {
    let store = client.output_dir().ok().map(Store::at);
    let listed = |key: &Key| {
        store
            .as_ref()
            .and_then(|store| store.load(key))
            .and_then(|record| record.job(id).cloned())
    };
    if let Some(key) = task {
        return match listed(key) {
            Some(entry) => Ok(Found {
                expected: entry.expected,
                owner: Some(key.clone()),
                checked: Checked::Task,
            }),
            None => Err(Error::new(
                "job",
                "That task's record does not list this job; atlas job list --all shows whose it is",
            )
            .with("reason", "unknown")
            .with("id", id)),
        };
    }
    let mine = client.task.as_ref().map(|task| task.key().clone());
    if let Some(entry) = mine.as_ref().and_then(listed) {
        return Ok(Found {
            expected: entry.expected,
            owner: mine,
            checked: Checked::Task,
        });
    }

    let shown = show(client, id).await?;
    let theirs = shown["task"].as_str().and_then(Key::parse);
    match theirs {
        Some(theirs) if Some(&theirs) != mine.as_ref() => Err(Error::new(
            "job",
            format!(
                "This job belongs to another task. Stop it only to clean up after that task's \
                 session was lost: atlas job stop {id} --task {theirs}"
            ),
        )
        .with("reason", "other-task")
        .with("id", id)
        .with("task", theirs)),
        theirs => {
            let expected = serde_json::from_value::<Expected>(json!({
                "boot": shown["boot"],
                "pid": shown["pid"],
                "start": shown["start"],
                "pgid": shown["pgid"],
            }))
            .map_err(|_| {
                Error::new("job", "The job has not finished starting")
                    .with("reason", "unknown")
                    .with("id", id)
            })?;
            let checked = if theirs.is_some() {
                Checked::Task
            } else {
                Checked::Helper
            };
            Ok(Found {
                expected,
                owner: theirs,
                checked,
            })
        }
    }
}

/// Stop job `id` of `task` for a cleanup, under the control the cleanup
/// holds, checked against that task's record, and wait for it to end: how
/// it ended, or that it is still stopping.
pub(crate) async fn stop_for_cleanup(client: &Client, task: &Key, id: &JobId) -> Result<Value> {
    client.verify_offering(JOBS, OLDER).await?;
    let found = expected_for(client, id, Some(task)).await?;
    let acknowledged = stop_held(client, id, &found.expected, JOB_GRACE).await?;
    Ok(waited_for(client, id, JOB_GRACE, &acknowledged, found).await)
}

/// Ask the helper to stop job `id` when it is still `expected`, under
/// control the caller holds: its reply once TERM was sent, or once the
/// job was found already ended or stopping. A refusal keeps its reason; a
/// lost reply is of unknown outcome and stays pending.
pub(crate) async fn stop_held(
    client: &Client,
    id: &JobId,
    expected: &Expected,
    grace: Duration,
) -> Result<Value> {
    let request = json!({
        "op": "job",
        "action": "stop",
        "id": id,
        "expect": expected,
        "graceMs": u64::try_from(grace.as_millis()).unwrap_or(u64::MAX),
    });
    let reply = client.change(request).await?;
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(refused(&reply))
    }
}

/// How a stop that was waited for reads: how the job ended and what left
/// its group, or that it is still stopping. Pure, for tests.
fn stopped(id: &JobId, shown: &Value, waited: Duration) -> Value {
    let waited_ms = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX);
    if shown["state"] != "ended" {
        return json!({
            "ok": false,
            "code": "job",
            "reason": "still-stopping",
            "id": id,
            "state": "stopping",
            "remaining": shown["remaining"].as_u64().unwrap_or(0),
            "waitedMs": waited_ms,
            "uncertain": false,
            "error": "The job has not ended yet; Atlas keeps stopping it",
            "next": "atlas job list shows when it has ended.",
        });
    }
    let left = shown["left"].as_array().cloned().unwrap_or_default();
    let mut reply = json!({
        "ok": true,
        "id": id,
        "state": "ended",
        "how": shown["how"],
        "signal": shown["signal"],
        "waitedMs": waited_ms,
        "left": left,
    });
    if !shown["exitCode"].is_null() {
        reply["exitCode"] = shown["exitCode"].clone();
    }
    if !left.is_empty() {
        reply["next"] = json!(LEFT_RUNNING);
    }
    reply
}

/// This task's jobs, or with `all` every job in the helper, each saying
/// whether it is this task's and, when not, whose. No control is taken:
/// it works while agents are stopped, while an outcome is unknown and while
/// another agent acts. When the helper cannot say, the task's record is
/// shown with every job's state unknown.
pub async fn list(client: &Client, all: bool) -> Result<Value> {
    let mine = client.task.as_ref().map(|task| task.key().clone());
    let store = client.output_dir().ok().map(Store::at);
    let recorded: Vec<JobEntry> = mine
        .as_ref()
        .zip(store.as_ref())
        .and_then(|(key, store)| store.load(key))
        .map(|record| record.jobs)
        .unwrap_or_default();

    let shown = shown_all(client).await;
    let jobs = match shown {
        Ok(jobs) => jobs,
        Err(error) => {
            let jobs: Vec<Value> = recorded
                .iter()
                .map(|entry| {
                    json!({"id": entry.id, "name": entry.name, "state": "unknown",
                        "pid": entry.expected.pid, "startedAt": entry.started_at})
                })
                .collect();
            return Ok(json!({"ok": true, "jobs": jobs, "warnings": [error.error]}));
        }
    };

    let mut listed = Vec::new();
    for job in jobs {
        let Some(id) = job["id"].as_str().and_then(JobId::parse) else {
            if all {
                listed.push(job);
            }
            continue;
        };
        let task = job["task"].as_str().and_then(Key::parse);
        let ours = recorded.iter().any(|entry| entry.id == id) || (task.is_some() && task == mine);
        if !ours && !all {
            continue;
        }
        if ours
            && let Some(entry) = entry(&job)
            && recorded
                .iter()
                .any(|known| known.id == entry.id && known.state != entry.state)
        {
            let _ = note(client, None, entry);
        }
        let mut view = listed_view(&job);
        if all {
            view["mine"] = json!(ours);
            if !ours && let Some(task) = task {
                view["task"] = json!(task);
            }
        }
        listed.push(view);
    }
    Ok(json!({"ok": true, "jobs": listed}))
}

/// A job as `job list` shows it.
fn listed_view(job: &Value) -> Value {
    if job["state"] == "unknown" {
        return job.clone();
    }
    let mut view = json!({
        "id": job["id"],
        "name": job["name"],
        "state": job["state"],
        "pid": job["pid"],
        "startedAt": job["startedAt"],
        "endsAt": job["endsAt"],
    });
    if job["adopted"] == true {
        view["adopted"] = json!(true);
    }
    if job["state"] == "ended" {
        view["how"] = job["how"].clone();
        view["endedAt"] = job["endedAt"].clone();
        view["exitCode"] = job["exitCode"].clone();
        if job["left"].as_array().is_some_and(|left| !left.is_empty()) {
            view["left"] = job["left"].clone();
        }
    }
    view
}

/// The end of job `id`'s output, the last `bytes` of it, decoded lossily as
/// `run`'s output is. Read without control.
pub async fn log(client: &Client, id: &JobId, bytes: usize) -> Result<Value> {
    // Read like a list: only the helper's account serves the socket, and a
    // log is read on screen too.
    let request = json!({"op": "job", "action": "log", "id": id, "bytes": bytes});
    let reply = client.transport.call(&request).await.map_err(certain)?;
    if reply["code"] == "bad-request" {
        return Err(Error::new("daemon", OLDER));
    }
    if reply["ok"] != true {
        return Err(refused(&reply));
    }
    let data = reply["data"]
        .as_str()
        .and_then(|data| STANDARD.decode(data).ok())
        .ok_or_else(|| Error::new("protocol", "The job's log came back malformed"))?;
    Ok(json!({
        "ok": true,
        "id": id,
        "output": String::from_utf8_lossy(&data),
        "size": reply["size"],
        "outputTruncated": reply["outputTruncated"],
        "logResets": reply["logResets"],
    }))
}

/// How stopping every job came out, for a log-out or uninstall to say.
pub(crate) struct Everyone {
    /// How many were told to stop.
    pub asked: u64,
    /// How many still ran when Atlas stopped waiting.
    pub left: u64,
    /// How many the helper would not signal: their processes no longer
    /// matched them, or their records could not be written.
    pub refused: u64,
    /// Why the helper could not be told, when it could not.
    pub error: Option<String>,
}

impl Everyone {
    /// Add what this came to to `reply`: `jobsStopped`, and a warning for
    /// jobs still running or a helper that could not be told.
    pub fn say(&self, reply: &mut Value) {
        reply["jobsStopped"] = json!(self.asked.saturating_sub(self.left));
        let warning = match (&self.error, self.left) {
            (Some(error), _) => Some(format!(
                "Atlas could not stop the helper's background jobs: {error}"
            )),
            (None, 0) => None,
            (None, left) => Some(format!(
                "{} still ran when Atlas stopped waiting",
                crate::ui::text::count(left as usize, "background job")
            )),
        };
        let refused = (self.refused > 0).then(|| {
            format!(
                "{} could not be stopped, and nothing was signalled; atlas job list --all shows {}",
                crate::ui::text::count(self.refused as usize, "background job"),
                if self.refused == 1 { "it" } else { "them" }
            )
        });
        for warning in warning.into_iter().chain(refused) {
            match reply["warnings"].as_array_mut() {
                Some(warnings) => warnings.push(json!(warning)),
                None => reply["warnings"] = json!([warning]),
            }
        }
    }
}

/// Stop every job in the helper for `reason`, by the helper's own records,
/// and wait for them to end as long as a stop's grace and KILL take, under
/// control the caller holds.
pub(crate) async fn stop_every(client: &Client, reason: StopReason) -> Everyone {
    let request = json!({"op": "job", "action": "stop-all", "reason": reason});
    let none = Everyone {
        asked: 0,
        left: 0,
        refused: 0,
        error: None,
    };
    let reply = match client.transport.call(&request).await {
        Ok(reply) if reply["ok"] == true => reply,
        // A daemon from before jobs, or none running, keeps none.
        Ok(reply) if reply["code"] == "bad-request" => return none,
        Err(error) if error.code == "io" && !client.transport.path.exists() => return none,
        Ok(reply) => {
            return Everyone {
                error: Some(refused(&reply).error),
                ..none
            };
        }
        Err(error) => {
            return Everyone {
                error: Some(error.error),
                ..none
            };
        }
    };
    let asked = reply["stopping"].as_u64().unwrap_or(0);
    let refused = reply["refused"]
        .as_array()
        .map_or(0, |refused| refused.len() as u64);
    let mut left = asked;
    let started = Instant::now();
    while left > 0 && started.elapsed() < JOB_GRACE + BEYOND_GRACE {
        tokio::time::sleep(POLL).await;
        if let Ok(jobs) = shown_all(client).await {
            left = jobs
                .iter()
                .filter(|job| matches!(job["state"].as_str(), Some("running" | "stopping")))
                .count() as u64;
        }
    }
    Everyone {
        asked,
        left,
        refused,
        error: None,
    }
}

/// Job `id` as the helper shows it now. Reading changes nothing.
pub(crate) async fn show(client: &Client, id: &JobId) -> Result<Value> {
    let reply = client
        .transport
        .call(&json!({"op": "job", "action": "show", "id": id}))
        .await
        .map_err(certain)?;
    if reply["ok"] == true {
        Ok(reply["job"].clone())
    } else {
        Err(refused(&reply))
    }
}

/// Every job as the helper shows it now, or why it cannot say.
async fn shown_all(client: &Client) -> Result<Vec<Value>> {
    let daemon = Transport {
        timeout: Duration::from_secs(10),
        ..client.transport.clone()
    };
    let reply = daemon
        .call(&json!({"op": "job", "action": "show"}))
        .await
        .map_err(certain)?;
    if reply["ok"] == true {
        return Ok(reply["jobs"].as_array().cloned().unwrap_or_default());
    }
    if reply["code"] == "bad-request" {
        return Err(Error::new("daemon", OLDER));
    }
    Err(refused(&reply))
}

/// The task's record's entry for a job the helper shows as `job`.
fn entry(job: &Value) -> Option<JobEntry> {
    Some(JobEntry {
        id: job["id"].as_str().and_then(JobId::parse)?,
        name: job["name"].as_str()?.to_owned(),
        started_at: job["startedAt"].as_u64()?,
        expected: serde_json::from_value(json!({
            "boot": job["boot"],
            "pid": job["pid"],
            "start": job["start"],
            "pgid": job["pgid"],
        }))
        .ok()?,
        state: serde_json::from_value::<JobState>(job["state"].clone()).ok()?,
        how: serde_json::from_value::<How>(job["how"].clone()).ok(),
        ended_at: job["endedAt"].as_u64(),
        later: Map::new(),
    })
}

/// Note `entry` in the record of the task `owner` names, else of this
/// command's task; nothing without one.
fn note(client: &Client, owner: Option<&Key>, entry: JobEntry) -> Result<()> {
    let task = match owner {
        Some(owner) => {
            let store = Store::at(client.output_dir()?);
            let Some(record) = store.load(owner) else {
                return Ok(());
            };
            crate::task::Task::recorded(&record)
        }
        None => match &client.task {
            Some(task) => task.clone(),
            None => return Ok(()),
        },
    };
    Store::at(client.output_dir()?).update(&task, now_ms(), |record| {
        if record.noted_job(entry) {
            Ok(())
        } else {
            Err(Error::new(
                "state",
                "This task's record is full of jobs that have not ended",
            ))
        }
    })
}

/// The warnings a reply carried, as text.
fn carried_warnings(reply: &Value) -> Vec<String> {
    reply["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| warning.as_str().map(str::to_owned))
        .collect()
}

/// A helper's refusal of a job request, keeping its code, why and which
/// job, and whether it may have happened anyway.
fn refused(reply: &Value) -> Error {
    let mut error = crate::transport::refusal(reply);
    for field in ["reason", "id", "task", "state"] {
        if let Some(value) = reply.get(field).filter(|value| !value.is_null()) {
            error = error.with(field, value);
        }
    }
    error
}

/// `error` from something that only read: nothing in the helper changed.
fn certain(error: Error) -> Error {
    Error {
        uncertain: false,
        ..error
    }
}

#[cfg(test)]
mod tests;
