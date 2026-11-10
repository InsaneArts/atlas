//! Background jobs an agent keeps running in the helper: started and
//! stopped under control like any change, each held only until the helper
//! acknowledges it, and listed and read without control.
//!
//! The helper daemon's record of a job is the truth about its process; the
//! task's record holds the person's side's copy of its identity, which a
//! stop must agree with. A job of another task is stopped only when that
//! task is named.
use crate::{
    client::{Client, Mutation, unconfirmed},
    task::{
        Task,
        record::{JobEntry, Noted},
        store::Store,
    },
};
use atlas_core::{
    error::{Code, Error, Result},
    host::Key,
    lenient::Lenient,
    protocol::{
        Expected, How, JOB_GRACE, JOB_LIFETIME_MAX, JOB_LIFETIME_MIN, JOBS, JobId, JobRefusal,
        JobReply, JobStart, JobState, JobView, Logged, Progress, SessionPin, Showing, Shown,
        StopCameTo, StopReason, Stopped, job_name,
    },
    time::now_ms,
    transport::Transport,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::time::{Duration, Instant};

/// What an older helper daemon is told it cannot do.
const OLDER: &str = "The helper is running an older Atlas that cannot keep background jobs; run atlas restart, then try again";
/// How often a stop looks at the job while it waits for it to end.
const POLL: Duration = Duration::from_millis(200);
/// How long a stop waits beyond its grace for the job to end.
const BEYOND_GRACE: Duration = Duration::from_secs(5);
/// What a stop says of processes still running.
const LEFT_RUNNING: &str = "These processes are still running; Atlas did not stop them.";
/// What a stop says of processes still running once the job's leader was
/// gone, in its group or out of it.
const LEADER_GONE: &str =
    "The job's first process ended, so Atlas did not stop these processes. They are still running.";

/// Start `argv` as a background job named `name` (its program's name
/// unless given) that Atlas stops after `lifetime`. Control is held until
/// the helper acknowledges it with the job's identity, which goes into the
/// task's record before this returns.
///
/// # Errors
///
/// Returns argument, ownership, helper-identity or job-start confirmation failures. A
/// missing acknowledgement can leave a started job pending.
pub async fn start(
    client: &Client,
    argv: Vec<String>,
    lifetime: Duration,
    name: Option<String>,
) -> Result<Value> {
    if argv.first().is_none_or(String::is_empty) || argv.iter().any(|a| a.contains('\0')) {
        return Err(Error::new(
            Code::Input,
            "Provide a command and arguments after job start --",
        ));
    }
    if !(JOB_LIFETIME_MIN..=JOB_LIFETIME_MAX).contains(&lifetime) {
        return Err(Error::new(
            Code::Usage,
            "A job may run for 1 second to a day",
        ));
    }
    if name.as_deref().is_some_and(|name| !job_name(name)) {
        return Err(Error::new(
            Code::Usage,
            "Give a name of 1 to 64 characters, none of them a control character",
        ));
    }

    // The helper ends every job while agents are stopped, the person's
    // own included, so none is started then.
    if client.stop_marked() {
        return Err(Error::stopped());
    }
    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify_offering(JOBS, OLDER).await?;
    let started = started(client, verified.pin()?, argv, lifetime, name).await;
    drop(ownership);
    started
}

/// [`start`] once the helper is verified, under control the caller holds:
/// the request, pinned to the session verified, the job noted in the
/// task's record, and the reply.
async fn started(
    client: &Client,
    pin: SessionPin,
    argv: Vec<String>,
    lifetime: Duration,
    name: Option<String>,
) -> Result<Value> {
    let mutation = Mutation::JobStart {
        start: JobStart {
            argv,
            lifetime,
            name,
            task: client.task.as_ref().map(|task| task.key().clone()),
            session: client.task.as_ref().map(|task| task.session().clone()),
        },
        uid: client.config.helper_uid,
        pin,
    };
    let reply = mutate(client, mutation).await?;
    let Ok(JobReply::Start { job }) = JobReply::deserialize(&reply) else {
        return Err(unconfirmed());
    };
    let mut warnings: Vec<String> = reply["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| warning.as_str().map(str::to_owned))
        .collect();
    if let Some(entry) = entry(&job)
        && note(client, None, entry).is_err()
    {
        warnings.push(
            "Atlas could not note this job in the task's record; atlas job list --all shows it"
                .into(),
        );
    }
    let mut out = json!({
        "ok": true,
        "id": job.id,
        "name": job.name,
        "pid": job.pid,
        "state": job.state,
        "endsAt": job.ends_at,
    });
    if job.state == JobState::Ended {
        out["how"] = json!(job.ended().map(|ended| ended.how));
        out["exitCode"] = json!(job.ended().and_then(|ended| ended.exit_code));
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
///
/// # Errors
///
/// Returns job-identity, ownership, transport or stop-confirmation failures. A successful
/// signal is not proof that all descendants have exited.
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
    acknowledged: &Stopped,
    found: Found,
) -> Value {
    let started = Instant::now();
    let already_ended = matches!(acknowledged.came_to, StopCameTo::AlreadyEnded(true));
    let mut shown = Shown::Job(Box::new(acknowledged.job.clone()));
    if !already_ended {
        let until = grace + BEYOND_GRACE;
        while shown.state() != Some(JobState::Ended) && started.elapsed() < until {
            tokio::time::sleep(POLL).await;
            if let Ok(now) = show(client, id).await {
                shown = now;
            }
        }
    }

    let noted = shown
        .job()
        .and_then(entry)
        .map(|entry| note(client, found.owner.as_ref(), entry));
    let mut reply = stopped(id, &shown, started.elapsed());
    if matches!(noted, Some(Ok(Noted::Disagrees))) {
        reply["mismatch"] = json!(true);
        reply["warnings"] = json!([DISAGREES]);
    }
    if already_ended {
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
    let mine = client.task.as_ref().map(|task| task.key());
    let owner = task.or(mine);
    if let Some(entry) = owner.and_then(listed) {
        return Ok(Found {
            expected: entry.expected,
            owner: owner.cloned(),
            checked: Checked::Task,
        });
    }
    if task.is_some() {
        return Err(unknown_job(
            id,
            "That task's record does not list this job; atlas job list --all shows whose it is",
        ));
    }

    let shown = show(client, id).await?;
    let theirs = shown.job().and_then(|job| job.task.as_ref());
    if let Some(theirs) = theirs
        && Some(theirs) != mine
    {
        return Err(other_task(id, theirs, "stop"));
    }
    // No task's record lists it: the helper's record is all it is checked
    // against, whichever task that record names.
    Ok(Found {
        expected: shown
            .job()
            .and_then(JobView::expected)
            .ok_or_else(|| unknown_job(id, "The job has not finished starting"))?,
        owner: theirs.cloned(),
        checked: Checked::Helper,
    })
}

/// The refusal of a stop of job `id`, which is not known well enough to stop.
fn unknown_job(id: &JobId, said: &str) -> Error {
    Error::new(Code::Job, said)
        .with("reason", json!(JobRefusal::Unknown))
        .with("id", json!(id))
}

/// Stop job `id` of `task` for a cleanup, under the control the cleanup
/// holds, checked against that task's record, and wait for it to end: how
/// it ended, or that it is still stopping.
pub(crate) async fn stop_for_cleanup(client: &Client, task: &Key, id: &JobId) -> Result<Value> {
    client.verify_offering(JOBS, OLDER).await?;
    let found = match expected_for(client, id, Some(task)).await {
        Ok(found) => found,
        // A job the task's record never noted (its start was not confirmed,
        // or its entry could not be written) is stopped by the helper's
        // record alone, when that record says it is this task's.
        Err(error) if refusal_of(&error) == Some(JobRefusal::Unknown) => {
            let Some(job) = show(client, id).await?.job().cloned() else {
                return Err(error);
            };
            if job.task.as_ref() != Some(task) {
                return Err(error);
            }
            Found {
                expected: job.expected().ok_or(error)?,
                owner: Some(task.clone()),
                checked: Checked::Helper,
            }
        }
        Err(error) => return Err(error),
    };
    let acknowledged = stop_held(client, id, &found.expected, JOB_GRACE).await?;
    Ok(waited_for(client, id, JOB_GRACE, &acknowledged, found).await)
}

/// The refusal of `command` (stop or log) of job `id`, which belongs to
/// task `theirs`, naming the key to pass.
fn other_task(id: &JobId, theirs: &Key, command: &str) -> Error {
    Error::new(
        Code::Job,
        format!(
            "This job belongs to another task. Use it only to clean up after that task's \
             session was lost: atlas job {command} {id} --task {theirs}"
        ),
    )
    .with("reason", json!(JobRefusal::OtherTask))
    .with("id", json!(id))
    .with("task", json!(theirs))
}

/// Why the helper refused a job request, when the refusal says.
pub(crate) fn refusal_of(error: &Error) -> Option<JobRefusal> {
    error
        .details
        .get("reason")
        .and_then(|reason| JobRefusal::deserialize(reason).ok())
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
) -> Result<Stopped> {
    let mutation = Mutation::JobStop {
        id,
        expected,
        grace,
    };
    let reply = mutate(client, mutation).await?;
    match JobReply::deserialize(&reply) {
        Ok(JobReply::Stop(stopped)) => Ok(stopped),
        _ => Err(unconfirmed()),
    }
}

/// Send a job `mutation`: the helper's reply, unless it refused.
async fn mutate(client: &Client, mutation: Mutation<'_>) -> Result<Value> {
    let reply = client.mutate(mutation, None, None, None).await?;
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(refused(&reply))
    }
}

/// How a stop that was waited for reads: how the job ended and what left
/// its group, or that it is still stopping. Pure, for tests.
fn stopped(id: &JobId, shown: &Shown, waited: Duration) -> Value {
    let waited_ms = atlas_core::time::ms(waited);
    let ended = shown
        .job()
        .filter(|job| job.state == JobState::Ended)
        .map(|job| (job, job.ended()));
    let Some((job, ended)) = ended else {
        let remaining = match shown.job().map(|job| &job.progress) {
            Some(Progress::Stopping(stopping)) => stopping.remaining,
            Some(Progress::Ended(ended)) => ended.remaining,
            Some(Progress::Neither {}) | None => None,
        };
        return json!({
            "ok": false,
            "code": "job",
            "reason": JobRefusal::StillStopping,
            "id": id,
            "state": JobState::Stopping,
            "remaining": remaining.unwrap_or(0),
            "waitedMs": waited_ms,
            "uncertain": false,
            "error": "The job has not ended yet; Atlas keeps stopping it",
            "next": "atlas job list shows when it has ended.",
        });
    };
    let left = ended.map(|ended| ended.left.as_slice()).unwrap_or_default();
    let how = ended.map(|ended| ended.how);
    let mut reply = json!({
        "ok": true,
        "id": id,
        "state": job.state,
        "how": how,
        "signal": ended.and_then(|ended| ended.signal.as_deref()),
        "waitedMs": waited_ms,
        "left": left,
    });
    if let Some(code) = ended.and_then(|ended| ended.exit_code) {
        reply["exitCode"] = json!(code);
    }
    if !left.is_empty() {
        reply["next"] = json!(if how == Some(How::LeaderGone) {
            LEADER_GONE
        } else {
            LEFT_RUNNING
        });
    }
    reply
}

/// This task's jobs, or with `all` every job in the helper, each saying
/// whether it is this task's and, when not, whose. No control is taken:
/// it works while agents are stopped, while an outcome is unknown and while
/// another agent acts. When the helper cannot say, the task's record is
/// shown with every job's state unknown.
///
/// # Errors
///
/// Returns helper or job-list decoding failures. Unreadable supervision state is not
/// reported as an empty list.
pub async fn list(client: &Client, all: bool) -> Result<Value> {
    let mine = client.task.as_ref().map(|task| task.key().clone());
    let store = client.output_dir().ok().map(Store::at);
    let recorded: Vec<JobEntry> = mine
        .as_ref()
        .zip(store.as_ref())
        .and_then(|(key, store)| store.load(key))
        .map(|record| record.jobs)
        .unwrap_or_default();

    let jobs = match shown_all(client).await {
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
        let Some(id) = job.id() else {
            if all {
                listed.push(json!(job));
            }
            continue;
        };
        let task = job.job().and_then(|job| job.task.clone());
        let ours = recorded.iter().any(|entry| entry.id == id) || (task.is_some() && task == mine);
        if !ours && !all {
            continue;
        }
        let mut view = listed_view(&job);
        // How far a job of this task got is noted; the identity it was
        // first noted with is not, and one the helper disagrees with is said.
        if let Some(entry) = job.job().and_then(entry)
            && let Some(known) = recorded.iter().find(|known| known.id == entry.id)
        {
            if known.expected != entry.expected {
                view["mismatch"] = json!(true);
            } else if known.state != entry.state || known.how != entry.how {
                let _ = note(client, None, entry);
            }
        }
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

/// What the person hears when an agent's background job keeps them from
/// signing the helper in to an account.
pub const JOB_RUNNING: &str =
    "An agent's background job is still running. Stop it, or try again when it ends.";

/// Refuse while any background job in the helper may still have processes:
/// one that is starting, running or stopping, one whose record can't be
/// read or that a later Atlas shows in another shape, and one that ended but
/// left processes that still run as the helper. The helper's own list is
/// read, once and bounded; unlike [`list`], a list that can't be read,
/// decoded or completed refuses, and the task's records never stand in.
///
/// # Errors
///
/// Returns `busy`, held by an agent with `running` `job`, while a job may
/// still run, and `job` when the helper's list can't be read.
pub async fn require_none(client: &Client) -> Result<()> {
    let jobs = shown_all(client).await.map_err(|cause| {
        Error::new(
            Code::Job,
            "Atlas couldn't read the helper's background jobs. Try again.",
        )
        .with_source(cause)
    })?;
    if jobs
        .iter()
        .any(|job| may_run(job, client.config.helper_uid))
    {
        return Err(Error::new(Code::Busy, JOB_RUNNING)
            .held_by(atlas_core::error::Holder::Agent)
            .with("running", json!("job")));
    }
    Ok(())
}

/// Whether the job `shown` may still have processes in helper account
/// `helper`. Only a job that ended, with every process it left gone or now
/// another account's, has none.
fn may_run(shown: &Shown, helper: u32) -> bool {
    let Some(job) = shown.job() else {
        return true;
    };
    match (job.state, job.ended()) {
        (JobState::Ended, Some(ended)) => {
            atlas_core::protocol::left_may_run(&ended.left, job.pgid, helper)
        }
        _ => true,
    }
}

/// A job as `job list` shows it: always its identity and times; whether
/// it was adopted, how its stop goes and the last signal sent, when known;
/// how it ended once its state is ended; and the processes that left its
/// group or that its leader left behind, which still run and were never
/// signalled.
fn listed_view(shown: &Shown) -> Value {
    let Some(job) = shown.job() else {
        return json!(shown);
    };
    let mut full = json!(job);
    let mut view = json!({});
    for field in ["id", "name", "state", "pid", "startedAt", "endsAt"] {
        view[field] = full[field].take();
    }
    for field in ["adopted", "remaining", "refused", "signal"] {
        if !full[field].is_null() {
            view[field] = full[field].take();
        }
    }
    if job.state == JobState::Ended {
        for field in ["how", "endedAt", "exitCode"] {
            view[field] = full[field].take();
        }
    }
    if full["left"].as_array().is_some_and(|left| !left.is_empty()) {
        view["left"] = full["left"].take();
    }
    view
}

/// The end of job `id`'s output, the last `bytes` of it, decoded lossily as
/// `run`'s output is. Read without control.
///
/// # Errors
///
/// Returns invalid job, byte-range, helper or log-read failures. The read does not change
/// the job's lifecycle.
pub async fn log(client: &Client, id: &JobId, bytes: usize, task: Option<&Key>) -> Result<Value> {
    // Read like a list: only the helper's account serves the socket, and a
    // log is read on screen too. Another task's job's output is read only
    // when that task is named, as for a stop.
    match show(client, id).await {
        Ok(shown) => {
            let theirs = shown.job().and_then(|job| job.task.as_ref());
            let mine = client.task.as_ref().map(|task| task.key());
            if let Some(theirs) = theirs
                && Some(theirs) != mine
                && Some(theirs) != task
            {
                return Err(other_task(id, theirs, "log"));
            }
        }
        Err(error) if error.code == Code::Daemon || error.details.contains_key("reason") => {
            return Err(error);
        }
        Err(_) => {}
    }
    let request = json!({"op": "job", "action": "log", "id": id, "bytes": bytes});
    let reply = read_one(client, &request).await?;
    let malformed = || Error::new(Code::Protocol, "The job's log came back malformed");
    let Ok(JobReply::Log(Logged {
        data,
        size,
        output_truncated,
        log_resets,
        ..
    })) = JobReply::deserialize(&reply)
    else {
        return Err(malformed());
    };
    let data = STANDARD.decode(data).map_err(|_| malformed())?;
    Ok(json!({
        "ok": true,
        "id": id,
        "output": String::from_utf8_lossy(&data),
        "size": size,
        "outputTruncated": output_truncated,
        "logResets": log_resets,
    }))
}

/// How stopping every job came out, for a log-out or uninstall to say.
#[derive(Default)]
pub struct Everyone {
    /// How many were told to stop.
    pub asked: u64,
    /// How many still ran when Atlas stopped waiting.
    pub(crate) left: u64,
    /// How many the helper would not signal: their processes no longer
    /// matched them, or their records could not be written.
    pub(crate) refused: u64,
    /// How many the helper could not observe, so it signalled nothing.
    pub(crate) unknown: u64,
    /// How many records the helper could not read, whose jobs may still run.
    pub(crate) unresolved: u64,
    /// How many other records could not be read when Atlas stopped waiting:
    /// each may be of a job told to stop, which may still run.
    pub(crate) unread: u64,
    /// Why the helper could not be told, when it could not.
    pub(crate) error: Option<String>,
}

impl Everyone {
    /// The helper could not be told, for the reason `error`.
    fn failed(error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }

    /// How many of those told to stop have stopped. One whose record could
    /// not be read again is not known to have.
    pub fn stopped(&self) -> u64 {
        self.asked
            .saturating_sub(self.left.saturating_add(self.unread))
    }

    /// A warning for jobs still running or a helper that could not be told.
    pub fn warnings(&self) -> Vec<String> {
        let jobs = |count: u64| atlas_core::name::count(count as usize, "background job");
        let them = |count: u64| if count == 1 { "it" } else { "them" };
        let warning = match (&self.error, self.left) {
            (Some(error), _) => Some(format!(
                "Atlas could not stop the helper's background jobs: {error}"
            )),
            (None, 0) => None,
            (None, left) => Some(format!(
                "{} still ran when Atlas stopped waiting",
                jobs(left)
            )),
        };
        let refused = (self.refused > 0).then(|| {
            format!(
                "{} could not be stopped, and nothing was signalled; atlas job list --all shows {}",
                jobs(self.refused),
                them(self.refused)
            )
        });
        let unknown = (self.unknown > 0).then(|| {
            format!(
                "Atlas could not see the processes of {}, so nothing was signalled; atlas job list --all shows {}",
                jobs(self.unknown),
                them(self.unknown)
            )
        });
        let unresolved = self.unresolved.saturating_add(self.unread);
        let unresolved = (unresolved > 0).then(|| {
            let (records, they) = if unresolved == 1 {
                ("record", "it")
            } else {
                ("records", "they")
            };
            format!(
                "Atlas could not read the {records} of {}; {they} may still be running",
                jobs(unresolved)
            )
        });
        warning
            .into_iter()
            .chain(refused)
            .chain(unknown)
            .chain(unresolved)
            .collect()
    }

    /// Add what this came to to `reply`: `jobsStopped` and its warnings.
    pub(crate) fn say(&self, reply: &mut Value) {
        reply["jobsStopped"] = json!(self.stopped());
        for warning in self.warnings() {
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
pub async fn stop_every(client: &Client, reason: StopReason) -> Everyone {
    let request = json!({"op": "job", "action": "stop-all", "reason": reason});
    stopped_every(client, client.transport.call(&request)).await
}

/// [`stop_every`] once `asked` brings the helper's reply to its request.
async fn stopped_every(
    client: &Client,
    asked: impl std::future::Future<Output = Result<Value>>,
) -> Everyone {
    let reply = match asked.await {
        Ok(reply) if reply["ok"] == true => reply,
        // A helper with no daemon running keeps none. A removed socket does
        // not acknowledge a request already sent.
        Err(error)
            if error.code == Code::Io && !error.uncertain && !client.transport.path.exists() =>
        {
            return Everyone::default();
        }
        Ok(reply) => return Everyone::failed(refused(&reply).error),
        Err(error) => return Everyone::failed(error.error),
    };
    // Only the reply to a stop of every job, with no doubt about it, says
    // what was stopped.
    let all = match JobReply::deserialize(&reply) {
        Ok(JobReply::StopAll(all)) if reply["uncertain"] != true => all,
        _ => return Everyone::failed("the helper did not confirm what it stopped".into()),
    };
    let asked = all.stopping;
    let refused = all.refused.len() as u64;
    let unknown = all.unknown.len() as u64;
    let unresolved = all.unresolved.len() as u64;
    let named: Vec<JobId> = all.unresolved.into_iter().map(|job| job.id).collect();
    let Outstanding { left, unread } = outstanding(asked, &named, || shown_all(client)).await;
    Everyone {
        asked,
        left,
        refused,
        unknown,
        unresolved,
        unread,
        error: None,
    }
}

/// What may still run of the jobs a stop of every job told to stop, once
/// Atlas stopped waiting.
#[derive(Debug, PartialEq, Eq)]
struct Outstanding {
    /// Jobs still running or stopping.
    left: u64,
    /// Records that could not be read, other than those the stop named.
    unread: u64,
}

/// Wait, as long as a stop's grace and KILL take, for the `asked` jobs a
/// stop of every job told to stop to end, looking at every job with `look`.
/// `named` are the records the stop already said it could not read.
async fn outstanding<F, Looked>(asked: u64, named: &[JobId], mut look: F) -> Outstanding
where
    F: FnMut() -> Looked,
    Looked: std::future::Future<Output = Result<Vec<Shown>>>,
{
    // The stop says how many it stops, not which, so a record that cannot
    // be read now, other than those it named, may be of one of them: it is
    // waited for like a job still running.
    let (mut left, mut unread) = (asked, 0);
    let started = tokio::time::Instant::now();
    while left.saturating_add(unread) > 0 && started.elapsed() < JOB_GRACE + BEYOND_GRACE {
        tokio::time::sleep(POLL).await;
        if let Ok(jobs) = look().await {
            left = jobs
                .iter()
                .filter(|job| match job.state() {
                    Some(JobState::Running | JobState::Stopping) => true,
                    Some(JobState::Starting | JobState::Launched | JobState::Ended) | None => false,
                })
                .count() as u64;
            unread = jobs
                .iter()
                .filter(|job| job.state().is_none())
                .filter(|job| job.id().is_none_or(|id| !named.contains(&id)))
                .count() as u64;
        }
    }
    Outstanding { left, unread }
}

/// Single-job reads keep transport failures certain and refusals whole
/// before decoding their operation-specific replies.
async fn read_one(client: &Client, request: &Value) -> Result<Value> {
    let reply = client
        .transport
        .call(request)
        .await
        .map_err(Error::certain)?;
    if reply["ok"] == true {
        Ok(reply)
    } else {
        Err(refused(&reply))
    }
}

/// Job `id` as the helper shows it now. Reading changes nothing.
pub(crate) async fn show(client: &Client, id: &JobId) -> Result<Shown> {
    let reply = read_one(client, &json!({"op": "job", "action": "show", "id": id})).await?;
    Ok(match JobReply::deserialize(&reply) {
        Ok(JobReply::Show(Showing::One { job: Some(shown) })) => shown,
        _ => Shown::Later(reply["job"].clone()),
    })
}

/// The jobs the helper's records give to `task` of host session
/// `session` (every task of it without one) that may still run, for a
/// cleanup to stop.
pub(crate) async fn of_session(
    client: &Client,
    session: &Key,
    task: Option<&Key>,
) -> Result<Vec<JobView>> {
    let jobs = shown_all(client).await?;
    Ok(jobs
        .iter()
        .filter_map(Shown::job)
        .filter(|job| job.state.live())
        .filter(|job| job.session.as_ref() == Some(session))
        .filter(|job| {
            job.task
                .as_ref()
                .is_some_and(|theirs| task.is_none_or(|task| task == theirs))
        })
        .cloned()
        .collect())
}

/// Every job as the helper shows it now, or why it cannot say.
pub(crate) async fn shown_all(client: &Client) -> Result<Vec<Shown>> {
    let daemon = Transport {
        timeout: Duration::from_secs(10),
        ..client.transport.clone()
    };
    let reply = daemon
        .call(&json!({"op": "job", "action": "show"}))
        .await
        .map_err(Error::certain)?;

    shown_from(&reply)
}

fn shown_from(reply: &Value) -> Result<Vec<Shown>> {
    if reply["ok"] != true {
        return Err(refused(reply));
    }
    match JobReply::deserialize(reply) {
        Ok(JobReply::Show(Showing::Every { jobs })) => Ok(jobs),
        _ => Err(Error::new(
            Code::Protocol,
            "The helper did not report its background jobs",
        )),
    }
}

/// The task's record's entry for a job the helper shows as `job`.
fn entry(job: &JobView) -> Option<JobEntry> {
    let ended = job.ended();
    Some(JobEntry {
        id: job.id.clone(),
        name: job.name.clone(),
        started_at: job.started_at,
        expected: job.expected()?,
        state: Lenient::Known(job.state),
        how: ended.map(|ended| Lenient::Known(ended.how)),
        ended_at: ended.map(|ended| ended.ended_at),
        later: Map::new(),
    })
}

/// Note `entry` in the record of the task `owner` names, else of this
/// command's task; nothing without one. An identity the record already
/// keeps for the job is never replaced: [`Noted::Disagrees`] says so.
fn note(client: &Client, owner: Option<&Key>, entry: JobEntry) -> Result<Noted> {
    let store = || client.output_dir().map(Store::at);
    let task = match owner {
        Some(owner) => match store()?.load(owner) {
            Some(record) => Task::recorded(&record),
            None => return Ok(Noted::Yes),
        },
        None => match &client.task {
            Some(task) => task.clone(),
            None => return Ok(Noted::Yes),
        },
    };
    let mut noted = Noted::Yes;
    store()?.update(&task, now_ms(), |record| {
        noted = record.noted_job(entry);
        match noted {
            Noted::Full => Err(Error::new(
                Code::State,
                "This task's record is full of jobs that have not ended",
            )),
            Noted::Yes | Noted::Disagrees => Ok(()),
        }
    })?;
    Ok(noted)
}

/// What a reply says when the helper's record of a job names another
/// identity than the task's record does, which is kept as it was.
const DISAGREES: &str = "The helper's record of this job names another process than the task's record does; the task's record was kept as it was";

/// A helper's refusal of a job request, keeping its code, why and which
/// job, and whether it may have happened anyway.
fn refused(reply: &Value) -> Error {
    let mut error = atlas_core::transport::refusal(reply);
    for field in ["reason", "id", "task", "state"] {
        if let Some(value) = reply.get(field).filter(|value| !value.is_null()) {
            error = error.with(field, value.clone());
        }
    }
    error
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
