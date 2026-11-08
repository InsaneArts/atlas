//! The questions Atlas asks the person before an agent sends from an account
//! they listed, and what they allowed: kept beside the Stop and the person's
//! control notes, private to this account.
//!
//! An ask is one file, `<id>.json`, in the helper's asks folder: who asks,
//! the account, what would be sent and, for a message, its preview. It lives
//! two minutes. The person's answer is `<id>.answer`, written only through
//! their viewer. A task grant ("Allow for This Task") is kept in
//! `allowed.json`: an agent session, an account and when it ends.
//!
//! The session key a task grant names becomes an authority here: it is
//! verified by the host's process ancestry, but an agent with the person's
//! own rights can still fake it, as it can write these files. Like Stop,
//! this holds against agents using Atlas's commands, not a shell that
//! edits Atlas's files.
use atlas_core::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    os::{fd::AsRawFd, unix::fs::DirBuilderExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long an ask stays up unanswered.
pub const LIFE: Duration = Duration::from_secs(120);
/// The longest Allow for This Task lasts.
pub const TASK_LIFE: Duration = Duration::from_secs(3600);

/// The asks of one helper.
#[derive(Debug, Clone)]
pub struct Asks {
    folder: PathBuf,
    allowed: PathBuf,
    stop: PathBuf,
}

/// One question to the person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub id: String,
    /// The agent session asking, by its key.
    pub session: String,
    /// Who asks, as the person reads it, such as "Claude Code · release".
    pub who: String,
    /// The task, when the agent named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The account, as the person reads it.
    pub account: String,
    /// What would be sent, as the input service read it: what it would do,
    /// the element, the key, how much text, and the request's digest.
    pub question: Value,
    /// When it was asked and when it ends, in milliseconds since 1970.
    pub asked: u64,
    pub ends: u64,
}

/// What the person answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    /// Allow for This Task: the rest of this session in this account.
    Task,
    /// Just This Once: this very request.
    Once,
    Deny,
}

/// What a task grant allows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowed {
    pub session: String,
    pub account: String,
    /// When it ends, in milliseconds since 1970.
    pub ends: u64,
}

/// How a wait for the person ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    Answered(Answer),
    /// Its time ran out with no answer; the ask stays up.
    Unanswered,
    /// The person stopped agents meanwhile.
    Stopped,
}

impl Asks {
    /// The asks kept in `folder`, the task grants in `allowed`, and the
    /// Stop marker at `stop`, which ends a wait.
    pub fn at(folder: PathBuf, allowed: PathBuf, stop: PathBuf) -> Self {
        Self {
            folder,
            allowed,
            stop,
        }
    }

    /// Ask `ask`, or, when this session already asks the same request,
    /// join that ask: its id either way.
    ///
    /// # Errors
    ///
    /// Returns file errors when the ask can't be written.
    pub fn ask(&self, mut ask: Ask, now: u64) -> Result<String> {
        self.expire(now);
        let batch = &ask.question["batch"];
        if let Some(open) = self
            .open(now)
            .into_iter()
            .find(|open| open.session == ask.session && &open.question["batch"] == batch)
        {
            return Ok(open.id);
        }
        ask.id = atlas_core::files::token();
        ask.asked = now;
        ask.ends = now + millis(LIFE);
        self.make_folder()?;
        let path = self.folder.join(format!("{}.json", ask.id));
        atlas_core::files::save_private(&path, &serde_json::to_vec(&ask)?)?;
        Ok(ask.id)
    }

