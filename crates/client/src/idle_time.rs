//! Whether the idle time apps read has stopped while the person is at the
//! Mac, and putting it right where this login can.
//!
//! Apps read one record kept for every login as the person's idle time:
//! password managers lock on it, chat apps mark the person away. While an
//! app in the login on the screen has secure typing on, the window server
//! copies input into that record on a short timer tied to a login, and a
//! switch between logins, as to and from the helper, can lose that timer.
//! The record then stops until secure typing next turns off in the login on
//! the screen, so the person's apps think they are away while they type
//! (see [`atlas_native::native::idle`]).
//!
//! The login's own idle time and the hardware's do not go through that
//! copy, so the record has stopped when both show input and it does not,
//! and still does not once the timer would have copied it. When no app
//! keeps secure typing on, turning it on and off once from this login
//! restarts the record. While one does, only that app letting go restarts
//! it, so the person is told which app it is.
use crate::console_user::Login;
use atlas_core::files::{self, Lock};
use atlas_native::native::{
    apps,
    idle::{self, Idle, SecureTyping},
};
use serde::Serialize;
use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock, TryLockError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Input this recent counts as the person at the Mac.
const PRESENT: f64 = 5.0;
/// The idle time apps read this far behind the hardware's has stopped: far
/// more than the half second it trails while secure typing is on, and less
/// than any app's idle lock.
const STOPPED: f64 = 30.0;
/// How long a working record can take to catch up with input: the window
/// server copies it every half second while secure typing is on.
const CATCH_UP: Duration = Duration::from_secs(1);
/// How often Atlas tries to restart the record, at most, across every
/// process of the account.
const RESET_EVERY: Duration = Duration::from_secs(60);
/// The login window's bundle identifier: it keeps secure typing on behind
/// the lock screen.
const LOGIN_WINDOW: &str = "com.apple.loginwindow";

/// What Atlas finds of the idle time apps read in this login.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum IdleTime {
    /// It follows the person's input.
    Live,
    /// It has stopped while the person is at the Mac, and no app the
    /// person can quit keeps it stopped.
    Stuck,
    /// It has stopped while the person is at the Mac because an app keeps
    /// secure typing on: only that app letting go restarts it, so the
    /// person can quit it.
    Held {
        /// The name the app shows, when it has one.
        app: Option<String>,
    },
    /// It could not be read, this login is not the one on the screen, or
    /// the person has not used the Mac in the last few seconds: none shows
    /// whether it moves.
    Unknown,
}

/// What a look may do besides looking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Look {
    /// Only look.
    Only,
    /// When the record has stopped and no app keeps secure typing on, turn
    /// secure typing on and off once and look again, while `Leave` still
    /// holds at that moment: at most once a minute for the whole account,
    /// never two at once (see [`restart_when_due`]). Only the service that
    /// keeps the person's status gives leave.
    AndRestart(Leave),
}

/// Leave to restart the record, which its giver can take back at any time.
/// The restart itself is made under it: taking it back waits for one under
/// way, a moment, and no restart starts once it is back.
#[derive(Clone, Debug, Default)]
pub struct Leave(Arc<RwLock<bool>>);

impl Leave {
    /// Give it, until [`Leave::take_back`].
    pub fn give(&self) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// Take it back, from every copy of it, once a restart under way ends.
    pub fn take_back(&self) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = false;
    }

    /// Whether it holds now, without waiting: while it is being given or
    /// taken back, it does not.
    pub fn holds(&self) -> bool {
        match self.0.try_read() {
            Ok(held) => *held,
            Err(TryLockError::Poisoned(held)) => *held.into_inner(),
            Err(TryLockError::WouldBlock) => false,
        }
    }

    /// Run `act` while it holds, keeping it from being taken back until
    /// `act` ends; whether it ran.
    fn while_held(&self, act: impl FnOnce()) -> bool {
        let held = self.0.read().unwrap_or_else(PoisonError::into_inner);
        if *held {
            act();
        }
        *held
    }
}

