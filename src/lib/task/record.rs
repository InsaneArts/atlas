//! What a task's record says it made in the helper. A record keeps Atlas's
//! own names only (tokens, plan IDs, app names), never a file's name or
//! path. What a newer build writes that this one does not know is kept as
//! it was on every rewrite, so a record never loses a claim to an older
//! build.
use super::{Name, Task};
use crate::{
    control::Token,
    error::{Error, Result},
    host_session::Key,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The record format this build writes. A record of a later version is
/// read, never rewritten.
pub(crate) const VERSION: u32 = 1;
/// The most entries `owned` keeps.
pub(crate) const OWNED: usize = 256;
/// The most cleanups a record keeps note of, unfinished ones aside.
const CLEANUPS: usize = 16;

/// What a task's record says of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    pub version: u32,
    pub key: Key,
    pub session: Key,
    /// The ATLAS_TASK name; none for a session's default task.
    pub name: Option<Name>,
    /// When the task's first claim was written, in milliseconds since 1970.
    pub started: u64,
    pub updated: u64,
    /// What the task made, oldest first.
    #[serde(default)]
    pub owned: Vec<Entry>,
    /// The cleanups that covered this task's work, oldest first. Written
    /// only when there is one, so a record without stays as earlier builds
    /// wrote it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleanups: Vec<CleanupNote>,
    /// Who took over this task's work after its session ended, when
    /// someone did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted_by: Option<Adoption>,
    /// Fields a later build keeps here, as it wrote them.
    #[serde(flatten)]
    pub later: Map<String, Value>,
}

impl Record {
    /// A new record of `task`, first written `now`.
    pub fn new(task: &Task, now: u64) -> Self {
        Self {
            version: VERSION,
            key: task.key().clone(),
            session: task.session().clone(),
            name: task.name().cloned(),
            started: now,
            updated: now,
            owned: Vec::new(),
            cleanups: Vec::new(),
            adopted_by: None,
            later: Map::new(),
        }
    }

    /// Note that the task is about to make `owned`, or made it, `now`. A
    /// thing already listed takes the new state where it is. When the list
    /// is full, room is made by forgetting the oldest thing that is gone,
    /// then the oldest app opened by name; with neither, the claim is
    /// refused.
    pub fn claim(&mut self, owned: &Owned, state: State, now: u64) -> Result<()> {
        if self.settle(owned, state, now) {
            return Ok(());
        }
        if self.owned.len() >= OWNED {
            let finished = self.oldest(|claim| claim.state == State::Gone);
            let listed =
                || self.oldest(|claim| matches!(claim.owned(), Some(Owned::Launch { .. })));
            let Some(place) = finished.or_else(listed) else {
                return Err(Error::new(
                    "state",
                    "This task's record is full of things it has not cleaned up",
                ));
            };
            self.owned.remove(place);
        }

        self.owned.push(Entry::Known(Claim::new(owned, state, now)));
        Ok(())
    }

    /// Note what became of `owned` `now`; whether this record lists it.
    pub fn settle(&mut self, owned: &Owned, state: State, now: u64) -> bool {
        let claim = self.owned.iter_mut().find_map(|entry| match entry {
            Entry::Known(claim) if claim.owned().is_some_and(|known| known.same(owned)) => {
                Some(claim)
            }
            _ => None,
        });
        let Some(claim) = claim else {
            return false;
        };
        claim.state = state;
        claim.at = now;
        true
    }

    /// Everything this record claims that this build understands, with its
    /// state.
    pub fn claims(&self) -> impl Iterator<Item = (Owned, State)> + '_ {
        self.owned.iter().filter_map(|entry| match entry {
            Entry::Known(claim) => claim.owned().map(|owned| (owned, claim.state)),
            Entry::Later(_) => None,
        })
    }

    /// When the claim of `owned` was made or last settled, when this record
    /// lists it.
    pub fn claimed_at(&self, owned: &Owned) -> Option<u64> {
        self.owned.iter().find_map(|entry| match entry {
            Entry::Known(claim) if claim.owned().is_some_and(|known| known.same(owned)) => {
                Some(claim.at)
            }
            _ => None,
        })
    }

    /// Whether anything this record lists may still be in the helper or
    /// still be under way: a claim not yet gone or kept, a cleanup that has
    /// not finished, or anything a later build wrote that this one cannot
    /// read (a field of the record or of a claim, or a whole claim), which
    /// it never takes for finished.
    pub fn holds_anything(&self) -> bool {
        !self.later.is_empty()
            || self.unfinished_cleanup().is_some()
            || self.owned.iter().any(|entry| match entry {
                Entry::Known(claim) => {
                    !claim.later.is_empty()
                        || claim.owned().is_none()
                        || matches!(claim.state, State::Intent | State::Made)
                }
                Entry::Later(_) => true,
            })
    }

    /// The newest cleanup of this task's work that has not finished.
    pub fn unfinished_cleanup(&self) -> Option<&CleanupNote> {
        self.cleanups
            .iter()
            .filter(|note| note.state != CleanupState::Complete)
            .max_by_key(|note| note.updated)
    }

    /// Note that cleanup `id` covered this task's work and got to `state`
    /// `now`. The oldest finished notes go first once there are more than
    /// [`CLEANUPS`]; unfinished ones are never forgotten.
    pub fn noted_cleanup(&mut self, id: &Token, state: CleanupState, now: u64) {
        match self.cleanups.iter_mut().find(|note| note.id == *id) {
            Some(note) => {
                note.state = state;
                note.updated = now;
            }
            None => self.cleanups.push(CleanupNote {
                id: id.clone(),
                state,
                updated: now,
            }),
        }
        while self.cleanups.len() > CLEANUPS {
            let Some(oldest) = self
                .cleanups
                .iter()
                .position(|note| note.state == CleanupState::Complete)
            else {
                break;
            };
            self.cleanups.remove(oldest);
        }
    }

    /// Where the oldest claim `which` picks is.
    fn oldest(&self, which: impl Fn(&Claim) -> bool) -> Option<usize> {
        self.owned
            .iter()
            .enumerate()
            .filter_map(|(place, entry)| match entry {
                Entry::Known(claim) if which(claim) => Some((claim.at, place)),
                _ => None,
            })
            .min()
            .map(|(_, place)| place)
    }
}

