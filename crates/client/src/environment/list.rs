//! `environment list`: every copy into the helper Atlas knows of, from the
//! helper's transactions and the frozen plans here, labelled with the session
//! and task that made each one. It only reads.
use super::{
    model::*,
    reply::{ShownFile, Transactions},
    undo::PlanPhase,
    *,
};
use atlas_core::lenient::Lenient;

/// The most plans `list` shows; `more` counts the rest.
pub(super) const SHOWN: usize = 50;

/// Every copy into the helper Atlas knows of, newest first: the helper's
/// transactions joined with the frozen plans here that it never started,
/// labelled with the session and task that made each one. Files are named
/// by their targets, never their contents. Reads only: it takes no control
/// and records nothing.
///
/// # Errors
///
/// Returns private-storage, identity or helper-observation failures rather than dropping
/// unknown transaction ownership.
pub async fn list(client: &Client, all: bool) -> Result<Value> {
    match client.desktop().kind() {
        Kind::User(login) => {
            let helper = crate::user::environment::helper_name(login.uid)?;
            let (here, there) =
                crate::user::environment::list_as(client, login, &helper, all).await?;
            labelled(client, &here, &there, all)
        }
    }
}

/// The listing of `here` and `there`, each labelled with the session and
/// task that made it.
pub(crate) fn labelled(
    client: &Client,
    here: &[Frozen],
    there: &Transactions,
    all: bool,
) -> Result<Value> {
    let work = task::everyone(client.output_dir()?);
    Ok(listing(here, there, &Labels::new(&work, client), all))
}

/// A plan folder beside the shared command, as `list` reports it.
pub(crate) struct Frozen {
    pub(crate) id: String,
    /// The plan's files, when it holds a plan for this helper.
    files: Option<Vec<ShownFile>>,
    /// Whether it keeps a request whose outcome is not known.
    request: bool,
    /// Whether a command holds its lock right now.
    pub(crate) running: bool,
    /// When the folder last changed, in milliseconds since 1970.
    pub(crate) updated: u64,
}

impl Frozen {
    /// A plan folder named `id`: its plan's files when it holds one for
    /// this helper, whether it keeps a request whose outcome is not known,
    /// whether a command holds its lock and when it last changed.
    pub(crate) fn new(
        id: String,
        files: Option<Vec<ShownFile>>,
        request: bool,
        running: bool,
        updated: u64,
    ) -> Self {
        Self {
            id,
            files,
            request,
            running,
            updated,
        }
    }

    /// How far it got, as one the helper never started.
    pub(crate) fn state(&self) -> PlanPhase {
        if self.running {
            PlanPhase::Running
        } else if self.request {
            PlanPhase::Unknown
        } else {
            PlanPhase::Previewed
        }
    }

    /// The targets its plan copies into the helper.
    pub(crate) fn targets(&self) -> Vec<String> {
        self.files
            .iter()
            .flatten()
            .map(|file| file.target.to_string_lossy().into_owned())
            .collect()
    }
}

/// Which session and task made each plan, from their records.
pub(crate) struct Labels<'a> {
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
pub(crate) fn listing(here: &[Frozen], there: &Transactions, labels: &Labels, all: bool) -> Value {
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
        let entry = json!({"planId": plan.id, "state": plan.state(), "updatedAt": plan.updated,
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

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