impl PartialEq for Leave {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Leave {}

impl IdleTime {
    /// As it is now in the login this process runs in. A record that looks
    /// stopped is looked at again a second later, so the call can take that
    /// long; a caller that must not wait bounds it.
    pub fn now(look: Look) -> Self {
        self::look(&Real, look)
    }
}

/// What a look reads of the Mac and does to it.
trait Mac {
    /// The login this process runs in.
    fn login(&self) -> Option<Login>;
    /// The idle times as they are now.
    fn idle(&self) -> Idle;
    /// Wait while the window server catches up.
    fn wait(&self, time: Duration);
    /// Whether an app keeps secure typing on in this login.
    fn secure_typing(&self) -> Option<SecureTyping>;
    /// Turn secure typing on and off once, under `leave`, when the
    /// account's allowance has come and `ready`, asked once the allowance
    /// is held, still says so; whether it did.
    fn restart_when_due(&self, ready: &mut dyn FnMut() -> bool, leave: &Leave) -> bool;
    /// The process `pid`, as the person sees it.
    fn holder(&self, pid: i64) -> Holder;
}

/// The Mac this process runs on.
struct Real;

impl Mac for Real {
    fn login(&self) -> Option<Login> {
        Login::this()
    }

    fn idle(&self) -> Idle {
        Idle::now()
    }

    fn wait(&self, time: Duration) {
        std::thread::sleep(time);
    }

    fn secure_typing(&self) -> Option<SecureTyping> {
        SecureTyping::now()
    }

    fn restart_when_due(&self, ready: &mut dyn FnMut() -> bool, leave: &Leave) -> bool {
        allowance().is_some_and(|path| {
            restart_when_due(&path, SystemTime::now, ready, || {
                leave.while_held(idle::reset_secure_typing)
            })
        })
    }

    fn holder(&self, pid: i64) -> Holder {
        Holder {
            login_window: apps::bundle_id(pid).as_deref() == Some(LOGIN_WINDOW),
            name: apps::name(pid),
        }
    }
}

/// The look on `mac`, restarting the record when `look` asks and the
/// account's allowance has come.
fn look(mac: &impl Mac, look: Look) -> IdleTime {
    let Some(login) = mac.login().filter(|login| login.on_screen) else {
        return IdleTime::Unknown;
    };
    match stopped(&mac.idle()) {
        None => return IdleTime::Unknown,
        Some(false) => return IdleTime::Live,
        Some(true) => {}
    }
    // Input the moment before this look may not be copied yet.
    mac.wait(CATCH_UP);
    if mac.login() != Some(login) {
        return IdleTime::Unknown;
    }
    match stopped(&mac.idle()) {
        None => return IdleTime::Unknown,
        Some(false) => return IdleTime::Live,
        Some(true) => {}
    }
    let Some(mut secure) = mac.secure_typing() else {
        return IdleTime::Unknown;
    };
    if let Look::AndRestart(leave) = &look
        && secure == SecureTyping::Off
    {
        // The restart acts on the login on the screen while no app keeps
        // secure typing on, so both are read again just before it, once the
        // allowance is held; the leave is asked at the restart itself.
        let mut moved = false;
        let mut ready = || {
            let still =
                mac.login() == Some(login) && mac.secure_typing() == Some(SecureTyping::Off);
            moved = !still;
            still
        };
        let restarted = mac.restart_when_due(&mut ready, leave);
        if moved {
            return IdleTime::Unknown;
        }
        if !restarted {
            return stopped_by(None);
        }
        match stopped(&mac.idle()) {
            None => return IdleTime::Unknown,
            Some(false) => return IdleTime::Live,
            Some(true) => {}
        }
        let Some(after) = mac.secure_typing() else {
            return IdleTime::Unknown;
        };
        secure = after;
    }
    stopped_by(match secure {
        SecureTyping::Off => None,
        SecureTyping::HeldBy(pid) => Some(mac.holder(pid)),
    })
}

/// The process keeping secure typing on, as the person sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Holder {
    /// Whether it is the login window.
    login_window: bool,
    /// The name it shows, when it has one.
    name: Option<String>,
}

/// A stopped record kept stopped by `holder`, or by nothing when it is
/// `None`. The person cannot quit the login window.
fn stopped_by(holder: Option<Holder>) -> IdleTime {
    match holder {
        Some(holder) if !holder.login_window => IdleTime::Held { app: holder.name },
        _ => IdleTime::Stuck,
    }
}

