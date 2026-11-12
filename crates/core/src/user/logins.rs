use crate::{account::FIRST_USER_UID, session::Absence};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Why the silent helper `uid` is down, if this side can tell. It asks
/// directory services and the process list, so call it off the async
/// runtime.
pub fn absence(uid: u32) -> Option<Absence> {
    match crate::account::account_with_uid(uid) {
        Ok(None) => Some(Absence::Missing),
        Ok(Some(account)) => {
            (logged_in(uid) == Some(false)).then_some(Absence::LoggedOut(account.name))
        }
        Err(_) => None,
    }
}

/// One process, as `ps` lists it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Process {
    pub(super) uid: u32,
    pub(super) pid: u32,
    pub(super) running: Duration,
    pub(super) command: String,
}

/// Every process on this Mac, or `None` when they could not be listed.
pub(super) fn processes() -> Option<Vec<Process>> {
    let output = crate::bounded::output(
        std::process::Command::new("/bin/ps").args(["-axo", "uid=,pid=,etime=,comm="]),
        crate::bounded::LOOKUP,
    )
    .ok()?;

    if !output.status.success() {
        return None;
    }
    parse_processes(std::str::from_utf8(&output.stdout).ok()?)
}

/// `ps -o uid=,pid=,etime=,comm=` lines; the command is the rest of a line.
pub(super) fn parse_processes(listing: &str) -> Option<Vec<Process>> {
    let mut processes = Vec::new();
    for line in listing.lines() {
        let mut words = line.split_whitespace();
        let uid = words.next()?.parse::<i64>().ok()?;
        // Negative system IDs cannot belong to a helper session.
        if uid < 0 {
            continue;
        }
        let uid = u32::try_from(uid).ok()?;
        let pid = words.next()?.parse().ok()?;
        let running = running_for(words.next()?)?;
        let command = words.collect::<Vec<_>>().join(" ");
        if command.is_empty() {
            return None;
        }
        processes.push(Process {
            uid,
            pid,
            running,
            command,
        });
    }
    Some(processes)
}

/// Whether `uid` is logged in to a desktop (its Dock runs); `None` when the
/// process list could not be read, which is never "logged out".
pub fn logged_in(uid: u32) -> Option<bool> {
    Some(runs_as(&processes()?, uid, "/Dock"))
}

/// Whether `uid` is fully logged out: neither its loginwindow nor its Dock
/// runs. A Dock alone is not enough, since launchd restarts it in a login
/// that lives on. `None` when the process list could not be read.
pub fn logged_out(uid: u32) -> Option<bool> {
    Some(logged_out_in(&processes()?, uid))
}

pub(super) fn logged_out_in(listed: &[Process], uid: u32) -> bool {
    !runs_as(listed, uid, "/loginwindow") && !runs_as(listed, uid, "/Dock")
}

/// Whether `uid`'s desktop login runs: its loginwindow and its Dock both.
/// `None` when the process list could not be read.
pub fn desktop_runs(uid: u32) -> Option<bool> {
    let listed = processes()?;
    Some(runs_as(&listed, uid, "/loginwindow") && runs_as(&listed, uid, "/Dock"))
}

/// Whether a process of `uid` whose command ends with `name` runs.
pub(super) fn runs_as(listed: &[Process], uid: u32, name: &str) -> bool {
    listed
        .iter()
        .any(|process| process.uid == uid && process.command.ends_with(name))
}

/// Whether macOS is still updating an account for its login: its
/// UserAccountUpdater runs, as root and as the account.
pub(super) fn updating(listed: &[Process]) -> bool {
    listed
        .iter()
        .any(|process| process.command.ends_with("/UserAccountUpdater"))
}

/// Whether `uid`'s desktop is up: macOS done updating the account, Setup
/// Assistant gone, and the Dock and the Finder running (loginwindow starts
/// the Dock only once Setup Assistant is done). `None` when processes could
/// not be listed.
pub fn desktop_up(uid: u32) -> Option<bool> {
    Some(desktop_up_in(&processes()?, uid))
}

pub(super) fn desktop_up_in(listed: &[Process], uid: u32) -> bool {
    !updating(listed)
        && !runs_as(listed, uid, "/Setup Assistant")
        && runs_as(listed, uid, "/Dock")
        && runs_as(listed, uid, "/Finder")
}

