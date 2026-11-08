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
    let groups = helper.groups()?;
    let (readable, examples) = readable_in(&list(&["-laen"], &home)?, helper_uid, &groups);
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
/// fails or doesn't hold.
pub fn make_private(helper_uid: u32) -> Result<Files> {
    let helper = Helper::of(helper_uid)?;
    let home = atlas_core::config::home()?;
    let entry = format!("user:{} {DENIED}", helper.name);
    if !private_in(&list(&["-lde"], &home)?, &helper.name) {
        change(&["+a#", "0", &entry], &home)?;
    }
    let files = status(helper_uid)?;
    if !files.private {
        return Err(by_hand("make your home folder private", "+a# 0", &entry));
    }
    mark_offered(helper_uid);
    Ok(files)
}

/// Let the helper `helper_uid` open the home folder again: the person only.
///
/// # Errors
///
/// Returns `stopped` when an agent asks, and `io` with the Terminal line
/// when the change fails or doesn't hold.
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
    if private_in(&list(&["-lde"], &home)?, &helper.name) {
        change(&["-a", &entry], &home)?;
    }
    let files = status(helper_uid)?;
    if files.private {
        return Err(by_hand("open your home folder again", "-a", &entry));
    }
    mark_offered(helper_uid);
    Ok(files)
}

/// The Terminal line that removes the entry, for uninstall to show: Atlas
/// never removes it there, as only the person opens the home folder.
#[must_use]
pub fn undo_line(helper: &str) -> String {
    format!("chmod -a \"user:{helper} {DENIED}\" ~")
}

/// Whether the person's home folder carries the entry for `helper`, and its
/// place in the list; `None` when it doesn't or can't be read.
#[must_use]
pub fn entry_index(helper: &str) -> Option<usize> {
    let home = atlas_core::config::home().ok()?;
    deciding_deny(&list(&["-lde"], &home).ok()?, helper)
}

/// Whether making the home folder private was offered for the helper
/// `helper_uid`, as it is now: a replaced helper is offered it again.
#[must_use]
pub fn offered(helper_uid: u32) -> bool {
    atlas_core::config::home().is_ok_and(|home| offered_in(&home, &identity_of(helper_uid)))
}

/// Remember that making the home folder private was offered for the helper
/// `helper_uid`, whatever the person chose.
pub fn mark_offered(helper_uid: u32) {
    if let Ok(home) = atlas_core::config::home() {
        mark_offered_in(&home, &identity_of(helper_uid));
    }
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

/// The helper as recorded when it was chosen: its GeneratedUID, which no
/// later account gets, or its user ID when nothing is recorded.
fn identity_of(helper_uid: u32) -> String {
    match atlas_daemon::helper_setup::identity::on_file(helper_uid) {
        atlas_daemon::helper_setup::identity::OnFile::Recorded(identity) => identity.generated_uid,
        _ => format!("uid:{helper_uid}"),
    }
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
    let output = atlas_core::bounded::output(
        Command::new("/bin/ls").args(flags).arg(path),
        atlas_core::bounded::FILE,
    )?;
    if !output.status.success() {
        return Err(problem(
            "Atlas couldn't read your home folder's permissions",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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
    deciding_deny(listing, helper).is_some()
}

/// The place of the entry that shuts `helper` out, when one decides it.
/// macOS takes entries in order, and the first that grants or denies a right
/// decides it: so the helper's deny of both rights counts only when it applies
/// to the folder itself and no earlier entry, for anyone, allows either.
fn deciding_deny(listing: &str, helper: &str) -> Option<usize> {
    let subject = format!("user:{helper}");
    for entry in listing.lines().skip(1).filter_map(Entry::parse) {
        let grants = |right: &str| entry.rights.contains(&right);
        match entry.kind {
            "allow" if grants("list") || grants("search") => return None,
            "deny"
                if entry.subject == subject
                    && grants("list")
                    && grants("search")
                    && !grants("only_inherit") =>
            {
                return Some(entry.index);
            }
            _ => {}
        }
    }
    None
}

/// One line of an access list, as `ls -le` prints it: ` 1: user:name deny
/// list,search`, perhaps with `inherited` before the kind.
struct Entry<'a> {
    index: usize,
    subject: &'a str,
    kind: &'a str,
    rights: Vec<&'a str>,
}

impl<'a> Entry<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let (index, rest) = line.trim_start().split_once(": ")?;
        let mut words = rest.split_whitespace();
        let subject = words.next()?;
        let kind = match words.next()? {
            "inherited" => words.next()?,
            kind => kind,
        };
        let rights = words.next()?.split(',').collect();
        Some(Self {
            index: index.parse().ok()?,
            subject,
            kind,
            rights,
        })
    }
}

/// The folders in an `ls -laen` listing of the home folder that the helper
/// `uid`, in `groups`, can list and go into by their permissions, counted,
/// and the first two by name. None when the home folder itself keeps the
/// helper out; a folder with its own access entries isn't counted.
fn readable_in(listing: &str, uid: u32, groups: &[u32]) -> (u32, Vec<String>) {
    let mut lines = listing.lines().skip(1).peekable();
    let mut home_open = false;
    let mut names = Vec::new();
    while let Some(line) = lines.next() {
        let Some(item) = Item::parse(line) else {
            continue;
        };
        let has_entries = lines
            .peek()
            .is_some_and(|next| Entry::parse(next).is_some());
        match item.name {
            "." => home_open = item.opens_for(uid, groups),
            ".." => {}
            name if item.folder && !has_entries && item.opens_for(uid, groups) => {
                names.push(name.to_owned());
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

/// One item of an `ls -laen` listing: `drwxr-xr-x+ 4 501 20 128 Jul 7 09:23
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
            name: rest.trim_start(),
        })
    }

    /// Whether the helper `uid`, in `groups`, may list it and go into it:
    /// macOS checks the owner's bits for its owner, else the group's for a
    /// member, else everyone else's.
    fn opens_for(&self, uid: u32, groups: &[u32]) -> bool {
        let class = if self.owner == uid {
            &self.mode[1..4]
        } else if groups.contains(&self.group) {
            &self.mode[4..7]
        } else {
            &self.mode[7..10]
        };
        class[0] == b'r' && matches!(class[2], b'x' | b's' | b't')
    }
}

#[cfg(test)]
mod tests;
