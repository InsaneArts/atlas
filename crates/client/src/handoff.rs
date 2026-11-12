//! Handing the person's apps and files to a desktop to open: which targets
//! name files and which name apps or links, the refusal of the person's
//! sign-ins and secrets, and the copies a desktop keeps of what it cannot
//! read, through their owner [`Copies`]. The user desktop's copies, in a
//! private folder of this disk, are [`crate::user::copies`].
use crate::{
    client::{AppRow, Client},
    host_session::Key,
};
use atlas_control::Ownership;
use atlas_core::{
    desktop::{Desktop, Kind},
    error::{Code, Error, Result},
    files::Token,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

/// The copies a desktop opens, and everything done with them once made:
/// whether one is still there, the app it is for, where it is and its
/// identity, which a path lies in, and removing and pruning them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copies(Place);

/// Where a desktop keeps its copies.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    /// A folder of this disk the desktop reads, as a second login does:
    /// ROOT, but in tests.
    SharedFolder(PathBuf),
}

/// A copy folder's identity, as a cleanup of the session that made it
/// checks what runs from it against: where it is, and the node it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CopyIdentity {
    pub(crate) folder: String,
    pub(crate) node: String,
}

impl Copies {
    /// The copies `desktop` opens.
    pub fn of(desktop: &Desktop) -> Self {
        match desktop.kind() {
            Kind::User(_) => Self::in_shared_folder(),
        }
    }

    /// The copies in this disk's shared folder, which a second login opens
    /// and which session records name, as they do not say which desktop
    /// made them.
    pub(crate) fn in_shared_folder() -> Self {
        Self(Place::SharedFolder(PathBuf::from(
            crate::user::copies::ROOT,
        )))
    }

    /// The copies in the folder `root`, for tests.
    #[cfg(test)]
    pub(crate) fn in_folder(root: &Path) -> Self {
        Self(Place::SharedFolder(root.to_path_buf()))
    }

    /// Whether the copy folder `token` names is still there.
    pub(crate) fn exists(&self, token: &Token) -> bool {
        match &self.0 {
            Place::SharedFolder(root) => crate::user::copies::copy_exists(root, token),
        }
    }

    /// The app the copy folder `token` names is for: the app its files
    /// were opened with, else the app it holds. Never a copied file's name.
    pub fn app(&self, token: &Token) -> Option<String> {
        match &self.0 {
            Place::SharedFolder(root) => crate::user::copies::copy_app(root, token),
        }
    }

    /// Where the copy folder `token` names is, as replies show it.
    pub(crate) fn location(&self, token: &Token) -> String {
        match &self.0 {
            Place::SharedFolder(root) => crate::user::copies::copy_folder(root, token)
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// The identity of the copy folder `token` names, when a cleanup of
    /// `session`'s work may treat it as its own.
    pub(crate) fn identity(&self, token: &Token, session: &Key) -> Option<CopyIdentity> {
        match &self.0 {
            Place::SharedFolder(root) => {
                crate::user::copies::copy_node(root, token, session).map(|node| CopyIdentity {
                    folder: crate::user::copies::copy_folder(root, token)
                        .to_string_lossy()
                        .into_owned(),
                    node,
                })
            }
        }
    }

    /// The token of the copy folder `path` lies in, when it lies in one of
    /// these copies: the folder's own name, never the copy's.
    pub(crate) fn token_of(&self, path: &str) -> Option<Token> {
        match &self.0 {
            Place::SharedFolder(root) => crate::user::copies::token_of(path, root),
        }
    }

    /// Remove the copy folder `token` names for a cleanup of `session`'s
    /// work, unless an app in `running` may use it: see [`crate::user::copies::remove_copy`].
    pub(crate) fn remove(&self, token: &Token, session: &Key, running: &[AppRow]) -> CopyRemoval {
        match &self.0 {
            Place::SharedFolder(root) => {
                crate::user::copies::remove_copy(root, token, session, running)
            }
        }
    }

    /// Remove the copies of `app` for host `session`'s command or none,
    /// unless an app in `running` may use them: see [`crate::user::copies::remove_copies_in`].
    pub(crate) fn remove_of_app(
        &self,
        app: &str,
        running: &[AppRow],
        session: Option<&Key>,
    ) -> Removed {
        match &self.0 {
            Place::SharedFolder(root) => {
                crate::user::copies::remove_copies_in(root, app, running, session)
            }
        }
    }

    /// Whether `path` lies in one of these copies: a path Atlas made, which
    /// needs no check of the person's own files.
    pub(crate) fn holds(&self, path: &Path) -> bool {
        match &self.0 {
            Place::SharedFolder(root) => path.starts_with(root),
        }
    }

    /// Remove the copies older than a week that no app in `running` may
    /// use: see [`crate::user::copies::prune_in`]. The tokens of those removed come back.
    pub(crate) fn prune(&self, running: &[AppRow]) -> Vec<Token> {
        match &self.0 {
            Place::SharedFolder(root) => crate::user::copies::prune_in(root, running),
        }
    }
}

/// Refuses `targets` when one is, or holds, the person's sign-ins or secrets: a path here, or a local `file:` URL
/// to one, copied or opened in place alike. Copies already made for the helper (in `copies`) are its own. Callers check before
/// anything else, so nothing of theirs is done first.
///
/// # Errors
///
/// Returns the `credentials` refusal for the first such target.
pub(crate) fn refuse_targets(targets: &[Target], copies: &Copies) -> Result<()> {
    let mut paths = Vec::new();
    for target in targets {
        match target {
            Target::Path(path) if !copies.holds(path) => paths.push(path.clone()),
            Target::Name(name) if is_file_url(name) => {
                // A file URL Atlas cannot read is never handed on unchecked.
                paths.push(local_file(name).ok_or_else(|| {
                    Error::new(
                        Code::Usage,
                        "Atlas can't tell which file that file URL names",
                    )
                })?);
            }
            Target::Name(_) | Target::Path(_) => {}
        }
    }
    crate::credential_files::refuse(paths.iter().map(PathBuf::as_path))
}

/// Whether `name` starts with a URL scheme, as `https:` or `file:` do: letters, then letters, digits, `+`, `-` or
/// `.`, then a colon.
pub(crate) fn has_scheme(name: &str) -> bool {
    name.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|first: char| first.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|part| part.is_ascii_alphanumeric() || "+-.".contains(part))
    })
}

