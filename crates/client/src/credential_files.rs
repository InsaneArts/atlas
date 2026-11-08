//! What Atlas never copies into the helper: the person's sign-ins and
//! secrets. A selection is refused by where it is, never by what it holds:
//! its path, with symlinks resolved, names a store by its folder under the
//! home folder or by its file name anywhere, and a folder that holds such a
//! folder is refused too. Nothing is read. A hard link or a renamed copy of
//! a store is not recognized, and a folder is not searched for files named
//! like one. Nothing is refused while the person lets agents copy sign-ins
//! in ([`crate::sign_in_copies`]).
use atlas_core::error::{Code, Error, Result};
use std::{
    collections::VecDeque,
    fs,
    path::{Component, Path, PathBuf},
};

/// What a store holds, which says why it stays out of the helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holds {
    /// The person's sign-ins: cookies, keychains, app sessions, keys.
    SignIns,
    /// Passwords and tokens, as a project's settings often do.
    Secrets,
}

/// A store found at, or under, a selected path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Store {
    /// What it is, in words: "Firefox's cookies.sqlite", "Chrome's profiles".
    pub(crate) what: String,
    /// What it holds.
    pub(crate) holds: Holds,
    /// Whether the selection is a folder that holds the store, not the store
    /// itself.
    pub(crate) within: bool,
}

/// Folders under the home folder that hold sign-ins, by their path there,
/// with what they are. A last part ending in `*` matches any name it starts.
const FOLDERS: &[(&str, &str)] = &[
    ("Library/Keychains", "your keychains"),
    (
        "Library/Application Support/Google/Chrome*",
        "Chrome's profiles",
    ),
    (
        "Library/Application Support/Chromium",
        "Chromium's profiles",
    ),
    (
        "Library/Application Support/BraveSoftware",
        "Brave's profiles",
    ),
    (
        "Library/Application Support/Microsoft Edge*",
        "Edge's profiles",
    ),
    (
        "Library/Application Support/Arc/User Data",
        "Arc's profiles",
    ),
    ("Library/Application Support/Vivaldi", "Vivaldi's profiles"),
    (
        "Library/Application Support/com.operasoftware.Opera*",
        "Opera's profiles",
    ),
    (
        "Library/Application Support/Firefox/Profiles",
        "Firefox's profiles",
    ),
    ("Library/Cookies", "Safari's cookies"),
    ("Library/Safari", "Safari's data"),
    ("Library/Containers/com.apple.Safari", "Safari's data"),
    (
        "Library/Application Support/Slack",
        "Slack's sign-in storage",
    ),
    (
        "Library/Containers/com.tinyspeck.slackmacgap",
        "Slack's sign-in storage",
    ),
    (
        "Library/Application Support/discord",
        "Discord's sign-in storage",
    ),
    (".ssh", "your SSH keys"),
    (".gnupg", "your GPG keys"),
    (".aws/credentials", "your AWS credentials"),
    (".config/gh/hosts.yml", "GitHub CLI's sign-in"),
    (".netrc", "your .netrc passwords"),
    (".docker/config.json", "Docker's sign-ins"),
];

/// Files in `~/.ssh` that hold no key.
const SSH_PUBLIC: &[&str] = &["known_hosts", "known_hosts.old", "config"];

/// Store files a browser keeps in its profile, wherever they are, with whose
/// they are.
const STORE_FILES: &[(&str, &str)] = &[
    ("Cookies", "a browser's"),
    ("Login Data", "a browser's"),
    ("Web Data", "a browser's"),
    ("cookies.sqlite", "Firefox's"),
    ("logins.json", "Firefox's"),
    ("key4.db", "Firefox's"),
    ("cert9.db", "Firefox's"),
];

/// The `.env` files that are examples, with no secrets in them.
const ENV_EXAMPLES: &[&str] = &[".env.example", ".env.sample", ".env.template"];

/// Refuses the first of `paths` that is, or holds, one of the person's
/// stores of sign-ins or secrets, before anything is copied. Each path is
/// checked as written and with its symlinks resolved; one that does not
/// exist is checked as written, so its own refusal comes later as before.
///
/// # Errors
///
/// Returns a `credentials` refusal naming the store and the selection's file
/// name, never its full path.
pub(crate) fn refuse<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<()> {
    refuse_at(paths, atlas_core::config::home().ok().as_deref())
}

/// [`refuse`] for the person whose home folder is `home`: nothing is refused
/// while they let agents copy sign-ins in.
fn refuse_at<'a>(paths: impl IntoIterator<Item = &'a Path>, home: Option<&Path>) -> Result<()> {
    if home.is_some_and(crate::sign_in_copies::allowed_in) {
        return Ok(());
    }
    refuse_in(paths, home)
}

