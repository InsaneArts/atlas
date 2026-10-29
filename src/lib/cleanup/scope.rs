//! Which work a cleanup covers, read from the records: one session's tasks
//! or one task of it, what they claim, the receipts of earlier cleanups of
//! it, and whether the session may still be working, which keeps an agent
//! from taking over a neighbour's work.
use super::{
    plan::{Collection, Job, Others, Undoable},
    receipt::{Receipt, Receipts},
};
use crate::{
    client::Client,
    error::{Code, Error, Result},
    host_session::{
        Key, now_ms,
        record::{ACTIVE_FOR, Record as SessionRecord},
        store::Store as Sessions,
    },
    task::{
        self, Owned, State, Task,
        record::{CleanupState, Record as TaskRecord},
        store::Store as Tasks,
    },
};
use serde_json::{Value, json};
use std::path::Path;

/// What one cleanup covers: one session's work, or one task's of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    /// The session whose work it is.
    pub session: Key,
    /// The one task it covers; none for every task of the session.
    pub task: Option<Key>,
    /// Whether the work is another session's than the one cleaning up.
    pub adopting: bool,
    /// Whether the session or task was named by its key, so a key that
    /// names nothing is said, never taken for nothing to clean up.
    pub named: bool,
}

impl Scope {
    /// What a command of `client` covers: the session `session` names, or
    /// its own; the task `task` names, or the one ATLAS_TASK names for its
    /// own session, or every task.
    pub fn of(client: &Client, session: Option<&Key>, task: Option<&Key>) -> Result<Self> {
        let own = client.host_session.as_ref().map(|host| host.key());
        let session = session.or(own).cloned().ok_or_else(|| {
            Error::new(
                Code::Usage,
                "Atlas cannot tell which session this is, so it has nothing to clean up. Set \
                 ATLAS_SESSION=NAME before the task's first command, or pass --session KEY \
                 (atlas activity --all lists sessions).",
            )
        })?;
        let own_task = client
            .task
            .as_ref()
            .filter(|own| own.name().is_some() && *own.session() == session)
            .map(|own| own.key().clone());
        Ok(Self {
            adopting: own != Some(&session),
            named: own != Some(&session) || task.is_some(),
            task: task.cloned().or(own_task),
            session,
        })
    }

    /// The person's cleanup of `session`'s work, through Atlas.app.
    pub fn for_person(session: &Key) -> Self {
        Self {
            session: session.clone(),
            task: None,
            adopting: true,
            named: true,
        }
    }
}

/// Who takes over another session's work: an agent's session, or the
/// person through Atlas.app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Adopter {
    Session(Key),
    Person,
}

impl Adopter {
    /// The adopting session's key; none for the person.
    pub fn key(&self) -> Option<Key> {
        match self {
            Self::Session(key) => Some(key.clone()),
            Self::Person => None,
        }
    }
}

/// What a scope's records say: the session's record, its tasks' records,
/// the work in scope, and the receipts of earlier cleanups of the session.
pub(crate) struct Loaded {
    pub record: Option<SessionRecord>,
    pub tasks: Vec<TaskRecord>,
    pub work: Vec<task::Work>,
    pub receipts: Vec<Receipt>,
    /// The one task in scope; none for every task of the session.
    pub task: Option<Key>,
}

impl Loaded {
    /// The records of `scope`'s session in `output`; a key the command
    /// named that names no record is said. A task named by ATLAS_TASK with
    /// no record yet has nothing to clean up.
    pub fn of(output: &Path, scope: &Scope) -> Result<Self> {
        let record = Sessions::at(output).read(&scope.session);
        let tasks = Tasks::at(output).of_session(&scope.session);
        let nothing = || {
            Error::new(
                Code::Input,
                "No session or task with that key is recorded here; atlas activity --all lists them",
            )
        };
        if scope.named && record.is_none() && tasks.is_empty() {
            return Err(nothing());
        }
        if let Some(task) = &scope.task
            && scope.named
            && *task != scope.session
            && !tasks.iter().any(|record| record.key == *task)
        {
            return Err(nothing());
        }

        let work = task::work(record.as_ref(), &tasks)
            .into_iter()
            .filter(|work| scope.task.as_ref().is_none_or(|task| work.task == *task))
            .collect();
        Ok(Self {
            record,
            tasks,
            work,
            receipts: Receipts::at(output).of_session(&scope.session),
            task: scope.task.clone(),
        })
    }

    /// The background jobs the tasks in scope started that may still run,
    /// and the collections they made.
    pub fn others(&self) -> Others {
        let tasks = || {
            self.tasks
                .iter()
                .filter(|record| self.task.as_ref().is_none_or(|task| record.key == *task))
        };
        Others {
            jobs: tasks()
                .flat_map(|record| {
                    record
                        .jobs
                        .iter()
                        .filter(|job| !job.ended())
                        .map(|job| Job {
                            id: job.id.clone(),
                            task: record.key.clone(),
                            name: job.name.clone(),
                        })
                })
                .collect(),
            outputs: tasks()
                .flat_map(|record| {
                    record.outputs.iter().map(|output| Collection {
                        task: record.key.clone(),
                        receipt: output.receipt.clone(),
                    })
                })
                .collect(),
            unlisted: None,
        }
    }