/// Whether the idle time apps read has stopped while the person is at the
/// Mac: the hardware and this login both saw input a moment ago, and the
/// record apps read did not. `None` when the hardware's is unknown, or
/// when either saw no input in the last few seconds, since then the record
/// is not expected to move.
fn stopped(idle: &Idle) -> Option<bool> {
    let hardware = idle.hardware?;
    if hardware > PRESENT || idle.login > PRESENT {
        return None;
    }
    Some(idle.shared - hardware >= STOPPED)
}

/// The folder where every process of this account keeps the record of its
/// last restart, with Atlas's other records in the account's home.
fn allowance() -> Option<PathBuf> {
    let home = atlas_core::config::home().ok()?;
    Some(home.join("Library/Application Support/Atlas"))
}

/// The lock every restart is made under, in the allowance's folder.
const LOCK: &str = "idle-restart.lock";
/// When the last restart was made, in nanoseconds since 1970, in the
/// allowance's folder; always replaced whole.
const LAST: &str = "idle-restart.time";

/// Restart the record with `reset`, which says whether it made one, when the allowance kept in `folder` has
/// come and `ready`, asked right before the restart, still says so. The
/// lock is held from before the clock is read until the restart is noted,
/// so no two processes restart at once or within a minute of each other.
/// Each note replaces the last whole, so it reads as the old time or the
/// new one; a lock already held, or a note that is there but cannot be
/// read, means no restart. Whether it restarted.
fn restart_when_due(
    folder: &Path,
    clock: impl Fn() -> SystemTime,
    ready: &mut dyn FnMut() -> bool,
    reset: impl FnOnce() -> bool,
) -> bool {
    // The folder is made private when missing, as Atlas's other records.
    if std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
        .is_err()
    {
        return false;
    }
    let Ok(lock) = files::open_lock(&folder.join(LOCK)) else {
        return false;
    };
    if files::lock_within(&lock, Lock::Exclusive, Duration::ZERO).is_err() {
        return false;
    }
    let path = folder.join(LAST);
    let noted = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return false,
        Ok(_) => {
            let Some(last) = files::read_own(&path, 64)
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|text| text.trim().parse().ok())
            else {
                return false;
            };
            Some(last)
        }
    };
    let last = noted.map(|nanoseconds| UNIX_EPOCH + Duration::from_nanos(nanoseconds));
    // A restart that could not be noted first is not made, so the
    // allowance holds whatever happens next.
    if !due(last, clock()) || note(&path, clock()).is_err() {
        return false;
    }
    if !ready() {
        // Best effort: the note before comes back, as no restart was made;
        // left as it is, it only delays the next one.
        let _ = match noted {
            Some(nanoseconds) => files::save_private(&path, nanoseconds.to_string().as_bytes()),
            None => std::fs::remove_file(&path).map_err(atlas_core::error::Error::from),
        };
        return false;
    }
    // A restart refused at the last moment leaves the minute noted before.
    if !reset() {
        return false;
    }
    // The minute counts from the restart itself, however long the steps
    // before it took. When that cannot be noted, the lock is kept for the
    // minute instead, so no other process restarts within it.
    if note(&path, clock()).is_err() {
        std::thread::sleep(RESET_EVERY);
    }
    true
}

/// Note `at` at `path`, in nanoseconds since 1970, replacing the last note
/// whole.
fn note(path: &Path, at: SystemTime) -> atlas_core::error::Result<()> {
    let nanoseconds = at
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_nanos()).ok())
        .ok_or_else(|| {
            atlas_core::error::Error::new(
                atlas_core::error::Code::State,
                "a time before 1970 or past 2554",
            )
        })?;
    files::save_private(path, nanoseconds.to_string().as_bytes())
}

/// Whether a restart may be made at `now`, the last having been made at
/// `last`: a minute after it, or once the clock reads a minute or more
/// before it, as after it was set back.
fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.is_none_or(|at| match now.duration_since(at) {
        Ok(since) => since >= RESET_EVERY,
        Err(back) => back.duration() >= RESET_EVERY,
    })
}

#[cfg(test)]
#[path = "idle_time_tests.rs"]
mod tests;