/// [`refuse`] with `home` as the home folder; with none, only the store
/// files known by name anywhere are recognized.
pub(crate) fn refuse_in<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
    home: Option<&Path>,
) -> Result<()> {
    let moved = home.map(relocated).unwrap_or_default();
    let homes = home.map(homes).unwrap_or_default();
    for path in paths {
        let steps = steps(path);
        // A store folder the walk passes through as a link is where it really is: what lies there is judged as in it.
        let mut moved = moved.clone();
        for (step, _) in steps.iter().filter(|(_, link)| *link) {
            moved.extend(followed_root(step, &homes));
        }
        // Where it ends: a store, inside one, or a folder that holds one.
        let at_end = steps
            .last()
            .and_then(|(end, _)| judged(end, &homes, &moved));
        // The path as written: one that reads as a store's, whatever links lie under it, is refused too. A `..` is
        // taken back as written, unless what it goes back from is a link, which only the walk can follow.
        let as_written = written(path).and_then(|written| judged(&written, &homes, &moved));
        // Each link on the way, as written, before it is followed: one named like a store, or inside a store folder,
        // refuses, so no link can hide where the path came from. A link that is a store folder itself, as a `~/.ssh`
        // kept on another disk, is followed: what lies in it is judged where it really is, its exceptions too.
        let inside = |step: &Path| {
            let found = judged(step, &homes, &moved).filter(|found| !found.within)?;
            let named = step
                .file_name()
                .and_then(|name| by_name(&name.to_string_lossy()))
                .is_some();
            let in_folder = step
                .parent()
                .and_then(|parent| judged(parent, &homes, &moved))
                .is_some_and(|parent| !parent.within);
            (named || in_folder).then_some(found)
        };
        let link = || {
            steps
                .iter()
                .filter(|(_, link)| *link)
                .find_map(|(step, _)| inside(step))
        };
        // A store itself, or inside one, first, then a link that hides one, then a folder that holds one.
        let (holding, store): (Vec<Store>, Vec<Store>) = [at_end, as_written]
            .into_iter()
            .flatten()
            .partition(|found| found.within);
        let found = store
            .into_iter()
            .next()
            .or_else(link)
            .or_else(|| holding.into_iter().next());
        if let Some(found) = found {
            return Err(refusal(path, &found));
        }
    }
    Ok(())
}

/// `path` made absolute with `.` dropped and each `..` taking away the name before it; None once a `..` would go
/// back from a link, as where it leads is not where it is written.
fn written(path: &Path) -> Option<PathBuf> {
    let mut kept = PathBuf::new();
    for part in std::path::absolute(path).ok()?.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if fs::symlink_metadata(&kept).is_ok_and(|found| found.file_type().is_symlink()) {
                    return None;
                }
                kept.pop();
            }
            other => kept.push(other),
        }
    }
    Some(kept)
}

/// The most links one path may lead through before the walk stops, as the system's own resolution does.
const LINKS: usize = 32;

/// Every path `path` names on the way as its links are followed one at a time, as the system resolves it, with
/// whether it is a link: each entry as written, then, if it is a link, the entries of where it leads. A `..` goes
/// back from where the walk really is. A part that does not exist is taken as written.
fn steps(path: &Path) -> Vec<(PathBuf, bool)> {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut parts: VecDeque<PathBuf> = names_of(&absolute).collect();
    let mut current = PathBuf::from("/");
    let mut steps = Vec::new();
    let mut links = 0;
    while let Some(part) = parts.pop_front() {
        match part.components().next() {
            Some(Component::ParentDir) => {
                current.pop();
                continue;
            }
            Some(Component::Normal(_)) => current.push(&part),
            _ => continue,
        }
        let Ok(target) = fs::read_link(&current) else {
            steps.push((current.clone(), false));
            continue;
        };
        steps.push((current.clone(), true));
        links += 1;
        if links > LINKS {
            break;
        }
        // Where the link leads: from the root, or from the folder it sits in.
        current.pop();
        if target.is_absolute() {
            current = PathBuf::from("/");
        }
        for part in names_of(&target).collect::<Vec<_>>().into_iter().rev() {
            parts.push_front(part);
        }
    }
    // Where the walk really ends, after a last `..` or at the root, is judged as well.
    if steps.last().map(|(end, _)| end) != Some(&current) {
        steps.push((current, false));
    }
    steps
}

/// The parts of `path` after its root, each as a path of one part.
fn names_of(path: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    path.components()
        .filter(|part| !matches!(part, Component::RootDir | Component::Prefix(_)))
        .map(|part| PathBuf::from(part.as_os_str()))
}