    /// The asks still up at `now`, oldest first.
    #[must_use]
    pub fn open(&self, now: u64) -> Vec<Ask> {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        let mut asks: Vec<Ask> = entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            })
            .filter_map(|entry| serde_json::from_slice(&fs::read(entry.path()).ok()?).ok())
            .filter(|ask: &Ask| ask.ends > now)
            .collect();
        asks.sort_by_key(|ask| ask.asked);
        asks
    }

    /// The person's answer to ask `id`; the person only, through their
    /// viewer. An ask that is gone can't be answered.
    ///
    /// # Errors
    ///
    /// Returns `stopped` for an agent, `input` for an ask that is gone, and
    /// file errors when the answer can't be written.
    pub fn answer(&self, id: &str, answer: Answer, by_person: bool, now: u64) -> Result<()> {
        if !by_person {
            return Err(Error::new(
                Code::Stopped,
                "Only the person answers Atlas's questions, in the Atlas menu bar or the viewer",
            ));
        }
        let Some(ask) = self.open(now).into_iter().find(|ask| ask.id == id) else {
            return Err(Error::new(Code::Input, "That question is no longer open"));
        };
        if answer == Answer::Task {
            self.allow_task(&ask.session, &ask.account, now)?;
        }
        let path = self.folder.join(format!("{id}.answer"));
        atlas_core::files::save_private(&path, serde_json::to_string(&answer)?.as_bytes())
    }

    /// The answer to ask `id`, once the person gave one.
    #[must_use]
    pub fn answered(&self, id: &str) -> Option<Answer> {
        let bytes = fs::read(self.folder.join(format!("{id}.answer"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Take ask `id` down, with its answer.
    pub fn close(&self, id: &str) {
        for ending in ["json", "answer"] {
            let _ = fs::remove_file(self.folder.join(format!("{id}.{ending}")));
        }
    }

    /// Take down every ask whose time ran out by `now`.
    pub fn expire(&self, now: u64) {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
                && let Ok(bytes) = fs::read(&path)
                && serde_json::from_slice::<Ask>(&bytes).is_ok_and(|ask| ask.ends <= now)
                && let Some(id) = path.file_stem().and_then(|stem| stem.to_str())
            {
                self.close(id);
            }
        }
    }

    /// The task grants still in force at `now`.
    #[must_use]
    pub fn allowed(&self, now: u64) -> Vec<Allowed> {
        fs::read(&self.allowed)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Allowed>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|allowed| allowed.ends > now)
            .collect()
    }

    /// Allow `session` the rest of its task in `account`, for an hour at most.
    fn allow_task(&self, session: &str, account: &str, now: u64) -> Result<()> {
        let mut allowed = self.allowed(now);
        allowed.retain(|grant| !(grant.session == session && grant.account == account));
        allowed.push(Allowed {
            session: session.to_owned(),
            account: account.to_owned(),
            ends: now + millis(TASK_LIFE),
        });
        self.save_allowed(&allowed)
    }

    /// End every task grant in `account` (End in the panel), or every one of
    /// `session` (its agent session ended); the person only.
    ///
    /// # Errors
    ///
    /// Returns `stopped` for an agent, and file errors.
    pub fn end(
        &self,
        account: Option<&str>,
        session: Option<&str>,
        by_person: bool,
        now: u64,
    ) -> Result<()> {
        if !by_person {
            return Err(Error::new(
                Code::Stopped,
                "Only the person ends what they allowed, in the Atlas menu bar",
            ));
        }
        let mut allowed = self.allowed(now);
        allowed.retain(|grant| {
            account.is_none_or(|account| grant.account != account)
                && session.is_none_or(|session| grant.session != session)
        });
        self.save_allowed(&allowed)
    }

    fn save_allowed(&self, allowed: &[Allowed]) -> Result<()> {
        atlas_core::files::save_private(&self.allowed, &serde_json::to_vec(allowed)?)
    }

    fn make_folder(&self) -> Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.folder)
            .map_err(|error| Error::writing(&self.folder, error))
    }

    /// Wait until the person answers ask `id`, Stop is pressed, or `until`.
    /// It sleeps on the asks folder and the Stop marker's folder, woken by
    /// any change there; nothing is checked on a timer.
    ///
    /// # Errors
    ///
    /// Returns an error when the folders can't be watched.
    pub fn wait(&self, id: &str, until: Instant) -> Result<Waited> {
        use nix::sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue};

        self.make_folder()?;
        let stop_folder = self.stop.parent().unwrap_or(Path::new("/"));
        let watched = [fs::File::open(&self.folder)?, fs::File::open(stop_folder)?];
        let queue = Kqueue::new().map_err(std::io::Error::from)?;
        let changes: Vec<KEvent> = watched
            .iter()
            .map(|folder| {
                KEvent::new(
                    usize::try_from(folder.as_raw_fd()).unwrap_or_default(),
                    EventFilter::EVFILT_VNODE,
                    EvFlags::EV_ADD | EvFlags::EV_CLEAR,
                    FilterFlag::NOTE_WRITE,
                    0,
                    0,
                )
            })
            .collect();
        queue
            .kevent(
                &changes,
                &mut [],
                Some(nix::libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                }),
            )
            .map_err(std::io::Error::from)?;
        loop {
            if let Some(answer) = self.answered(id) {
                return Ok(Waited::Answered(answer));
            }
            if matches!(self.stop.try_exists(), Ok(true)) {
                return Ok(Waited::Stopped);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(Waited::Unanswered);
            }
            let timeout = nix::libc::timespec {
                tv_sec: i64::try_from(left.as_secs()).unwrap_or(i64::MAX),
                tv_nsec: i64::from(left.subsec_nanos()),
            };
            let mut events = [KEvent::new(
                0,
                EventFilter::EVFILT_VNODE,
                EvFlags::empty(),
                FilterFlag::empty(),
                0,
                0,
            ); 2];
            match queue.kevent(&[], &mut events, Some(timeout)) {
                Ok(_) | Err(nix::errno::Errno::EINTR) => {}
                Err(error) => return Err(Error::from(std::io::Error::from(error))),
            }
        }
    }
}

/// `duration` in whole milliseconds.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
