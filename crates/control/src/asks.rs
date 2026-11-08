//! The questions Atlas asks the person before an agent sends from an account
//! they listed, and what they allowed: kept beside the Stop and the person's
//! control notes, private to this account.
//!
//! An ask is one file, `<id>.json`, in the helper's asks folder: who asks,
//! the account, what would be sent and, for a message, its preview. It lives
//! two minutes; its picture, `<id>-<token>.jpg`, lives as long as it does, and goes
//! too when its agent session ends. The preview lives only while a command waits on the ask: the
//! command takes it off when its wait ends, however it ends, and a rerun
//! that joins the ask writes it again. An ask whose command was killed is
//! taken down at the next read of the folder, and the person's app, which
//! watches the folder, also takes each down when its two minutes end. The person's answer is `<id>.answer`, written only through
//! their viewer. A task grant ("Allow for This Task") is kept in
//! `allowed.json`: an agent session, the host process it ran in, an account
//! and when it ends. Just This Once is claimed by one waiting command alone.
//! Allowed for an action that stopped a request partway, it is a permit in
//! `permits`, which one later request takes, once, within two minutes.
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
    /// The host process that session runs in, when it is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<atlas_core::process::Instance>,
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
    /// Its picture of the helper's screen, a JPEG in the asks folder by
    /// this name, for the person's card alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
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
    /// The host process the session ran in when the person allowed it: the
    /// grant is that process's alone, so a session key resumed in another
    /// gets none of it.
    #[serde(default)]
    pub process: Option<atlas_core::process::Instance>,
    /// The account, as the input service names it: its kind, id and
    /// browser.
    pub account: Value,
    /// The account as the person reads it.
    pub name: String,
    /// When it ends, in milliseconds since 1970.
    pub ends: u64,
}