/// Where the system's data volume shows the same folders under another path.
const DATA: &str = "/System/Volumes/Data";

/// The home folder as every path that names it: as given, its real path, and the same through the system's data
/// volume, which shows the same folders under another path.
fn homes(home: &Path) -> Vec<PathBuf> {
    let mut homes = spellings(home);
    if let Ok(real) = fs::canonicalize(home) {
        homes.extend(spellings(&real));
    }
    homes.sort();
    homes.dedup();
    homes
}

/// `path` and its other spelling through the system's data volume, which shows the same folders both with and
/// without its prefix.
fn spellings(path: &Path) -> Vec<PathBuf> {
    let other = match path.strip_prefix(DATA) {
        Ok(inside) => Path::new("/").join(inside),
        Err(_) => path.strip_prefix("/").map_or_else(
            |_| path.to_path_buf(),
            |inside| Path::new(DATA).join(inside),
        ),
    };
    vec![path.to_path_buf(), other]
}

/// Where a link that is a store folder itself, met on a walk, really leads, by every spelling, with what the folder
/// is; nothing for any other link. The walk finds such a folder even where its parent can't be listed.
fn followed_root(step: &Path, homes: &[PathBuf]) -> Vec<(PathBuf, &'static str, &'static str)> {
    let Some((folder, what)) = homes.iter().find_map(|home| {
        let inside = parts(strip(step, home)?);
        FOLDERS.iter().find(|(folder, _)| {
            let wanted: Vec<&str> = folder.split('/').collect();
            inside.len() == wanted.len() && matches(&inside, &wanted)
        })
    }) else {
        return Vec::new();
    };
    let Ok(real) = fs::canonicalize(step) else {
        return Vec::new();
    };
    spellings(&real)
        .into_iter()
        .map(|spelling| (spelling, *folder, *what))
        .collect()
}

/// The store `step` is or holds, judged against every path of the home and the store folders moved elsewhere.
fn judged(
    step: &Path,
    homes: &[PathBuf],
    moved: &[(PathBuf, &'static str, &'static str)],
) -> Option<Store> {
    if homes.is_empty() {
        return store(step, None);
    }
    homes
        .iter()
        .find_map(|home| store(step, Some(home)))
        .or_else(|| moved_store(step, moved))
}

/// Where the folders of [`FOLDERS`] under `home` really are, with what they are, for those whose real place differs
/// from where they show: one of them, or a folder above it, may be a link to another disk. A name ending in `*`
/// stands for each folder there that it matches.
fn relocated(home: &Path) -> Vec<(PathBuf, &'static str, &'static str)> {
    let mut moved = Vec::new();
    for (folder, what) in FOLDERS {
        let shown = home.join(folder.trim_end_matches('*'));
        let candidates: Vec<PathBuf> = match folder.strip_suffix('*') {
            None => vec![shown],
            Some(_) => {
                let (Some(parent), Some(start)) = (shown.parent(), shown.file_name()) else {
                    continue;
                };
                let start = start.to_string_lossy().to_ascii_lowercase();
                fs::read_dir(parent)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name().is_some_and(|name| {
                            name.to_string_lossy()
                                .to_ascii_lowercase()
                                .starts_with(&start)
                        })
                    })
                    .collect()
            }
        };
        for candidate in candidates {
            if let Ok(real) = fs::canonicalize(&candidate)
                && real != candidate
            {
                moved.extend(
                    spellings(&real)
                        .into_iter()
                        .map(|spelling| (spelling, *folder, *what)),
                );
            }
        }
    }
    moved
}

/// The store `path`, already resolved, is or holds among the folders `moved` to another place: inside one, as its own
/// exceptions allow, or a folder above one.
fn moved_store(path: &Path, moved: &[(PathBuf, &'static str, &'static str)]) -> Option<Store> {
    moved.iter().find_map(|(real, folder, what)| {
        let found = |within| Store {
            what: (*what).to_owned(),
            holds: Holds::SignIns,
            within,
        };
        if let Some(inside) = strip(path, real) {
            let rest = parts(inside);
            let public = *folder == ".ssh"
                && matches!(rest.as_slice(), [name] if SSH_PUBLIC.iter().any(|public| name.eq_ignore_ascii_case(public)));
            return (!public).then(|| found(false));
        }
        strip(real, path).map(|_| found(true))
    })
}

