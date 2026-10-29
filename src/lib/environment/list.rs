//! `environment list`: every copy into the helper Atlas knows of, from the
//! helper's transactions and the frozen plans here, labelled with the session
//! and task that made each one. It only reads.
use super::{
    model::*,
    reply::{ShownFile, Transactions},
    undo::PlanPhase,
    *,
};
use crate::task::record::Lenient;

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
pub(super) const ASKED: usize = 200;

/// The helper's own list of its transactions, with those rolled back when
/// `all`, and whatever it has of the newest frozen plans in `here`; asked
/// through a request folder of its own that goes once the answer is in.
async fn transactions(
    client: &Client,
    helper: &str,
    all: bool,
    here: &[Frozen],
) -> Result<Transactions> {
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
) -> Result<Transactions> {
    if ids.len() > ASKED {
        return Err(problem("Too many plans were asked about"));
    }
    let malformed = "The helper's list of its copies was malformed";
    let answer = one_off::send(client, helper, None, Request::Transactions { all, ids }).await?;
    let answer = one_off::read(answer, "transactions", malformed)?;
    serde_json::from_value(answer).map_err(|_| Error::new(Code::Protocol, malformed))
}

/// A plan folder beside the shared command, as `list` reports it.
pub(super) struct Frozen {
    id: String,
    /// The plan's files, when it holds a plan for this helper.
    files: Option<Vec<ShownFile>>,
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
            .map(|file| file.target.to_string_lossy().into_owned())
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
                .map(|plan| file_report(&plan).iter().map(ShownFile::from).collect());
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

/// What `list` replies: the helper's transactions, `there`, each in the
/// state its phase (or a command holding its folder) says, then the frozen
/// plans `here` the helper never started, newest first and labelled. Plans
/// rolled back completely are counted as `hidden`, not listed, unless
/// `all`; `more` counts those not shown or not looked at, and `unreadable`
/// those the helper or this side could not read, so an empty list is
/// never taken for proof that nothing was copied.
pub(super) fn listing(here: &[Frozen], there: &Transactions, labels: &Labels, all: bool) -> Value {
    let running = |id: &str| here.iter().any(|plan| plan.id == id && plan.running);
    let (mut hidden, mut unreadable) = (there.hidden, there.unreadable);
    let mut plans: Vec<(u64, Value)> = Vec::new();
    let mut started = Vec::new();

    for transaction in &there.transactions {
        let Lenient::Known(transaction) = transaction else {
            unreadable += 1;
            continue;
        };
        let id = transaction.plan_id.as_str();
        let (Ok(()), Some(phase)) = (validate_id(id), PlanPhase::started(&transaction.phase))
        else {
            unreadable += 1;
            continue;
        };
        started.push(id);
        let state = match phase {
            _ if running(id) => PlanPhase::Running,
            PlanPhase::RolledBack if !all => {
                hidden += 1;
                continue;
            }
            phase => phase,
        };
        let files: Vec<ShownFile> = transaction.files.iter().map(ShownFile::from).collect();
        let entry = json!({"planId": id, "state": state, "updatedAt": transaction.updated_at,
            "files": files});
        plans.push((transaction.updated_at, labels.label(entry, id)));
    }

    for plan in here
        .iter()
        .filter(|plan| !started.contains(&plan.id.as_str()))
    {
        let state = if plan.running {
            PlanPhase::Running
        } else if plan.request {
            PlanPhase::Unknown
        } else {
            PlanPhase::Previewed
        };
        let entry = json!({"planId": plan.id, "state": state, "updatedAt": plan.updated,
            "files": plan.files.clone().unwrap_or_default()});
        plans.push((plan.updated, labels.label(entry, &plan.id)));
    }

    plans.sort_by_key(|(updated, _)| std::cmp::Reverse(*updated));
    let more = u64::try_from(plans.len().saturating_sub(SHOWN)).unwrap_or(u64::MAX) + there.more;
    json!({
        "ok": true,
        "plans": plans.into_iter().take(SHOWN).map(|(_, plan)| plan).collect::<Vec<_>>(),
        "more": more,
        "hidden": hidden,
        "unreadable": unreadable,
    })
}
