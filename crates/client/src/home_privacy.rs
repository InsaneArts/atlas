//! Whether the person's home folder is private from the helper: one access
//! entry, first in the folder's list, that denies the helper listing and
//! entering it (`user:<helper> deny list,search`). No one else's access
//! changes, and nothing needs an administrator. Atlas adds it when the
//! person or an agent asks; only the person takes it away again.
//!
//! Every look runs `ls` under a time limit, so it happens only when the
//! person asks (setup's last screen, Settings › Helper, `atlas doctor` in a
//! terminal) and never on a status read or a poll.
use atlas_core::error::{Code, Error, Result};
use serde::Serialize;
use std::{fs, path::Path, process::Command};

/// The rights the entry denies: listing the home folder, and going into it.
const DENIED: &str = "deny list,search";

/// Where the one-time offer is remembered, under the person's home folder.
const OFFERED: &str = "Library/Application Support/Atlas/home-privacy";

/// How the person's home folder stands for the helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Files {
    /// Whether the helper is shut out of the home folder.
    pub private: bool,
    /// How many folders in it the helper can open by their permissions; a
    /// floor, since folders with their own access entries aren't counted.
    pub readable: u32,
    /// The first two of those folders: visible ones first, then by name.
    pub examples: Vec<String>,
}

/// How the person's home folder stands for the helper `helper_uid`.
///
/// # Errors
///
/// Returns identity errors when the helper isn't found, and `io` when its
/// listing can't be read in time.
pub fn status(helper_uid: u32) -> Result<Files> {
    let helper = Helper::of(helper_uid)?;
    let home = atlas_core::config::home()?;
    let entries = list(&["-lde"], &home)?;
    if private_in(&entries, &helper.name) {
        return Ok(Files {
            private: true,
            readable: 0,
            examples: Vec::new(),
        });
    }
    // A home the helper may not go into hides everything in it, whatever
    // the folders inside allow.
    let (readable, examples) = if denies_search(&entries) {
        (0, Vec::new())
    } else {
        readable_in(&list(&["-laenb"], &home)?, helper_uid, &helper.groups()?)
    };
    Ok(Files {
        private: false,
        readable,
        examples,
    })
}

/// Make the home folder private from the helper `helper_uid`, then look
/// again to prove it.
///
/// # Errors
///
/// Returns `io` with the Terminal line that does it by hand when the change
/// fails, can't be checked or doesn't hold.
pub fn make_private(helper_uid: u32) -> Result<Files> {
    let helper = Helper::of(helper_uid)?;
    let home = atlas_core::config::home()?;
    let entry = format!("user:{} {DENIED}", helper.name);
    let failed = |cause: Option<Error>| {
        let error = by_hand("make your home folder private", "+a# 0", &entry);
        match cause {
            Some(cause) => error.with_source(cause),
            None => error,
        }
    };
    let entries = list(&["-lde"], &home).map_err(|cause| failed(Some(cause)))?;
    if !private_in(&entries, &helper.name) {
        change(&["+a#", "0", &entry], &home).map_err(|cause| failed(Some(cause)))?;
    }
    let files = status(helper_uid).map_err(|cause| failed(Some(cause)))?;
    if !files.private {
        return Err(failed(None));
    }
    mark_offered(helper_uid);
    Ok(files)
}

/// Let the helper `helper_uid` open the home folder again: the person only.
/// Every entry Atlas added goes, wherever it is in the list.
///
/// # Errors
///
/// Returns `stopped` when an agent asks, and `io` with the Terminal line
/// when the change fails, can't be checked or doesn't hold.
pub fn open(helper_uid: u32, by_person: bool) -> Result<Files> {
    if !by_person {
        return Err(Error::new(
            Code::Stopped,
            "Only the person lets the helper open your home folder, from Atlas › Settings › Helper",
        ));
    }
    let helper = Helper::of(helper_uid)?;
    let home = atlas_core::config::home()?;
    let entry = format!("user:{} {DENIED}", helper.name);
    let failed = |cause: Option<Error>| {
        let error = by_hand("open your home folder again", "-a", &entry);
        match cause {
            Some(cause) => error.with_source(cause),
            None => error,
        }
    };
    // `chmod -a` takes one matching entry away at a time.
    for _ in 0..LINKS {
        let entries = list(&["-lde"], &home).map_err(|cause| failed(Some(cause)))?;
        if ours(&entries, &helper.name).is_none() {
            break;
        }
        change(&["-a", &entry], &home).map_err(|cause| failed(Some(cause)))?;
    }
    let entries = list(&["-lde"], &home).map_err(|cause| failed(Some(cause)))?;
    if ours(&entries, &helper.name).is_some() {
        return Err(failed(None));
    }
    let files = status(helper_uid).map_err(|cause| failed(Some(cause)))?;
    mark_offered(helper_uid);
    Ok(files)
}

