//! What a host session's record says of it, and what it left in the helper.
//! Every name a record keeps is typed as Atlas writes it, so a record that
//! holds anything else does not read at all.
use super::{Host, HostId, HostSession, Instance, Key};
use crate::{environment::Phase, files::Token, helper_info::Capture};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

/// How long after its last command a session counts as active.
pub(crate) const ACTIVE_FOR: Duration = Duration::from_secs(10 * 60);
/// How long a record's last seen time may lag a command that records
/// nothing else before that command writes it again.
const SEEN_WITHIN: Duration = Duration::from_secs(30);
/// The record format this build writes.
const VERSION: u32 = 1;
/// The most screenshot names, copy tokens, plans and verbs a record keeps.
const SCREENSHOTS: usize = 50;
const COPIES: usize = 64;
const PLANS: usize = 32;
const COUNTS: usize = 32;

/// What a session's record says of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    pub version: u32,
    pub key: Key,
    pub host: Host,
    /// The host's own id, so the person can find the conversation.
    pub host_session: HostId,
    /// The host's process, where the host names it (Claude Code).
    #[serde(default)]
    pub host_process: Option<Instance>,
    /// The project folder's name, never its path.
    #[serde(default)]
    pub folder: Option<String>,
    /// The session's first recorded Atlas command, in milliseconds since
    /// 1970; not when the host started it.
    pub started: u64,
    /// Its last Atlas command, in milliseconds since 1970: within
    /// [`SEEN_WITHIN`] for one that only looked.
    pub last_seen: u64,
    #[serde(default)]
    pub ended: Option<u64>,
    /// The session this one followed in the same host process (Claude
    /// Code's `/clear`).
    #[serde(default)]
    pub previous: Option<Key>,
    /// How many of each action it did.
    #[serde(default)]
    pub counts: BTreeMap<String, u64>,
    /// Its screenshots' file names, newest first.
    #[serde(default)]
    pub screenshots: Vec<Capture>,
    /// The tokens of the copy folders `open` made for it.
    #[serde(default)]
    pub copies: Vec<Token>,
    /// The environment plans it applied and nobody has rolled back.
    #[serde(default)]
    pub plans: Vec<PlanNote>,
}

impl Record {
    /// A new record of `session`, first seen `now`, working in `folder`.
    pub fn new(session: &HostSession, now: u64, folder: Option<String>) -> Self {
        Self {
            version: VERSION,
            key: session.key().clone(),
            host: session.host(),
            host_session: session.id().clone(),
            host_process: session.process(),
            folder,
            started: now,
            last_seen: now,
            ended: None,
            previous: None,
            counts: BTreeMap::new(),
            screenshots: Vec::new(),
            copies: Vec::new(),
            plans: Vec::new(),
        }
    }

    /// When the session ended: as recorded, or at its last command once
    /// the host process it names no longer runs.
    pub fn ended_at(&self) -> Option<u64> {
        self.ended.or_else(|| {
            self.host_process
                .filter(|process| !process.running())
                .map(|_| self.last_seen)
        })
    }

    /// Whether the session has not ended and ran a command within
    /// [`ACTIVE_FOR`] of `now`.
    pub fn active(&self, now: u64) -> bool {
        let within = crate::time::ms(ACTIVE_FOR);
        self.ended_at().is_none() && now.saturating_sub(self.last_seen) <= within
    }

    /// Note that `session` acted `now`, doing `verb` when its command was
    /// not refused.
    pub fn seen(&mut self, session: &HostSession, now: u64, verb: Option<&str>) {
        self.last_seen = now;
        self.ended = None;
        if session.process().is_some() {
            self.host_process = session.process();
        }
        if let Some(verb) = verb
            && (self.counts.len() < COUNTS || self.counts.contains_key(verb))
        {
            *self.counts.entry(verb.to_owned()).or_default() += 1;
        }
    }

    /// Note a command of this session `now` that records nothing else,
    /// `ancestor` being its host's process when that is one of the
    /// command's own ancestors. A process other than the one named (a
    /// resumed conversation) takes its place and undoes any end. Otherwise
    /// only the last seen time moves, at most every [`SEEN_WITHIN`], and an
    /// ended session stays ended: a command's word alone, as an id in its
    /// environment, never revives one. Whether anything changed.
    pub fn touched(&mut self, ancestor: Option<Instance>, now: u64) -> bool {
        if let Some(process) = ancestor.filter(|process| self.host_process != Some(*process)) {
            self.host_process = Some(process);
            self.ended = None;
            self.last_seen = now;
            return true;
        }

        let within = crate::time::ms(SEEN_WITHIN);
        if self.ended.is_some() || now.saturating_sub(self.last_seen) <= within {
            return false;
        }
        self.last_seen = now;
        true
    }

    /// Note the screenshot saved as `capture`, newest first.
    pub fn took(&mut self, capture: Capture) {
        self.screenshots.retain(|kept| *kept != capture);
        self.screenshots.insert(0, capture);
        self.screenshots.truncate(SCREENSHOTS);
    }

    /// Note the copy folders `tokens` name, made for this session.
    pub fn copied(&mut self, tokens: impl IntoIterator<Item = Token>) {
        for token in tokens {
            if !self.copies.contains(&token) {
                self.copies.insert(0, token);
            }
        }
        self.copies.truncate(COPIES);
    }

    /// Forget the copy folders `tokens` name, which are gone.
    pub fn removed(&mut self, tokens: &[Token]) {
        self.copies.retain(|token| !tokens.contains(token));
    }

    /// Note that this session's change took plan `plan_id` to `phase` `at`;
    /// a rolled-back plan leaves the record.
    pub fn planned(&mut self, plan_id: Token, phase: Phase, at: u64) {
        self.forget_plan(&plan_id);
        if phase != Phase::RolledBack {
            self.plans.insert(0, PlanNote { plan_id, phase, at });
            self.plans.truncate(PLANS);
        }
    }

    /// Forget plan `plan_id`, rolled back by whoever; whether it was here.
    pub fn forget_plan(&mut self, plan_id: &Token) -> bool {
        let before = self.plans.len();
        self.plans.retain(|plan| plan.plan_id != *plan_id);
        self.plans.len() != before
    }
}

/// An environment plan a session applied, as its last reply left it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanNote {
    pub plan_id: Token,
    pub phase: Phase,
    /// When a reply said so, in milliseconds since 1970.
    pub at: u64,
}

/// What a session left in the helper, worked out when read: nothing of it
/// is stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Leftovers {
    /// The tokens of its copy folders still there.
    pub copies: Vec<Token>,
    /// Its applied plans not yet rolled back.
    pub plans: usize,
    /// Its pending changes no live process is carrying out.
    pub uncertain: usize,
    /// Its pending changes a live process is still carrying out.
    pub running: usize,
}

impl Leftovers {
    pub fn is_empty(&self) -> bool {
        self.copies.is_empty() && self.plans == 0 && self.uncertain == 0 && self.running == 0
    }
}

/// A record as status reads share it: with how many of its copy folders
/// were still there when it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub record: Record,
    pub copies_left: usize,
}
