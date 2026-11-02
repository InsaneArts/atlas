//! Which task of an agent's session a command belongs to, and the record of
//! what that task made in the helper, so it can be found and cleaned up
//! later. A task is a label: it never decides who may act, what is pending
//! or whether something may be removed.
pub(crate) mod prune;
pub mod record;
pub mod store;

pub use record::{Owned, State};

use crate::{
    client::Client,
    host_session::{
        HostSession, Key,
        record::Record as SessionRecord,
        store::{ReplyFacts, Store as Sessions},
    },
};
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    name::name_type,
    time::now_ms,
};
use std::{ffi::OsStr, path::Path};
use store::Store;
#[cfg(test)]
use {crate::host_session::store::Kind, serde_json::Value};

/// The most bytes a task name keeps.
const NAME_LIMIT: usize = 64;

/// What names a task of the session a command comes from.
const VARIABLE: &str = "ATLAS_TASK";

name_type! {
    /// A task's name, as ATLAS_TASK gives it: 1 to [`NAME_LIMIT`] bytes of
    /// ASCII letters, digits, `.`, `_`, `:` and `-`, so no other value is
    /// ever kept or shown.
    pub struct Name;
    refused(_value) = "not a plain task name".to_owned();
    display
}

impl Name {
    /// `value` as a task name, when it is a plain one.
    fn parse(value: &str) -> Option<Self> {
        atlas_core::name::plain(value, NAME_LIMIT).then(|| Self(value.to_owned()))
    }
}

/// The task a command belongs to: its session's default task, or one the
/// agent named with ATLAS_TASK.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// The host session's key.
    session: Key,
    /// The session's key for its default task; otherwise drawn from the
    /// session's key and the name. The only form activity rows carry.
    key: Key,
    /// The name ATLAS_TASK gave, if any.
    name: Option<Name>,
}

impl Task {
    /// The task of `session` that `name` (ATLAS_TASK's value) names: the
    /// session's default task when it is unset or empty. A name that is not
    /// a plain [`Name`] is a usage error, so an agent that meant to split
    /// its work never finds it silently merged.
    ///
    /// # Errors
    ///
    /// Returns an error when `name` can't name a task.
    pub fn of(session: &Key, name: Option<&str>) -> Result<Self> {
        let name = name
            .filter(|name| !name.is_empty())
            .map(|name| Name::parse(name).ok_or_else(not_plain))
            .transpose()?;
        let key = match &name {
            None => session.clone(),
            Some(name) => Key::digest(&format!("task:{session}:{name}")),
        };

        Ok(Self {
            session: session.clone(),
            key,
            name,
        })
    }

    /// The task `name` names of a command from `session`; none without a
    /// host session, whatever the name: ATLAS_TASK alone makes no session.
    /// A name that is not text is no plainer than any other: a usage error.
    ///
    /// # Errors
    ///
    /// Rejects a non-plain task name when a host session supplies task ownership. This resolves
    /// identity without acknowledging or changing helper work.
    pub fn resolve(session: Option<&HostSession>, name: Option<&OsStr>) -> Result<Option<Self>> {
        let Some(session) = session else {
            return Ok(None);
        };
        let name = name
            .map(|name| name.to_str().ok_or_else(not_plain))
            .transpose()?;
        Self::of(session.key(), name).map(Some)
    }

    /// The task `record` is the record of, as it was named.
    pub(crate) fn recorded(record: &record::Record) -> Self {
        Self {
            session: record.session.clone(),
            key: record.key.clone(),
            name: record.name.clone(),
        }
    }

    /// The key of the host session the task belongs to.
    pub(crate) fn session(&self) -> &Key {
        &self.session
    }

    /// The task's own key: its session's for the default task.
    pub(crate) fn key(&self) -> &Key {
        &self.key
    }

    /// The name ATLAS_TASK gave the task; none for the default task.
    pub fn name(&self) -> Option<&Name> {
        self.name.as_ref()
    }
}

/// Why an ATLAS_TASK value was refused.
fn not_plain() -> Error {
    Error::new(
        Code::Usage,
        format!(
            "{VARIABLE} must be 1 to {NAME_LIMIT} letters, digits, '.', '_', ':' or '-', so \
             Atlas did nothing. Set it to a short plain name, or unset it."
        ),
    )
}

/// Where one command's claims go: its task's record, or nowhere for a
/// command without a task. Every claim is best effort: what it fails to
/// write, the session's record, `close` and the transaction list still
/// find by their own means.
pub struct Claims {
    task: Option<(Task, Store)>,
}

impl Claims {
    /// The claims of `client`'s command.
    pub(crate) fn of(client: &Client) -> Self {
        Self {
            task: client
                .task
                .clone()
                .zip(client.output_dir().ok().map(Store::at)),
        }
    }

    /// The claims of `task`, kept in `store`: written there or refused,
    /// never dropped for want of a task.
    pub fn in_store(store: Store, task: Task) -> Self {
        Self {
            task: Some((task, store)),
        }
    }

    /// Record that the task is about to make `owned` (`intent`), or has
    /// made it (`made`), written through to the disk before the caller goes
    /// on.
    ///
    /// # Errors
    ///
    /// Returns the failure to write the record, or a refusal when it is full.
    pub fn claim(&self, owned: &Owned, state: State) -> Result<()> {
        self.claim_all(std::slice::from_ref(owned), state)
    }