    /// What a cleanup undoes: everything in scope that may still be in the
    /// helper, with when it was claimed. A thing a step of an earlier
    /// cleanup already removed, undid or kept is not planned again, even
    /// while its claim is not yet settled.
    pub fn undoable(&self) -> Vec<Undoable> {
        self.work
            .iter()
            .filter(|work| matches!(work.state, State::Intent | State::Made))
            .filter(|work| !self.finished(&work.owned))
            .map(|work| Undoable {
                owned: work.owned.clone(),
                claimed: self.claimed(work),
            })
            .collect()
    }

    /// Whether a step of an earlier cleanup of the session finished with
    /// `owned`.
    fn finished(&self, owned: &Owned) -> bool {
        self.receipts
            .iter()
            .flat_map(|receipt| &receipt.steps)
            .any(|entry| {
                entry.state.finished() && entry.step.owned().is_some_and(|done| done.same(owned))
            })
    }

    /// When `work` was claimed: as its task's record says, else as the
    /// session's record noted its plan.
    fn claimed(&self, work: &task::Work) -> u64 {
        self.tasks
            .iter()
            .find(|record| record.key == work.task)
            .and_then(|record| record.claimed_at(&work.owned))
            .or_else(|| match &work.owned {
                Owned::Plan { id } => self.record.as_ref().and_then(|record| {
                    record
                        .plans
                        .iter()
                        .find(|plan| plan.plan_id == *id)
                        .map(|plan| plan.at)
                }),
                _ => None,
            })
            .unwrap_or(0)
    }

    /// The apps the scope's tasks opened by name, each once.
    pub fn opened_by_name(&self) -> Vec<&str> {
        let mut apps: Vec<&str> = Vec::new();
        for work in &self.work {
            if let Owned::Launch { app } = &work.owned
                && work.state == State::Made
                && !apps.iter().any(|known| crate::text::same_text(known, app))
            {
                apps.push(app);
            }
        }
        apps
    }

    /// The apps the scope's tasks opened by name that still run, by the
    /// helper's app list `running`: listed, never quit. With no list, each
    /// is listed, as it may still run.
    pub fn left(&self, running: Option<&Value>) -> Vec<Value> {
        let runs = |app: &str| {
            running.is_none_or(|listed| {
                listed["apps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|listed| listed["name"].as_str())
                    .any(|name| crate::text::same_text(name, app))
            })
        };
        self.opened_by_name()
            .into_iter()
            .filter(|app| runs(app))
            .map(|app| json!({"app": app, "why": "opened by name"}))
            .collect()
    }

    /// The newest receipt of `scope` that has not finished: one its tasks'
    /// records name, or one a run cut short before it named it.
    pub fn unfinished(&self, scope: &Scope) -> Option<Receipt> {
        self.receipts
            .iter()
            .filter(|receipt| receipt.task == scope.task && receipt.state != CleanupState::Complete)
            .max_by_key(|receipt| receipt.updated)
            .cloned()
    }

    /// The tasks whose records a cleanup of `scope` notes itself in: each
    /// task its work belongs to, else the task it names or the session's
    /// default task.
    pub fn covered(&self, scope: &Scope) -> Vec<Task> {
        let mut keys: Vec<&Key> = Vec::new();
        for work in &self.work {
            if !keys.contains(&&work.task) {
                keys.push(&work.task);
            }
        }
        if keys.is_empty() {
            keys.push(scope.task.as_ref().unwrap_or(&scope.session));
        }
        keys.into_iter()
            .filter_map(
                |key| match self.tasks.iter().find(|record| record.key == *key) {
                    Some(record) => Some(Task::recorded(record)),
                    None if *key == scope.session => Task::of(&scope.session, None).ok(),
                    None => None,
                },
            )
            .collect()
    }
}

/// Whether the session of `record` may still be working: it acted within
/// the last minutes, or the host process it names still runs.
pub(crate) fn alive(record: &SessionRecord) -> bool {
    record.active(now_ms()) || record.host_process.is_some_and(|process| process.running())
}

/// Whether a task record not yet taken over changed within the last
/// minutes: its session may still be working, even when its session's own
/// record is gone. A cleanup's own notes are written once the work is
/// taken over, so they never count.
fn task_alive(task: &TaskRecord, now: u64) -> bool {
    let within = crate::time::ms(ACTIVE_FOR);
    task.adopted_by.is_none() && now.saturating_sub(task.updated) <= within
}

/// Refuse an agent's cleanup of another session's work while that session
/// may still be working, by its record or its tasks' records.
pub(crate) fn may_adopt(record: Option<&SessionRecord>, tasks: &[TaskRecord]) -> Result<()> {
    let now = now_ms();
    let last = match record.filter(|record| alive(record)) {
        Some(record) => record.last_seen,
        None => match tasks
            .iter()
            .filter(|task| task_alive(task, now))
            .map(|task| task.updated)
            .max()
        {
            Some(updated) => updated,
            None => return Ok(()),
        },
    };
    let minutes = now.saturating_sub(last) / 60_000;
    let ago = match minutes {
        0 => "less than a minute ago".to_owned(),
        1 => "1 minute ago".to_owned(),
        minutes => format!("{minutes} minutes ago"),
    };
    Err(Error::new(
        Code::Input,
        format!("That session acted {ago} and may still be working; ask the person."),
    ))
}