/// Whether Setup Assistant runs for `uid` now; `None` when processes could
/// not be listed.
pub fn setup_assistant(uid: u32) -> Option<bool> {
    Some(runs_as(&processes()?, uid, "/Setup Assistant"))
}

/// The desktop login of `uid`, if it has one.
pub fn login(uid: u32) -> Option<Window> {
    loginwindows_in(&processes()?)
        .into_iter()
        .find(|window| window.uid == uid)
}

/// One desktop login as the process list shows it: its loginwindow, which
/// starts when the person logs in and lasts until they log out (unlike the
/// Dock, which may restart), the user ID that runs it, and how long it has
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub uid: u32,
    pub pid: u32,
    pub age: Duration,
}

/// What directory services say of one user ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Record {
    /// Found by `getpwuid`, which asks every directory macOS uses.
    pub(crate) passwd: bool,
    /// Found by a search of every directory (`dscl /Search`), network and
    /// mobile accounts included. Searched only when `getpwuid` found none.
    pub(crate) directory: bool,
    /// How long ago the account was made, when that is known.
    pub(crate) age: Option<Duration>,
}

impl Record {
    /// Whether any directory has an account with this user ID.
    pub(crate) fn exists(&self) -> bool {
        self.passwd || self.directory
    }
}

/// Whose a desktop login is, and so whether Atlas may ever close it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The account asking; never acted on.
    Mine,
    /// The helper's own login.
    Helper,
    /// A deleted account's login still running under a user ID the helper
    /// was given afterwards: older than the helper's account itself.
    HelperLeftover,
    /// The login of an account that no directory has any more.
    Orphan,
    /// Another person's login, or one on screen; never acted on.
    OtherPerson,
    /// The login window itself, run by a system user.
    Picker,
}

impl Kind {
    /// Whether this is a deleted account's login, which Atlas may close.
    fn leftover(self) -> bool {
        match self {
            Self::Orphan | Self::HelperLeftover => true,
            Self::Mine | Self::Helper | Self::OtherPerson | Self::Picker => false,
        }
    }
}

/// How much older than its account a login must be to have outlived an
/// earlier account: a login cannot start before its account exists, and a
/// few seconds cover clock rounding.
const REUSED_SLACK: Duration = Duration::from_secs(10);

/// Each login in `windows` with whose it is, given what directory services
/// say of each user ID (`accounts`), this account (`me`), the helper (or
/// the account about to be made it), and who owns the screen (`console`).
pub(crate) fn classify(
    windows: &[Window],
    accounts: impl Fn(u32) -> Record,
    me: u32,
    helper: Option<u32>,
    console: Option<u32>,
) -> Vec<(Window, Kind)> {
    windows
        .iter()
        .map(|window| {
            let kind = if window.uid < FIRST_USER_UID {
                Kind::Picker
            } else if window.uid == me {
                Kind::Mine
            } else {
                let record = accounts(window.uid);
                let helper = Some(window.uid) == helper;
                let reused = record
                    .age
                    .is_some_and(|age| window.age >= age + REUSED_SLACK);

                if !record.exists() && Some(window.uid) != console {
                    Kind::Orphan
                } else if helper && reused && Some(window.uid) != console {
                    Kind::HelperLeftover
                } else if helper {
                    Kind::Helper
                } else {
                    Kind::OtherPerson
                }
            };

            (*window, kind)
        })
        .collect()
}

/// Why a new login could hang now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Blocker {
    /// A deleted account is still logged in.
    Orphan { uid: u32 },
    /// A deleted account is still logged in under the helper's user ID.
    HelperLeftover { uid: u32 },
    /// Another account started logging in long ago and never got its
    /// desktop.
    StuckLogin { uid: u32 },
    /// macOS is updating an account for its login.
    AccountUpdating,
}

impl Blocker {
    /// What it is, in the words that follow "Atlas can't switch yet:".
    pub fn words(&self) -> &'static str {
        match self {
            Self::Orphan { .. } | Self::HelperLeftover { .. } => {
                "a deleted account is still logged in."
            }
            Self::StuckLogin { .. } => "another account is still logging in.",
            Self::AccountUpdating => "macOS is still setting up an account.",
        }
    }

    /// The user ID whose login it is, if it is one login's.
    pub fn uid(&self) -> Option<u32> {
        match self {
            Self::Orphan { uid } | Self::HelperLeftover { uid } | Self::StuckLogin { uid } => {
                Some(*uid)
            }
            Self::AccountUpdating => None,
        }
    }
}