/// Something a task made that a cleanup may undo or list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Owned {
    /// A folder under the copy root, by its token.
    Copy { id: Token },
    /// An environment plan, by its ID.
    Plan { id: Token },
    /// An app installed into the helper's Applications folder, by its
    /// install ID, with the app's name for listings only.
    Install { id: Token, app: String },
    /// An app opened by name: listed, never quit.
    Launch { app: String },
}

impl Owned {
    /// Whether `self` and `other` are the same thing: one copy, plan or
    /// install by its ID, one app by its name in any case.
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Copy { id }, Self::Copy { id: other }) => id == other,
            (Self::Plan { id }, Self::Plan { id: other }) => id == other,
            (Self::Install { id, .. }, Self::Install { id: other, .. }) => id == other,
            (Self::Launch { app }, Self::Launch { app: other }) => {
                crate::text::same_text(app, other)
            }
            _ => false,
        }
    }
}

/// How far a claimed thing got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    /// Written before the thing is made, so a crash right after leaves it
    /// findable.
    Intent,
    Made,
    /// Removed, or found missing.
    Gone,
    /// Left on purpose by whatever undid the rest.
    Kept,
}

/// One entry of `owned`: one this build reads, or one a later build wrote
/// (another kind, or a state this build does not know), kept as it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Entry {
    Known(Claim),
    Later(Value),
}

/// A claim this build reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Claim {
    kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<Token>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app: Option<String>,
    /// When it was claimed or last settled, in milliseconds since 1970.
    at: u64,
    state: State,
    /// Fields a later build keeps on a claim, as it wrote them.
    #[serde(flatten)]
    later: Map<String, Value>,
}

impl Claim {
    fn new(owned: &Owned, state: State, at: u64) -> Self {
        let (kind, id, app) = match owned {
            Owned::Copy { id } => (Kind::Copy, Some(id.clone()), None),
            Owned::Plan { id } => (Kind::Plan, Some(id.clone()), None),
            Owned::Install { id, app } => (Kind::Install, Some(id.clone()), Some(app.clone())),
            Owned::Launch { app } => (Kind::Launch, None, Some(app.clone())),
        };
        Self {
            kind,
            id,
            app,
            at,
            state,
            later: Map::new(),
        }
    }

    /// The thing claimed, when the claim names it as its kind should.
    fn owned(&self) -> Option<Owned> {
        match (self.kind, &self.id, &self.app) {
            (Kind::Copy, Some(id), None) => Some(Owned::Copy { id: id.clone() }),
            (Kind::Plan, Some(id), None) => Some(Owned::Plan { id: id.clone() }),
            (Kind::Install, Some(id), Some(app)) => Some(Owned::Install {
                id: id.clone(),
                app: app.clone(),
            }),
            (Kind::Launch, None, Some(app)) => Some(Owned::Launch { app: app.clone() }),
            _ => None,
        }
    }
}

/// The kinds of thing a claim may name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    Copy,
    Plan,
    Install,
    Launch,
}

/// How far a cleanup of a task's work got, as its receipt says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CleanupState {
    /// Every step done, or kept on purpose.
    Complete,
    /// Some step failed or has not run yet.
    Incomplete,
    /// A Stop ended it between steps.
    Blocked,
    /// A step's outcome is not known.
    Uncertain,
}

/// A cleanup that covered a task's work: its receipt's ID and state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CleanupNote {
    pub id: Token,
    pub state: CleanupState,
    /// When its receipt last changed, in milliseconds since 1970.
    pub updated: u64,
}

/// Who took over a task's work after its session ended: the adopting
/// session's key (none for the person, through Atlas.app), and when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Adoption {
    pub key: Option<Key>,
    pub at: u64,
}