/// How many entries of Atlas's own [`open`] removes at most.
const LINKS: usize = 8;

/// The Terminal line that removes the entry, for uninstall to show: Atlas
/// never removes it there, as only the person opens the home folder.
#[must_use]
pub fn undo_line(helper: &str) -> String {
    format!("chmod -a \"user:{helper} {DENIED}\" ~")
}

/// The place of Atlas's entry for `helper` in the person's home folder's
/// list, or `None` when it carries none.
///
/// # Errors
///
/// Returns `io` when the list can't be read in time.
pub fn entry_index(helper: &str) -> Result<Option<usize>> {
    Ok(ours(
        &list(&["-lde"], &atlas_core::config::home()?)?,
        helper,
    ))
}

/// Whether making the home folder private was offered for the helper
/// `helper_uid`, as it is now: a replaced helper is offered it again. It
/// reads only Atlas's own records, so a status poll may ask it; a helper
/// with no record counts as not offered.
#[must_use]
pub fn offered(helper_uid: u32) -> bool {
    offered_as(recorded(helper_uid))
}

/// [`offered`], asking macOS for the helper's GeneratedUID when Atlas has no
/// record of it, as setup's last screen, Settings and the one-time offer do.
#[must_use]
pub fn offered_now(helper_uid: u32) -> bool {
    offered_as(identity_now(helper_uid))
}

/// Remember that making the home folder private was offered for the helper
/// `helper_uid`, whatever the person chose. Nothing is kept for a helper
/// whose GeneratedUID can't be found, so it is offered again.
pub fn mark_offered(helper_uid: u32) {
    if let (Ok(home), Some(identity)) = (atlas_core::config::home(), identity_now(helper_uid)) {
        mark_offered_in(&home, &identity);
    }
}

fn offered_as(identity: Option<String>) -> bool {
    let (Ok(home), Some(identity)) = (atlas_core::config::home(), identity) else {
        return false;
    };
    offered_in(&home, &identity)
}

/// [`offered`] in the home folder `home`, for the helper known as `identity`.
fn offered_in(home: &Path, identity: &str) -> bool {
    fs::read_to_string(home.join(OFFERED))
        .is_ok_and(|text| text.trim().strip_prefix("offered ") == Some(identity))
}

/// [`mark_offered`] in the home folder `home`, for the helper `identity`.
fn mark_offered_in(home: &Path, identity: &str) {
    let path = home.join(OFFERED);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, format!("offered {identity}\n"));
}

/// The helper's GeneratedUID as Atlas recorded it when it was chosen: no
/// later account gets it.
fn recorded(helper_uid: u32) -> Option<String> {
    use atlas_daemon::helper_setup::identity::{OnFile, on_file};
    match on_file(helper_uid) {
        OnFile::Recorded(identity) => Some(identity.generated_uid),
        OnFile::Nothing | OnFile::Unreadable => None,
    }
}

/// [`recorded`], or, with no record, the GeneratedUID macOS gives the
/// account with that user ID now (a bounded `dscl` look).
fn identity_now(helper_uid: u32) -> Option<String> {
    recorded(helper_uid).or_else(|| {
        let account = atlas_core::account::account_with_uid(helper_uid).ok()??;
        atlas_daemon::helper_setup::identity::current(&account)
            .map(|identity| identity.generated_uid)
    })
}

/// The helper's account: its name and the groups it is in.
struct Helper {
    name: String,
}

impl Helper {
    fn of(uid: u32) -> Result<Self> {
        let not_found = || Error::new(Code::Identity, "The helper account was not found");
        let account = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
            .map_err(|cause| not_found().with_source(cause))?
            .ok_or_else(not_found)?;
        Ok(Self { name: account.name })
    }

    /// The group IDs the helper is in, as macOS answers for it.
    fn groups(&self) -> Result<Vec<u32>> {
        let output = atlas_core::bounded::output(
            Command::new("/usr/bin/id").arg("-G").arg(&self.name),
            atlas_core::bounded::FILE,
        )?;
        if !output.status.success() {
            return Err(problem("Atlas couldn't read the helper's groups"));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .filter_map(|gid| gid.parse().ok())
            .collect())
    }
}