/// How long a login may go without a desktop before it counts as stuck.
pub(super) const STUCK_AFTER: Duration = Duration::from_secs(120);

/// What would make a new login hang, from the classified logins and the
/// process list: a deleted account's login, a login of another account
/// with no Dock and no Setup Assistant for too long, or macOS updating an
/// account.
pub(super) fn blockers(classified: &[(Window, Kind)], listed: &[Process]) -> Vec<Blocker> {
    let mut found: Vec<Blocker> = classified
        .iter()
        .filter_map(|(window, kind)| match kind {
            Kind::Orphan => Some(Blocker::Orphan { uid: window.uid }),
            Kind::HelperLeftover => Some(Blocker::HelperLeftover { uid: window.uid }),
            Kind::Helper | Kind::OtherPerson
                if window.age > STUCK_AFTER
                    && !runs_as(listed, window.uid, "/Dock")
                    && !runs_as(listed, window.uid, "/Setup Assistant") =>
            {
                Some(Blocker::StuckLogin { uid: window.uid })
            }
            Kind::Helper | Kind::OtherPerson | Kind::Mine | Kind::Picker => None,
        })
        .collect();

    if updating(listed) {
        found.push(Blocker::AccountUpdating);
    }

    found
}

/// The loginwindow processes in a process list.
pub(super) fn loginwindows_in(listed: &[Process]) -> Vec<Window> {
    listed
        .iter()
        .filter(|process| process.command.ends_with("/loginwindow"))
        .map(|process| Window {
            uid: process.uid,
            pid: process.pid,
            age: process.running,
        })
        .collect()
}

/// The user ID that owns the screen now (0 at the login window), or
/// `None` when it cannot be read.
pub fn console_owner() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata("/dev/console")
        .ok()
        .map(|metadata| metadata.uid())
}

/// What directory services say of `uid`, with how long ago its account was
/// made when `aged` (only the helper's age is ever compared). A lookup that
/// fails counts as an account found: an unknown is never taken for a
/// deleted account.
pub(crate) fn record(uid: u32, aged: bool) -> Record {
    record_with(uid, aged, crate::account::account_with_uid(uid), searched)
}

/// Whether a search of every directory finds an account with `uid`. A
/// search that fails or takes too long counts as one found.
pub(super) fn searched(uid: u32) -> bool {
    crate::bounded::output(
        std::process::Command::new("/usr/bin/dscl").args([
            "/Search",
            "-search",
            "/Users",
            "UniqueID",
            &uid.to_string(),
        ]),
        crate::bounded::LOOKUP,
    )
    .map_or(true, |output| {
        !output.status.success() || !String::from_utf8_lossy(&output.stdout).trim().is_empty()
    })
}

/// [`record`] from `account`, what `getpwuid` gave. Every directory is
/// searched only when it found none: the answer cannot matter otherwise.
pub(super) fn record_with(
    uid: u32,
    aged: bool,
    account: crate::error::Result<Option<crate::account::Account>>,
    search: impl FnOnce(u32) -> bool,
) -> Record {
    let passwd = !matches!(account, Ok(None));
    let directory = !passwd && search(uid);
    let age = account.ok().flatten().filter(|_| aged).and_then(|account| {
        account_age(&account.name).or_else(|| {
            std::fs::metadata(&account.home)
                .and_then(|metadata| metadata.created())
                .ok()
                .and_then(|created| created.elapsed().ok())
        })
    });

    Record {
        passwd,
        directory,
        age,
    }
}

/// Every desktop login on this Mac with whose it is, and what would make a
/// new login hang now. `helper` is the helper (or the account about to be
/// made it). `None` when the process list could not be read.
pub fn sessions(helper: Option<u32>) -> Option<Sessions> {
    let listed = processes()?;
    let me = nix::unistd::getuid().as_raw();
    let accounts = |uid| record(uid, Some(uid) == helper);
    let logins = classify(
        &loginwindows_in(&listed),
        accounts,
        me,
        helper,
        console_owner(),
    );
    let blockers = blockers(&logins, &listed);

    Some(Sessions { logins, blockers })
}

