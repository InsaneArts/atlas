//! `environment list`: every copy into the helper Atlas knows of, from the
//! helper's transactions and the frozen plans here, labelled with the session
//! and task that made each one. It only reads.
use super::{model::*, *};

/// The most plans `list` shows; `more` counts the rest.
pub(super) const SHOWN: usize = 50;

/// Every copy into the helper Atlas knows of, newest first: the helper's
/// transactions joined with the frozen plans here that it never started,
/// labelled with the session and task that made each one. Files are named
/// by their targets, never their contents. Reads only: it takes no control
/// and records nothing.
pub async fn list(client: &Client, all: bool) -> Result<Value> {
    let helper = helper_name(client.config.helper_uid)?;
    list_as(client, &helper, all).await
}

/// [`list`] for the helper account `helper`.
pub(super) async fn list_as(client: &Client, helper: &str, all: bool) -> Result<Value> {
    let shared = client
        .config
        .identity_probe
        .parent()
        .ok_or_else(|| problem("Missing shared directory"))?;
    let here = frozen(shared, client.config.helper_uid);
    let there = transactions(client, helper, all, &here).await?;
    let work = task::everyone(client.output_dir()?);
    Ok(listing(&here, &there, &Labels::new(&work, client), all))
}

/// The most frozen plans the helper is asked about by ID.
const ASKED: usize = 200;

/// The helper's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of the newest frozen plans in `here`; asked
/// through a request folder of its own that goes once the answer is in.
async fn transactions(client: &Client, helper: &str, all: bool, here: &[Frozen]) -> Result<Value> {
    let mut newest: Vec<&Frozen> = here.iter().collect();
    newest.sort_by_key(|plan| std::cmp::Reverse(plan.updated));
    let ids = newest
        .into_iter()
        .take(ASKED)
        .map(|plan| plan.id.clone())
        .collect();
    ask(client, helper, all, ids).await
}

/// The helper's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of each of `ids` whatever its phase and age.
pub(super) async fn ask(
    client: &Client,
    helper: &str,
    all: bool,
    ids: Vec<String>,
) -> Result<Value> {
    if ids.len() > ASKED {
        return Err(problem("Too many plans were asked about"));
    }
    let id = crate::control::token();
    let stage = stage_path(client, &id)?.with_file_name(format!("environment-run-{id}"));
    create_stage(&stage, helper)?;
    let answer = dispatch(client, helper, &stage, Request::Transactions { all, ids }).await;
    discard(&stage);

    let answer = answer?;
    if answer["ok"] == false {
        return Err(crate::transport::refusal(&answer));
    }
    if answer["ok"] != true || !answer["transactions"].is_array() {
        return Err(Error::new(
            "protocol",
            "The helper's list of its copies was malformed",
        ));
    }
    Ok(answer)
}

/// A plan folder beside the shared command, as `list` reports it.
pub(super) struct Frozen {
    id: String,
    /// The plan's files, when it holds a plan for this helper.
    files: Option<Vec<Value>>,
    /// Whether it keeps a request whose outcome is not known.
    request: bool,
    /// Whether a command holds its lock right now.
    running: bool,
    /// When the folder last changed, in milliseconds since 1970.
    updated: u64,
}

impl Frozen {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn running(&self) -> bool {
        self.running
    }

    pub(super) fn request(&self) -> bool {
        self.request
    }

    /// The targets its plan copies into the helper.
    pub(super) fn targets(&self) -> Vec<String> {
        self.files
            .iter()
            .flatten()
            .filter_map(|file| file["target"].as_str().map(str::to_owned))
            .collect()
    }
}

/// The plan folders beside the shared command in `shared` that hold a plan
/// for helper `uid` or a request whose outcome is unknown.
pub(super) fn frozen(shared: &Path, uid: u32) -> Vec<Frozen> {
    leftovers(shared)
        .into_iter()
        .filter_map(|stage| {
            let name = stage.file_name()?.to_str()?.to_owned();
            let id = name.strip_prefix("environment-plan-")?.to_owned();
            let files = read_limited(&stage.join("plan.json"), MAX_RECORD)
                .ok()
                .and_then(|bytes| decode::<Plan>(&bytes).ok())
                .filter(|plan| plan.id == id && plan.uid == uid)
                .map(|plan| shown_files(&Value::from(file_report(&plan))));
            let request = requests(&stage);
            if files.is_none() && !request {
                return None;
            }
            Some(Frozen {
                running: held(&stage),
                updated: modified_ms(&stage),
                id,
                files,
                request,
            })
        })
        .collect()
}