    /// [`Claims::claim`] for each of `owned`, in one write. When the record
    /// is full, the claims that fit are kept and the rest are refused.
    pub(crate) fn claim_all(&self, owned: &[Owned], state: State) -> Result<()> {
        let Some((task, store)) = self.task.as_ref().filter(|_| !owned.is_empty()) else {
            return Ok(());
        };
        let now = now_ms();
        let mut result = Ok(());
        store.update(task, now, |record| {
            result = owned
                .iter()
                .try_for_each(|owned| record.claim(owned, state, now));
            Ok(())
        })?;
        result
    }

    /// Record what became of each of `settled`, which this task claimed, in
    /// one write.
    pub(crate) fn settle_all(&self, settled: &[(Owned, State)]) -> Result<()> {
        let Some((task, store)) = self.task.as_ref().filter(|_| !settled.is_empty()) else {
            return Ok(());
        };
        let now = now_ms();
        store.update(task, now, |record| {
            for (owned, state) in settled {
                record.settle(owned, *state, now);
            }
            Ok(())
        })
    }
}

/// Note, in every task's record that claims it, the plan a rollback in
/// `result`, to a command of `kind`, undid: `gone` when nothing was kept,
/// `kept` otherwise. Whoever rolled it back (the person at a terminal,
/// another session or this one), the plan's own state is what changed. It
/// decides nothing about the rollback itself.
#[cfg(test)]
pub(crate) fn settle_rolled_back(
    client: &Client,
    kind: Kind,
    result: &Result<Value>,
) -> Result<()> {
    settle_rolled_back_facts(client, &ReplyFacts::new(kind, result))
}

/// Settle the plan and kept outcome decoded by `dispatch::note_outcome`.
///
/// # Errors
///
/// Returns task-record lock or write failures while settling a confirmed rollback. A
/// storage failure is not successful settlement.
pub fn settle_rolled_back_facts(client: &Client, facts: &ReplyFacts) -> Result<()> {
    let Some((plan_id, state)) = facts.rolled_back() else {
        return Ok(());
    };
    Store::at(client.output_dir()?).settle_everywhere(
        &[Owned::Plan {
            id: plan_id.clone(),
        }],
        state,
        now_ms(),
    )
}

/// Note, in every task's record that claims one, that each of `owned` is
/// gone, whoever removed it. A record that cannot be written changes
/// nothing: the things are gone either way.
pub(crate) fn settle_gone(client: &Client, owned: &[Owned]) {
    if !owned.is_empty()
        && let Ok(output) = client.output_dir()
    {
        let _ = Store::at(output).settle_everywhere(owned, State::Gone, now_ms());
    }
}

/// [`settle_gone`] for the copy folders `tokens` name.
pub(crate) fn settle_copies_gone(client: &Client, tokens: Vec<Token>) {
    let owned: Vec<Owned> = tokens.into_iter().map(|id| Owned::Copy { id }).collect();
    settle_gone(client, &owned);
}

/// One thing a session's work holds, and the session and task it belongs
/// to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Work {
    pub(crate) session: Key,
    pub(crate) task: Key,
    pub(crate) owned: Owned,
    pub(crate) state: State,
}

/// A session's work: the claims of `tasks` (its tasks' records), and what
/// `session` (its own record, when there still is one) lists that no task
/// claims, which belongs to its default task: made by a build that kept no
/// task records. Each thing is listed once for each task that claims it.
pub(crate) fn work(session: Option<&SessionRecord>, tasks: &[record::Record]) -> Vec<Work> {
    let mut work: Vec<Work> = tasks
        .iter()
        .flat_map(|record| {
            record.claims().map(|(owned, state)| Work {
                session: record.session.clone(),
                task: record.key.clone(),
                owned,
                state,
            })
        })
        .collect();

    let Some(session) = session else {
        return work;
    };
    let recorded: Vec<Owned> = session
        .copies
        .iter()
        .map(|token| Owned::Copy { id: token.clone() })
        .chain(session.plans.iter().map(|plan| Owned::Plan {
            id: plan.plan_id.clone(),
        }))
        .collect();
    for owned in recorded {
        if !work.iter().any(|known| known.owned.same(&owned)) {
            work.push(Work {
                session: session.key.clone(),
                task: session.key.clone(),
                owned,
                state: State::Made,
            });
        }
    }
    work
}

/// Every session's work the records in `output` name: each session's
/// record joined with its tasks' records, and the tasks whose session's
/// record is gone. Only records are read; nothing they name is looked at.
pub(crate) fn everyone(output: &Path) -> Vec<Work> {
    let mut tasks = Store::at(output).all();
    let mut everyone = Vec::new();
    for session in Sessions::at(output).all() {
        let (own, others) = tasks
            .into_iter()
            .partition::<Vec<_>, _>(|task| task.session == session.key);
        everyone.extend(work(Some(&session), &own));
        tasks = others;
    }
    everyone.extend(work(None, &tasks));
    everyone
}

#[cfg(test)]
mod tests;
