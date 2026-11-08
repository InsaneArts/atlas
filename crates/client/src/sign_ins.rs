//! The accounts the helper is signed in to, as Atlas recorded them: a site
//! or an app, when it was added and how. Atlas adds one when the person
//! types a password in the helper while they have control, and the person
//! adds or removes them by hand. It never holds a user name, a password,
//! a cookie or anything typed, and it doesn't prove a sign-in: it lists
//! what the person signed in to as Atlas saw it.
//!
//! One file on the person's side, written whole and private. A missing file
//! is the empty list; one that can't be read, or that a newer Atlas wrote,
//! is unknown and is never taken as empty or written over. Only the person
//! changes it.
use atlas_core::error::{Code, Error, Result};
use serde::{Deserialize, Serialize};
use std::{io::ErrorKind, net::IpAddr, os::unix::fs::DirBuilderExt, path::Path};

/// Where the record is kept, under the person's home folder.
const RECORD: &str = "Library/Application Support/Atlas/sign-ins.json";
/// The lock every change holds from reading the record to replacing it, so
/// two changes at once never lose one another.
const LOCK: &str = "Library/Application Support/Atlas/sign-ins.lock";

/// The record's format; a file of a later one is unknown here.
const VERSION: u32 = 1;

/// How many accounts the record keeps at most.
const MOST: usize = 200;

/// What an account is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A web site, by its registrable domain.
    Site,
    /// A Mac app, by its bundle identifier.
    App,
}

/// How an account came into the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum How {
    /// The person typed a password there while they had control.
    TakeControl,
    /// The person added it in Settings.
    ByHand,
}

/// How agents may act in an account. Every account asks before sending
/// until the person picks another level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    /// Read and look only.
    LookOnly,
    /// Ask the person before anything is sent.
    #[default]
    Ask,
    /// Act without asking.
    Trusted,
}

/// One account in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub kind: Kind,
    /// A site's registrable domain, or an app's bundle identifier.
    pub id: String,
    /// What the person reads: the domain, or the app's name.
    pub name: String,
    /// For a site, the bundle identifier of the browser it was seen in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<String>,
    /// When it was added, in milliseconds since 1970.
    pub added: u64,
    pub how: How,
    #[serde(default)]
    pub level: Level,
}

impl Entry {
    /// Whether `self` and `other` are the same account: the same site in
    /// the same browser, or the same app.
    fn same(&self, other: &Self) -> bool {
        self.kind == other.kind && self.id == other.id && self.browser == other.browser
    }
}

/// The record as it is on disk.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct File {
    version: u32,
    accounts: Vec<Entry>,
}

/// The list as read: the accounts, or unknown when the record can't be
/// read or is newer than this Atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listed {
    Accounts(Vec<Entry>),
    Unknown,
}

/// What Atlas saw when the person typed a password in the helper: the app,
/// and for a page in a browser, its host. Never what was typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Seen {
    /// The bundle identifier of the app with the password field.
    pub app: String,
    /// The app's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The host of the page, when the field sat in a web page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// The accounts in the person's record.
#[must_use]
pub fn listed() -> Listed {
    atlas_core::config::home().map_or(Listed::Unknown, |home| listed_in(&home))
}

/// [`listed`] for the home folder `home`.
#[must_use]
pub fn listed_in(home: &Path) -> Listed {
    let bytes = match std::fs::read(home.join(RECORD)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Listed::Accounts(Vec::new()),
        Err(_) => return Listed::Unknown,
    };
    match serde_json::from_slice::<File>(&bytes) {
        Ok(file) if file.version == VERSION => Listed::Accounts(file.accounts),
        _ => Listed::Unknown,
    }
}

/// Add `entry`, unless the same account is listed already; the person only.
/// Says whether it was added.
///
/// # Errors
///
/// Returns `stopped` when an agent asks, `state` when the record is
/// unknown or full, and file errors when it can't be written.
pub fn add(entry: Entry, by_person: bool) -> Result<bool> {
    add_in(&atlas_core::config::home()?, entry, by_person)
}