/// Whether a command holds the lock of the plan folder `stage` right now.
/// It is looked at with a shared lock let go of at once; a command that
/// takes the lock in that instant waits it out.
fn held(stage: &Path) -> bool {
    match open_regular(&stage.join("controller.lock"), false) {
        Ok(Some(lock)) => FileExt::try_lock_shared(&lock).is_err(),
        Ok(None) | Err(_) => false,
    }
}

/// When `path` last changed, in milliseconds since 1970; 0 when unknown.
fn modified_ms(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| u64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

/// Which session and task made each plan, from their records.
pub(super) struct Labels<'a> {
    pub(super) work: &'a [task::Work],
    /// The task of the command listing.
    pub(super) task: Option<&'a task::Task>,
}

impl<'a> Labels<'a> {
    fn new(work: &'a [task::Work], client: &'a Client) -> Self {
        Self {
            work,
            task: client.task.as_ref(),
        }
    }

    /// `entry`, a listed plan, labelled with the session that made plan
    /// `id` (this command's own when it is one of them), or null when no
    /// record claims it, and whether this command's task claims it.
    fn label(&self, mut entry: Value, id: &str) -> Value {
        let claims: Vec<&task::Work> = self
            .work
            .iter()
            .filter(
                |work| matches!(&work.owned, task::Owned::Plan { id: plan } if plan.as_str() == id),
            )
            .collect();
        let own = |work: &task::Work| self.task.is_some_and(|task| work.task == *task.key());
        let session = claims
            .iter()
            .find(|work| own(work))
            .or_else(|| claims.first())
            .map(|work| &work.session);
        entry["session"] = json!(session);
        entry["thisTask"] = json!(claims.iter().any(|work| own(work)));
        entry
    }
}

/// The phases a helper transaction may be in, as it names them.
const PHASES: [&str; 4] = ["applying", "applied", "rollingBack", "rolledBack"];

/// What `list` replies: the helper's transactions, `there`, each in the
/// state its phase (or a command holding its folder) says, then the frozen
/// plans `here` the helper never started, newest first and labelled. Plans
/// rolled back completely are counted as `hidden`, not listed, unless
/// `all`; `more` counts those not shown or not looked at, and `unreadable`
/// those the helper or this side could not read, so an empty list is
/// never taken for proof that nothing was copied.
pub(super) fn listing(here: &[Frozen], there: &Value, labels: &Labels, all: bool) -> Value {
    let running = |id: &str| here.iter().any(|plan| plan.id == id && plan.running);
    let count = |field: &str| there[field].as_u64().unwrap_or(0);
    let (mut hidden, mut unreadable) = (count("hidden"), count("unreadable"));
    let mut plans: Vec<(u64, Value)> = Vec::new();
    let mut started = Vec::new();

    let transactions = there["transactions"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for transaction in transactions {
        let (Some(id), Some(phase)) = (
            transaction["planId"]
                .as_str()
                .filter(|id| validate_id(id).is_ok()),
            transaction["phase"]
                .as_str()
                .filter(|phase| PHASES.contains(phase)),
        ) else {
            unreadable += 1;
            continue;
        };
        started.push(id);
        let state = match phase {
            _ if running(id) => "running",
            "rolledBack" if !all => {
                hidden += 1;
                continue;
            }
            phase => phase,
        };
        let updated = transaction["updatedAt"].as_u64().unwrap_or(0);
        let entry = json!({"planId": id, "state": state, "updatedAt": updated,
            "files": shown_files(&transaction["files"])});
        plans.push((updated, labels.label(entry, id)));
    }

    for plan in here
        .iter()
        .filter(|plan| !started.contains(&plan.id.as_str()))
    {
        let state = if plan.running {
            "running"
        } else if plan.request {
            "unknown"
        } else {
            "previewed"
        };
        let entry = json!({"planId": plan.id, "state": state, "updatedAt": plan.updated,
            "files": plan.files.clone().unwrap_or_default()});
        plans.push((plan.updated, labels.label(entry, &plan.id)));
    }

    plans.sort_by_key(|(updated, _)| std::cmp::Reverse(*updated));
    let more = u64::try_from(plans.len().saturating_sub(SHOWN)).unwrap_or(u64::MAX) + count("more");
    json!({
        "ok": true,
        "plans": plans.into_iter().take(SHOWN).map(|(_, plan)| plan).collect::<Vec<_>>(),
        "more": more,
        "hidden": hidden,
        "unreadable": unreadable,
    })
}

/// Each of `files` (a helper's file report) by its target and operation
/// only: nothing else the helper sends is passed on.
fn shown_files(files: &Value) -> Vec<Value> {
    files
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| json!({"target": file["target"], "operation": file["operation"]}))
        .collect()
}