/// Every desktop login on this Mac, classified, and what would make a new
/// login hang now.
#[derive(Debug, Clone, PartialEq)]
pub struct Sessions {
    pub logins: Vec<(Window, Kind)>,
    pub blockers: Vec<Blocker>,
}

impl Sessions {
    /// The logins Atlas may close: deleted accounts' logins, as
    /// `{"uid", "kind"}`.
    pub fn leftovers(&self) -> Vec<Value> {
        self.logins
            .iter()
            .filter(|(_, kind)| kind.leftover())
            .map(|(window, kind)| json!({"uid": window.uid, "kind": kind}))
            .collect()
    }

    /// The user IDs of the deleted accounts still logged in, by kind.
    pub fn leftover_uids(&self, wanted: Kind) -> Vec<u32> {
        self.logins
            .iter()
            .filter(|(_, kind)| *kind == wanted)
            .map(|(window, _)| window.uid)
            .collect()
    }
}

/// What would make a new login hang now; `None` when the process list could
/// not be read.
pub fn blockers_now(helper: Option<u32>) -> Option<Vec<Blocker>> {
    sessions(helper).map(|sessions| sessions.blockers)
}

/// How long a login with its desktop up may go without Atlas answering
/// before its login counts as left behind rather than still starting.
pub(super) const STALE_AFTER: Duration = Duration::from_secs(180);

/// Whether `uid`'s login is up and settled: its loginwindow has run for
/// over three minutes and its Dock runs. Called for a helper whose Atlas is
/// silent, it tells a login left behind from one still starting. `None`
/// when processes could not be listed.
pub fn stale(uid: u32) -> Option<bool> {
    Some(stale_in(&processes()?, uid))
}

pub(super) fn stale_in(listed: &[Process], uid: u32) -> bool {
    loginwindows_in(listed)
        .iter()
        .any(|window| window.uid == uid && window.age > STALE_AFTER)
        && runs_as(listed, uid, "/Dock")
}

/// Whether the logins of `uids` are gone: no loginwindow and no Dock of
/// any of them, seen twice half a second apart, so a login on its way out
/// is not taken for one that ended (a Dock alone can restart in a login
/// that lives on). A process list that cannot be read is not gone.
pub fn confirm_gone(uids: &[u32]) -> bool {
    gone_twice(uids, processes, || {
        std::thread::sleep(Duration::from_millis(500));
    })
}

pub(super) fn gone_twice(
    uids: &[u32],
    mut listed: impl FnMut() -> Option<Vec<Process>>,
    mut pause: impl FnMut(),
) -> bool {
    (0..2).all(|_| {
        pause();
        listed().is_some_and(|listed| uids.iter().all(|uid| logged_out_in(&listed, *uid)))
    })
}

/// How long ago account `name` was created, from its directory record.
pub(super) fn account_age(name: &str) -> Option<Duration> {
    account_created(name)?.elapsed().ok()
}

/// The directory record's creation time; home-folder timestamps are not provenance.
fn account_created(name: &str) -> Option<SystemTime> {
    let output = crate::bounded::output(
        std::process::Command::new("/usr/bin/dscl").args([
            ".",
            "-read",
            &format!("/Users/{name}"),
            "accountPolicyData",
        ]),
        crate::bounded::LOOKUP,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let after = text.split("<key>creationTime</key>").nth(1)?;
    let value = after.split("<real>").nth(1)?.split("</real>").next()?;
    UNIX_EPOCH.checked_add(Duration::try_from_secs_f64(value.trim().parse().ok()?).ok()?)
}

/// `ps` elapsed time (`[[days-]hours:]minutes:seconds`) as a duration.
pub(super) fn running_for(elapsed: &str) -> Option<Duration> {
    let (days, clock) = match elapsed.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, elapsed),
    };

    let parts: Vec<u64> = clock
        .split(':')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    let seconds = match parts.as_slice() {
        [minutes, seconds] => minutes * 60 + seconds,
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        _ => return None,
    };

    Some(Duration::from_secs(days * 86_400 + seconds))
}

#[cfg(test)]
#[path = "logins_tests.rs"]
mod tests;