/// [`add`] for the home folder `home`.
///
/// # Errors
///
/// As [`add`].
pub fn add_in(home: &Path, entry: Entry, by_person: bool) -> Result<bool> {
    let _locked = locked(home, by_person)?;
    let mut accounts = writable(home)?;
    if accounts.iter().any(|listed| listed.same(&entry)) {
        return Ok(false);
    }
    if accounts.len() >= MOST {
        return Err(Error::new(
            Code::State,
            "The accounts list is full; remove one first",
        ));
    }
    accounts.push(entry);
    save(home, accounts)?;
    Ok(true)
}

/// Remove the account `kind` `id` (seen in `browser`, for a site); the
/// person only. Says whether it was listed.
///
/// # Errors
///
/// Returns `stopped` when an agent asks, `state` when the record is
/// unknown, and file errors when it can't be written.
pub fn remove(kind: Kind, id: &str, browser: Option<&str>, by_person: bool) -> Result<bool> {
    remove_in(&atlas_core::config::home()?, kind, id, browser, by_person)
}

/// [`remove`] for the home folder `home`.
///
/// # Errors
///
/// As [`remove`].
pub fn remove_in(
    home: &Path,
    kind: Kind,
    id: &str,
    browser: Option<&str>,
    by_person: bool,
) -> Result<bool> {
    let _locked = locked(home, by_person)?;
    let mut accounts = writable(home)?;
    let before = accounts.len();
    accounts.retain(|entry| {
        !(entry.kind == kind && entry.id == id && entry.browser.as_deref() == browser)
    });
    if accounts.len() == before {
        return Ok(false);
    }
    save(home, accounts)?;
    Ok(true)
}

/// The account a password typed where Atlas `seen` it belongs to, added at
/// `now`: the page's registrable domain in a browser, else the app.
#[must_use]
pub fn account_of(seen: &Seen, now: u64) -> Entry {
    match seen.host.as_deref().map(site_key) {
        Some(site) => Entry {
            kind: Kind::Site,
            name: site.clone(),
            id: site,
            browser: Some(seen.app.clone()),
            added: now,
            how: How::TakeControl,
            level: Level::default(),
        },
        None => Entry {
            kind: Kind::App,
            id: seen.app.clone(),
            name: seen.name.clone().unwrap_or_else(|| seen.app.clone()),
            browser: None,
            added: now,
            how: How::TakeControl,
            level: Level::default(),
        },
    }
}

/// The key a host is listed by: its registrable domain under the public
/// suffix list, private suffixes included, so `alice.github.io` and
/// `bob.github.io` are two accounts and `accounts.google.com` is
/// `google.com`. A host with no such domain, as `localhost` or an address,
/// is kept whole.
#[must_use]
pub fn site_key(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<IpAddr>().is_ok() {
        return host;
    }
    psl::domain_str(&host).map_or_else(|| host.clone(), str::to_owned)
}

/// The record's lock, held by this caller alone until it is dropped: the
/// person only.
fn locked(home: &Path, by_person: bool) -> Result<atlas_core::files::LockFile> {
    if !by_person {
        return Err(Error::new(
            Code::Stopped,
            "Only the person changes the helper's accounts, from Atlas › Settings › Accounts",
        ));
    }
    let path = home.join(LOCK);
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|error| Error::writing(parent, error))?;
    }
    let lock = atlas_core::files::open_lock(&path)?;
    fs2::FileExt::lock_exclusive(&lock.0).map_err(|error| Error::writing(&path, error))?;
    Ok(lock)
}

/// The accounts, when the record may be written over: only one that was
/// read.
fn writable(home: &Path) -> Result<Vec<Entry>> {
    match listed_in(home) {
        Listed::Accounts(accounts) => Ok(accounts),
        Listed::Unknown => Err(Error::new(
            Code::State,
            "Atlas can't read its accounts list; it was not changed",
        )),
    }
}

/// Write the record whole and private.
fn save(home: &Path, accounts: Vec<Entry>) -> Result<()> {
    let file = File {
        version: VERSION,
        accounts,
    };
    atlas_core::files::save_private(&home.join(RECORD), &serde_json::to_vec_pretty(&file)?)
}

#[cfg(test)]
mod tests;