/// One action the person allowed after it stopped an agent's request: the
/// session and host process that may send it, its account, the digest of
/// its whole payload, where it lands and the digest of the message it
/// sends. One request takes it, once, within two minutes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permit {
    pub session: String,
    #[serde(default)]
    pub process: Option<atlas_core::process::Instance>,
    pub account: Value,
    pub action: String,
    pub target: Value,
    #[serde(default)]
    pub value: Option<String>,
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

    /// The folder the asks are kept in.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Ask `ask`, or, when this session already asks the same request,
    /// join that ask: its id either way.
    ///
    /// # Errors
    ///
    /// Returns file errors when the ask can't be written.
    pub fn ask(&self, mut ask: Ask, now: u64) -> Result<String> {
        // The same question: this session, account, request, target and
        // message. A changed message or target is a question of its own.
        let bind = &ask.question["bind"];
        if let Some(open) = self.open(now).into_iter().find(|open| {
            open.session == ask.session && !bind.is_null() && &open.question["bind"] == bind
        }) {
            // The command joining it waits on it now: its message shows again.
            let message = &ask.question["message"];
            if !message.is_null() {
                self.rewrite(&open.id, |joined| {
                    joined.question["message"] = message.clone();
                })?;
            }
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

    /// The asks still up at `now`, oldest first. Each that ran out by then
    /// is taken down first, message and all, whoever reads.
    #[must_use]
    pub fn open(&self, now: u64) -> Vec<Ask> {
        self.expire(now);
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
    /// Returns `stopped` for an agent, `input` for an ask that is gone or a
    /// choice it doesn't offer, and file errors when the answer can't be
    /// written.
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
        // A choice the question doesn't offer can't be given.
        let offered = match answer {
            Answer::Task => ask.question["task"] != false,
            Answer::Once => ask.question["once"] != false,
            Answer::Deny => true,
        };
        if !offered {
            return Err(Error::new(
                Code::Input,
                "That choice isn't offered for this question",
            ));
        }
        if answer == Answer::Task {
            self.allow_task(&ask, now)?;
        }
        // Answered, the card goes, and with it its picture, whether or not a
        // command still waits.
        let _locked = self.lock()?;
        let path = self.folder.join(format!("{id}.answer"));
        atlas_core::files::save_private(&path, serde_json::to_string(&answer)?.as_bytes())?;
        if let Some(mut answered) = self.read(id)
            && answered.picture.take().is_some()
        {
            atlas_core::files::save_private(
                &self.folder.join(format!("{id}.json")),
                &serde_json::to_vec(&answered)?,
            )?;
        }
        self.remove_pictures(id, None);
        Ok(())
    }

    /// The answer to ask `id`, once the person gave one.
    #[must_use]
    pub fn answered(&self, id: &str) -> Option<Answer> {
        let bytes = fs::read(self.folder.join(format!("{id}.answer"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Take Just This Once, the answer to ask `id`, for one waiter alone:
    /// under the asks' lock, the ask and its answer are read and taken down
    /// together, so of every command joined to it, across processes, only
    /// the first to claim it gets it. None when another claimed it first,
    /// or it isn't answered that way.
    ///
    /// # Errors
    ///
    /// Returns file errors when the lock can't be taken.
    pub fn claim_once(&self, id: &str, now: u64) -> Result<Option<Ask>> {
        let _locked = self.lock()?;
        if self.answered(id) != Some(Answer::Once) {
            return Ok(None);
        }
        let claimed = self.read(id).filter(|ask| ask.ends > now);
        self.remove(id);
        Ok(claimed)
    }

    /// Take the message preview off ask `id`: its waiting command's wait
    /// ended. The ask stays up for a rerun to join.
    ///
    /// # Errors
    ///
    /// Returns file errors; the ask then still ends with its two minutes.
    pub fn forget_preview(&self, id: &str) -> Result<()> {
        self.rewrite(id, |ask| {
            if let Some(message) = ask.question["message"].as_object_mut() {
                message.remove("preview");
            }
        })
    }

    /// Change ask `id` as `change` says, under the asks' lock, when it is
    /// still up: one taken down meanwhile is never written back.
    fn rewrite(&self, id: &str, change: impl FnOnce(&mut Ask)) -> Result<()> {
        let _locked = self.lock()?;
        let Some(mut ask) = self.read(id) else {
            return Ok(());
        };
        change(&mut ask);
        let path = self.folder.join(format!("{id}.json"));
        atlas_core::files::save_private(&path, &serde_json::to_vec(&ask)?)
    }

    /// Ask `id` as it is on disk.
    fn read(&self, id: &str) -> Option<Ask> {
        let bytes = fs::read(self.folder.join(format!("{id}.json"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// The asks' lock, held across a change that reads an ask and writes or
    /// removes it, so none undoes another.
    fn lock(&self) -> Result<atlas_core::files::LockFile> {
        locked(&self.folder.join("asks.lock"))
    }

    /// Keep `permit` for the one request that sends its action, for two
    /// minutes at most.
    ///
    /// # Errors
    ///
    /// Returns file errors when it can't be kept.
    pub fn permit(&self, mut permit: Permit, now: u64) -> Result<()> {
        permit.ends = permit.ends.min(now + millis(LIFE));
        let _locked = self.lock()?;
        let mut permits = self.permits(now);
        permits.push(permit);
        self.save_permits(&permits)
    }

    /// Take, for one request alone, the permits of `session` in host
    /// `process` for any of the actions whose digests are `actions`: they
    /// are gone from the store once taken, whatever the request comes to.
    ///
    /// # Errors
    ///
    /// Returns file errors; nothing is taken then.
    pub fn take_permits(
        &self,
        session: &str,
        process: Option<atlas_core::process::Instance>,
        actions: &[String],
        now: u64,
    ) -> Result<Vec<Permit>> {
        let _locked = self.lock()?;
        let (taken, kept): (Vec<Permit>, Vec<Permit>) =
            self.permits(now).into_iter().partition(|permit| {
                permit.session == session
                    && permit.process == process
                    && actions.contains(&permit.action)
            });
        if !taken.is_empty() {
            self.save_permits(&kept)?;
        }
        Ok(taken)
    }

    /// Keep `permits` again: the request that took them sent nothing.
    ///
    /// # Errors
    ///
    /// Returns file errors; they are spent then.
    pub fn return_permits(&self, permits: Vec<Permit>, now: u64) -> Result<()> {
        if permits.is_empty() {
            return Ok(());
        }
        let _locked = self.lock()?;
        let mut kept = self.permits(now);
        kept.extend(permits.into_iter().filter(|permit| permit.ends > now));
        self.save_permits(&kept)
    }

    /// The permits of `session` in host `process` still in force at `now`,
    /// left in the store.
    #[must_use]
    pub fn permits_of(
        &self,
        session: &str,
        process: Option<atlas_core::process::Instance>,
        now: u64,
    ) -> Vec<Permit> {
        self.permits(now)
            .into_iter()
            .filter(|permit| permit.session == session && permit.process == process)
            .collect()
    }

    /// The permits still in force at `now`.
    fn permits(&self, now: u64) -> Vec<Permit> {
        fs::read(self.folder.join("permits"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Permit>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|permit| permit.ends > now)
            .collect()
    }

    fn save_permits(&self, permits: &[Permit]) -> Result<()> {
        self.make_folder()?;
        atlas_core::files::save_private(&self.folder.join("permits"), &serde_json::to_vec(permits)?)
    }

    /// Keep `jpeg` as ask `id`'s picture, private, with `frame`, where its
    /// element is in it in the picture's pixels, and the picture's size in
    /// them, all at once, while the ask is up and unanswered:
    /// it goes with the ask, however that ends, and with the person's
    /// answer.
    ///
    /// # Errors
    ///
    /// Returns file errors; the ask then has no picture.
    pub fn keep_picture(
        &self,
        id: &str,
        jpeg: &[u8],
        frame: &Value,
        (width, height): (u32, u32),
    ) -> Result<()> {
        let _locked = self.lock()?;
        // Read once the lock is held: a wait for it may outlast the ask.
        let now = atlas_core::time::now_ms();
        let Some(mut ask) = self.read(id) else {
            return Ok(());
        };
        // One that ran out by now goes, and gets none.
        if ask.ends <= now {
            self.remove(id);
            return Ok(());
        }
        // One the person answered has no card left to show it on.
        if self.answered(id).is_some() {
            return Ok(());
        }
        // Each picture has a name of its own, never written over: a reader
        // of the question finds the picture and the frame it names together,
        // and the one before stays until nothing names it.
        let name = format!("{id}-{}.jpg", atlas_core::files::token());
        let path = self.folder.join(&name);
        atlas_core::files::save_private(&path, jpeg)?;
        let before = ask.picture.replace(name.clone());
        ask.question["frame"] = frame.clone();
        ask.question["pictureSize"] = serde_json::json!({"width": width, "height": height});
        let published = atlas_core::files::save_private(
            &self.folder.join(format!("{id}.json")),
            &serde_json::to_vec(&ask)?,
        );
        if published.is_err() {
            // The question still names the one before, with its frame.
            let _ = fs::remove_file(&path);
            return published;
        }
        if before.is_some() {
            self.remove_pictures(id, Some(&name));
        }
        Ok(())
    }

    /// Remove ask `id`'s pictures but `keep`, whatever names they have.
    fn remove_pictures(&self, id: &str, keep: Option<&str>) {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return;
        };
        let prefix = format!("{id}-");
        // A write cut short leaves its temporary file, `.<name>.<token>`.
        let cut = format!(".{id}-");
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let its = (name.ends_with(".jpg")
                && (name.starts_with(&prefix) || name == format!("{id}.jpg")))
                || (name.starts_with(&cut) && name.contains(".jpg."));
            if its && Some(name) != keep {
                // One that can't be removed now goes at its ask's next cleanup.
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Take down every ask that `lives` says no longer has an agent session
    /// waiting on it: its session or host process ended.
    pub fn close_unless(&self, now: u64, lives: impl Fn(&Ask) -> bool) {
        for ask in self.open(now) {
            if !lives(&ask) {
                self.close(&ask.id);
            }
        }
    }

    /// Take ask `id` down, with its answer and picture.
    pub fn close(&self, id: &str) {
        // Taken down either way; the lock only keeps a rewrite from
        // bringing it back.
        let _locked = self.lock().ok();
        self.remove(id);
    }

    fn remove(&self, id: &str) {
        for ending in ["json", "answer"] {
            let _ = fs::remove_file(self.folder.join(format!("{id}.{ending}")));
        }
        self.remove_pictures(id, None);
    }

    /// Take down every ask whose time ran out by `now`.
    pub fn expire(&self, now: u64) {
        let Ok(entries) = fs::read_dir(&self.folder) else {
            return;
        };
        let _locked = self.lock().ok();
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
                && let Ok(bytes) = fs::read(&path)
                && serde_json::from_slice::<Ask>(&bytes).is_ok_and(|ask| ask.ends <= now)
                && let Some(id) = path.file_stem().and_then(|stem| stem.to_str())
            {
                self.remove(id);
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

    /// Allow the session that asked `ask` the rest of its task in its
    /// account, in the host process it runs in, for an hour at most.
    fn allow_task(&self, ask: &Ask, now: u64) -> Result<()> {
        let account = &ask.question["accountKey"];
        let _locked = self.lock_allowed()?;
        let mut allowed = self.allowed(now);
        allowed.retain(|grant| !(grant.session == ask.session && &grant.account == account));
        allowed.push(Allowed {
            session: ask.session.clone(),
            process: ask.process,
            account: account.clone(),
            name: ask.account.clone(),
            ends: now + millis(TASK_LIFE),
        });
        self.save_allowed(&allowed)
    }

    /// End every task grant that `lives` says no longer holds: its agent
    /// session or host process ended.
    ///
    /// # Errors
    ///
    /// Returns file errors.
    pub fn end_unless(&self, now: u64, lives: impl Fn(&Allowed) -> bool) -> Result<()> {
        let _locked = self.lock_allowed()?;
        let mut allowed = self.allowed(now);
        let before = allowed.len();
        allowed.retain(|grant| lives(grant));
        if allowed.len() == before {
            return Ok(());
        }
        self.save_allowed(&allowed)
    }

    /// The grants' lock, held across each read, change and write of them,
    /// so an End is never undone by another change at once.
    fn lock_allowed(&self) -> Result<atlas_core::files::LockFile> {
        locked(&self.allowed.with_extension("lock"))
    }

    /// End every task grant in `account` (End in the panel), or every one of
    /// `session` (its agent session ended); the person only.
    ///
    /// # Errors
    ///
    /// Returns `stopped` for an agent, and file errors.
    pub fn end(
        &self,
        account: Option<&Value>,
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
        let _locked = self.lock_allowed()?;
        let mut allowed = self.allowed(now);
        allowed.retain(|grant| {
            account.is_none_or(|account| &grant.account != account)
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
            // What ran out meanwhile is taken down at each wake.
            self.expire(atlas_core::time::now_ms());
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

/// The lock at `path`, held exclusively until it is dropped.
fn locked(path: &Path) -> Result<atlas_core::files::LockFile> {
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|error| Error::writing(parent, error))?;
    }
    let lock = atlas_core::files::open_lock(path)?;
    fs2::FileExt::lock_exclusive(&lock.0).map_err(|error| Error::writing(path, error))?;
    Ok(lock)
}

/// `duration` in whole milliseconds.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