/// Whether `name` is a `file:` URL.
fn is_file_url(name: &str) -> bool {
    name.get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

/// The path a `file:` URL names on this Mac, its escapes decoded; None for anything else.
pub(crate) fn local_file(url: &str) -> Option<PathBuf> {
    if !is_file_url(url) {
        return None;
    }
    let rest = &url[5..];
    // macOS opens a file URL's path on this Mac whatever host it names, so the host is passed over.
    let path = match rest.strip_prefix("//") {
        Some(authority) => &authority[authority.find('/')?..],
        None => rest,
    };
    let path = path.split(['?', '#']).next()?;
    // macOS keeps an escaped `/` inside a name: decoded here it would name another file, so it can't be judged.
    if path.to_ascii_lowercase().contains("%2f") {
        return None;
    }
    let path = PathBuf::from(percent_decoded(path)?);
    // A file reference names a file by its volume and number, not where it is: it can't be judged.
    if path.starts_with("/.file") {
        return None;
    }
    Some(path)
}

/// `text` with its `%XX` escapes decoded, as macOS reads a file URL: a `%` that starts no escape stays as it is.
/// None when the result is not UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        let escaped = (byte == b'%')
            .then(|| after.get(..2))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match escaped {
            Some(decoded) => {
                bytes.push(decoded);
                rest = &after[2..];
            }
            None => {
                bytes.push(byte);
                rest = after;
            }
        }
    }
    String::from_utf8(bytes).ok()
}

/// Something `open` hands the helper: a name or URL, or a path here.
pub(crate) enum Target {
    /// An app name or URL, passed through.
    Name(String),
    /// A file or folder here, made absolute.
    Path(PathBuf),
}

/// `target` as given: a path here when one exists (made absolute, since the
/// helper has its own working directory), otherwise a name or URL.
pub(crate) fn resolve(target: &str) -> Target {
    match fs::canonicalize(target) {
        Ok(path) => Target::Path(path),
        Err(_) => Target::Name(target.to_owned()),
    }
}

/// What [`hand_over`] gives the helper to open.
#[derive(Debug)]
pub(crate) struct Handed {
    /// The targets as the helper can open them, in order.
    pub(crate) paths: Vec<String>,
    /// The copies made for it.
    pub(crate) copied: Vec<Copied>,
}

/// One copy made for the helper: what was copied, where to, and the token
/// its folder is named with. Replies name only where from and to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Copied {
    pub(crate) from: PathBuf,
    pub(crate) to: PathBuf,
    #[serde(skip)]
    pub(crate) token: Token,
}

/// The paths among `targets` as the helper can open them: unchanged when it
/// can read them (or they are copies already), otherwise copied first. Every
/// target is checked before anything is copied; when a copy fails, the
/// copies already made for the others go. What is copied here is copied by
/// a worker of its own that holds `ownership` until it is done, so a caller
/// that stops waiting neither stalls other work nor lets another controller
/// in before the copies are settled.
pub(crate) async fn hand_over(
    client: &Client,
    ownership: &Arc<Ownership>,
    targets: Vec<Target>,
    running: &[AppRow],
) -> Result<Handed> {
    refuse_targets(&targets, &Copies::of(&client.desktop()))?;
    match client.desktop().kind() {
        Kind::User(login) => {
            crate::user::copies::hand_over_to(client, login, ownership, targets, running).await
        }
    }
}

/// The token a copy folder is named with, wherever it is.
pub(crate) fn folder_token(folder: &Path) -> Option<Token> {
    Token::parse(folder.file_name()?.to_str()?)
}

/// What closing an app did with its copies.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Removed {
    /// The folders removed.
    pub(crate) folders: Vec<String>,
    /// How many copies were kept because another host session made them,
    /// or their label could not be read.
    pub(crate) kept_others: usize,
    /// The copies that could not be removed.
    pub(crate) failed: Vec<Failed>,
}

/// A copy folder that could not be removed, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Failed {
    pub(crate) path: String,
    pub(crate) error: String,
}

impl Removed {
    /// The tokens of the folders removed.
    pub(crate) fn tokens(&self) -> Vec<Token> {
        self.folders
            .iter()
            .filter_map(|folder| folder_token(Path::new(folder)))
            .collect()
    }
}

/// What removing one copy folder by its token came to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CopyRemoval {
    /// Nothing is there under that token.
    AlreadyGone,
    Removed,
    /// Kept: another host session made it.
    AnotherSession,
    /// Kept: what is there is not a private folder of this account, its
    /// label cannot be read, or it was swapped while it was looked at.
    Unreadable,
    /// Kept for now: an app in the list runs from it or by one of its
    /// names.
    InUse,
    /// It could not be removed, and why; trying again may.
    Failed(String),
}

/// Whether `executable` lies inside `folder`: the one test of where a
/// process runs from that `close` and cleanup share.
pub fn runs_from(executable: &str, folder: &Path) -> bool {
    let path = Path::new(executable);
    !executable.is_empty() && path.starts_with(folder) && path != folder
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod tests;