/// The store `path`, absolute and with no `.` or `..` in it, as a walk gives it, is or holds, if any.
pub(crate) fn store(path: &Path, home: Option<&Path>) -> Option<Store> {
    if let Some(found) = path
        .file_name()
        .and_then(|name| by_name(&name.to_string_lossy()))
    {
        return Some(found);
    }
    let home = home?;
    if let Some(inside) = strip(path, home) {
        let parts = parts(inside);
        if let [first, .., last] = parts.as_slice()
            && first.eq_ignore_ascii_case(".ssh")
            && parts.len() == 2
            && SSH_PUBLIC
                .iter()
                .any(|public| last.eq_ignore_ascii_case(public))
        {
            return None;
        }
        if let Some((_, what)) = FOLDERS.iter().find(|(folder, _)| within(&parts, folder)) {
            return Some(Store {
                what: (*what).to_owned(),
                holds: Holds::SignIns,
                within: false,
            });
        }
    }
    holding(path, home).map(|what| Store {
        what: what.to_owned(),
        holds: Holds::SignIns,
        within: true,
    })
}

/// What a folder at `path` holds of [`FOLDERS`], handing it over whole: all of them when it is the home folder or
/// above it, else the first one it lies above.
fn holding(path: &Path, home: &Path) -> Option<&'static str> {
    if strip(home, path).is_some() {
        return FOLDERS.first().map(|(_, what)| *what);
    }
    let inside = parts(strip(path, home)?);
    FOLDERS
        .iter()
        .find(|(folder, _)| {
            let wanted: Vec<&str> = folder.split('/').collect();
            inside.len() < wanted.len() && matches(&inside, &wanted)
        })
        .map(|(_, what)| *what)
}

/// The store a file named `name` is, wherever it is.
fn by_name(name: &str) -> Option<Store> {
    let lower = name.to_ascii_lowercase();
    if let Some((file, whose)) = STORE_FILES
        .iter()
        .find(|(file, _)| file.eq_ignore_ascii_case(name))
    {
        return Some(Store {
            what: format!("{whose} {file}"),
            holds: Holds::SignIns,
            within: false,
        });
    }
    if lower.ends_with(".keychain") || lower.ends_with(".keychain-db") {
        return Some(Store {
            what: format!("the keychain {name}"),
            holds: Holds::SignIns,
            within: false,
        });
    }
    let env =
        lower == ".env" || (lower.starts_with(".env.") && !ENV_EXAMPLES.contains(&lower.as_str()));
    (env || lower == ".npmrc").then(|| Store {
        what: name.to_owned(),
        holds: Holds::Secrets,
        within: false,
    })
}

/// What is left of `path` below `base`, comparing names as the Mac's
/// disks do, ignoring case; None when `path` is not under `base`.
fn strip<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    let mut rest = path.components();
    for part in base.components() {
        let next = rest.next()?;
        if !same(next, part) {
            return None;
        }
    }
    Some(rest.as_path())
}

/// Whether two path parts name the same thing, ignoring case.
fn same(left: Component<'_>, right: Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

/// The names `path` is made of.
fn parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// Whether `parts` lie in `folder`, or are it, ignoring case.
fn within(parts: &[String], folder: &str) -> bool {
    let wanted: Vec<&str> = folder.split('/').collect();
    parts.len() >= wanted.len() && matches(parts, &wanted)
}

/// Whether each of `parts` is the name `wanted` has in its place, as far as either goes: a wanted name ending in `*`
/// matches any name it starts.
fn matches(parts: &[String], wanted: &[&str]) -> bool {
    wanted
        .iter()
        .zip(parts)
        .all(|(want, part)| match want.strip_suffix('*') {
            Some(start) => part
                .to_ascii_lowercase()
                .starts_with(&start.to_ascii_lowercase()),
            None => want.eq_ignore_ascii_case(part),
        })
}

/// The refusal for `path`, which is or holds `found`.
fn refusal(path: &Path, found: &Store) -> Error {
    let name = path.file_name().map_or_else(
        || "that folder".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let message = match (found.holds, found.within) {
        (Holds::Secrets, _) => format!(
            "Atlas won't copy {} into the helper: it often holds passwords and tokens",
            found.what
        ),
        (Holds::SignIns, true) => format!(
            "Atlas won't copy {name} into the helper: it holds {}, with your sign-ins",
            found.what
        ),
        (Holds::SignIns, false) if found.what.ends_with(&name) => format!(
            "Atlas won't copy {} into the helper: it holds your sign-ins",
            found.what
        ),
        (Holds::SignIns, false) => format!(
            "Atlas won't copy {name} into the helper: it is part of {}, which holds your sign-ins",
            found.what
        ),
    };
    Error::new(Code::Credentials, message)
}

#[cfg(test)]
mod tests;