/// `ls` with `flags` on `path`, under a time limit.
fn list(flags: &[&str], path: &Path) -> Result<String> {
    let output = atlas_core::bounded::output(&mut ls(flags, path), atlas_core::bounded::FILE)?;
    if !output.status.success() {
        return Err(problem(
            "Atlas couldn't read your home folder's permissions",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The `ls` command for `flags` on `path`, in plain text: the person's
/// colour settings would wrap names in escape codes even through a pipe, and
/// their language can spell a date in more than one word.
fn ls(flags: &[&str], path: &Path) -> Command {
    let mut command = Command::new("/bin/ls");
    command
        .args(flags)
        .arg(path)
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("LSCOLORS")
        .env("LC_ALL", "C");
    command
}

/// `chmod` with `args` on `path`, under a time limit.
fn change(args: &[&str], path: &Path) -> Result<()> {
    let output = atlas_core::bounded::output(
        Command::new("/bin/chmod").args(args).arg(path),
        atlas_core::bounded::FILE,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(problem(
            "Atlas couldn't change your home folder's permissions",
        ))
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Io, message)
}

/// A failure that says the Terminal line doing `what` by hand.
fn by_hand(what: &str, flag: &str, entry: &str) -> Error {
    problem(format!(
        "Atlas couldn't {what}. In Terminal, run: chmod {flag} \"{entry}\" ~"
    ))
}

/// Whether the `ls -lde` listing of the home folder shuts `helper` out.
fn private_in(listing: &str, helper: &str) -> bool {
    let subject = format!("user:{helper}");
    for entry in entries(listing) {
        match entry.kind {
            "allow" if entry.grants("list") || entry.grants("search") => return false,
            "deny"
                if entry.subject == subject
                    && entry.grants("list")
                    && entry.grants("search")
                    && entry.applies() =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Whether an entry of the listing may keep the helper from going into the
/// home folder: any deny of `search` that applies, for anyone. Who a subject
/// covers isn't looked up, so such a case counts as kept out, and its
/// folders as unreachable.
fn denies_search(listing: &str) -> bool {
    entries(listing).any(|entry| entry.kind == "deny" && entry.applies() && entry.grants("search"))
}

/// The place of the first entry Atlas adds for `helper`, wherever it is and
/// whether or not it decides anything.
fn ours(listing: &str, helper: &str) -> Option<usize> {
    let subject = format!("user:{helper}");
    entries(listing)
        .find(|entry| {
            entry.subject == subject
                && entry.kind == "deny"
                && entry.grants("list")
                && entry.grants("search")
        })
        .map(|entry| entry.index)
}

/// The access entries of an `ls -le` listing of one item, in order.
fn entries(listing: &str) -> impl Iterator<Item = Entry<'_>> {
    listing.lines().skip(1).filter_map(Entry::parse)
}

/// One line of an access list, as `ls -le` prints it: ` 1: user:name deny
/// list,search`, perhaps with `inherited` before the kind. macOS takes
/// entries in order, and the first that grants or denies a right decides it.
/// A subject's name may hold spaces, so a line is read from its right.
struct Entry<'a> {
    index: usize,
    subject: &'a str,
    kind: &'a str,
    rights: Vec<&'a str>,
}

impl<'a> Entry<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let (index, rest) = line.trim_start().split_once(": ")?;
        let mut words = rest.trim_end().rsplitn(3, ' ');
        let rights = words.next()?.split(',').collect();
        let kind = words.next()?;
        let subject = words.next()?.trim_end();
        let subject = subject.strip_suffix(" inherited").unwrap_or(subject);
        if !matches!(kind, "allow" | "deny") || subject.is_empty() {
            return None;
        }
        Some(Self {
            index: index.parse().ok()?,
            subject,
            kind,
            rights,
        })
    }

    fn grants(&self, right: &str) -> bool {
        self.rights.contains(&right)
    }

    /// Whether it is about the item itself, not only what is made in it.
    fn applies(&self) -> bool {
        !self.grants("only_inherit")
    }
}

/// The folders in an `ls -laenb` listing of the home folder that the helper
/// `uid`, in `groups`, can list and go into by their permissions, counted,
/// and the first two by name. None when the home folder itself keeps the
/// helper out; a folder with its own access entries isn't counted.
fn readable_in(listing: &str, uid: u32, groups: &[u32]) -> (u32, Vec<String>) {
    let mut lines = listing.lines().skip(1).peekable();
    let mut home_open = false;
    let mut seen_home = false;
    let mut names = Vec::new();
    while let Some(line) = lines.next() {
        let Some(item) = Item::parse(line) else {
            continue;
        };
        let has_entries = lines
            .peek()
            .is_some_and(|next| Entry::parse(next).is_some());
        match item.name {
            // `ls -a` lists the home folder itself first, as `.`.
            "." if !seen_home => {
                seen_home = true;
                // Going in is enough to reach a folder by its name.
                home_open = item.searchable(uid, groups);
            }
            ".." => {}
            name if item.folder && !has_entries && item.opens_for(uid, groups) => {
                names.push(unescaped(name));
            }
            _ => {}
        }
    }
    if !home_open {
        return (0, Vec::new());
    }
    // Folders the person sees in the Finder first, as they are the ones they know.
    names.sort_unstable_by_key(|name| (name.starts_with('.'), name.to_lowercase()));
    let count = u32::try_from(names.len()).unwrap_or(u32::MAX);
    names.truncate(2);
    (count, names)
}

/// One item of an `ls -laenb` listing, its name escaped so it takes one line: `drwxr-xr-x+ 4 501 20 128 Jul 7 09:23
/// name`.
struct Item<'a> {
    folder: bool,
    mode: &'a [u8],
    owner: u32,
    group: u32,
    name: &'a str,
}

impl<'a> Item<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        // Mode, links, owner, group, size, month, day and time or year, then
        // the name, which may hold spaces.
        let mut rest = line;
        let mut fields = [""; 8];
        for field in &mut fields {
            rest = rest.trim_start();
            let end = rest.find(char::is_whitespace)?;
            (*field, rest) = rest.split_at(end);
        }
        let mode = fields[0].as_bytes();
        if mode.len() < 10 {
            return None;
        }
        Some(Self {
            folder: mode[0] == b'd',
            mode,
            owner: fields[2].parse().ok()?,
            group: fields[3].parse().ok()?,
            // One space parts the date from the name; the name keeps its own.
            name: rest.strip_prefix(' ')?,
        })
    }

    /// The permission bits that apply to the helper `uid`, in `groups`: macOS
    /// checks the owner's for its owner, else the group's for a member, else
    /// everyone else's.
    fn class_for(&self, uid: u32, groups: &[u32]) -> &[u8] {
        if self.owner == uid {
            &self.mode[1..4]
        } else if groups.contains(&self.group) {
            &self.mode[4..7]
        } else {
            &self.mode[7..10]
        }
    }

    /// Whether the helper may go into it.
    fn searchable(&self, uid: u32, groups: &[u32]) -> bool {
        matches!(self.class_for(uid, groups)[2], b'x' | b's' | b't')
    }

    /// Whether the helper may list it and go into it.
    fn opens_for(&self, uid: u32, groups: &[u32]) -> bool {
        self.class_for(uid, groups)[0] == b'r' && self.searchable(uid, groups)
    }
}

/// A name as `ls -b` escaped it, as it is: `\\n`, `\\\\` and the other C
/// escapes, and `\\ooo` for any other byte it couldn't print.
fn unescaped(name: &str) -> String {
    let mut bytes = Vec::with_capacity(name.len());
    let mut rest = name.as_bytes();
    while let [first, tail @ ..] = rest {
        rest = tail;
        if *first != b'\\' {
            bytes.push(*first);
            continue;
        }
        let digits = rest
            .iter()
            .take(3)
            .take_while(|byte| (b'0'..=b'7').contains(byte))
            .count();
        if digits == 3 {
            let value = rest[..3]
                .iter()
                .fold(0u32, |value, digit| value * 8 + u32::from(digit - b'0'));
            bytes.push(u8::try_from(value).unwrap_or(b'?'));
            rest = &rest[3..];
            continue;
        }
        let Some((escape, tail)) = rest.split_first() else {
            bytes.push(b'\\');
            break;
        };
        rest = tail;
        bytes.push(match escape {
            b'n' => b'\n',
            b't' => b'\t',
            b'r' => b'\r',
            b'a' => 0x07,
            b'b' => 0x08,
            b'f' => 0x0c,
            b'v' => 0x0b,
            other => *other,
        });
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests;
