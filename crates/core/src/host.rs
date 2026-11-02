//! The agent conversation a command comes from, as its host tells the
//! shell: a Claude Code session, a Codex thread, an opencode or Pi session.
//! It is a label for the person and the agent, never a permission. Atlas's
//! own "session" is the helper's login; this is the host's.
pub mod ancestry;
pub mod verification;

use crate::{
    name::{bounded, hex, lower_hex, name_type},
    process::Instance,
    record_folder::RecordName,
};
use ancestry::{Ancestor, Facts};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Hex digits of a host session's key, the only part rows carry.
pub(crate) const KEY_LEN: usize = 12;
/// The longest host id or label accepted.
pub(crate) const ID_MAX: usize = 128;
/// How many of this process's ancestors are looked at to tell nested hosts
/// apart.
const ANCESTORS: usize = 32;
/// The most folders looked at, from the working directory up, for the
/// project a session works in.
const FOLDER_DEPTH: usize = 32;

/// Each identity declares its ordered environment sources. The declaration
/// also generates the complete scan, so a new host cannot omit attribution.
macro_rules! hosts {
    ($($(#[$attr:meta])* $host:ident => ($word:literal, $label:literal, $hint:expr, [$($source:literal),*])),* $(,)?) => {
        /// An agent host Atlas can tell a session of.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum Host { $($(#[$attr])* $host),* }

        const SOURCES: &[(Host, &[&str])] = &[$((Host::$host, &[$($source),*])),*];

        impl Host {
            fn pid_hint(self) -> Option<&'static str> {
                match self { $(Self::$host => $hint),* }
            }

            /// The host's display name; empty for a session named by its label.
            pub fn label(self) -> &'static str {
                match self { $(Self::$host => $label),* }
            }

            /// The host's word in records and keys.
            pub(crate) fn as_str(self) -> &'static str {
                match self { $(Self::$host => $word),* }
            }
        }
    };
}

// The first plain value wins. Named sessions are read only after this scan.
hosts! {
    Claude => ("claude", "Claude Code", Some("CLAUDE_PID"), ["CLAUDE_CODE_SESSION_ID"]),
    Codex => ("codex", "Codex", None, ["CODEX_SESSION_ID", "CODEX_THREAD_ID"]),
    Opencode => ("opencode", "opencode", Some("ATLAS_OPENCODE_PID"), ["ATLAS_OPENCODE_SESSION"]),
    Pi => ("pi", "Pi", Some("ATLAS_PI_PID"), ["PI_SESSION_ID"]),
    Cursor => ("cursor", "Cursor", None, ["ATLAS_CURSOR_SESSION"]),
    /// A name given with ATLAS_SESSION, outside any host Atlas knows.
    Named => ("named", "", None, []),
}

impl Host {
    fn sessions(env: &impl Fn(&str) -> Option<String>) -> Vec<(Self, HostId)> {
        SOURCES
            .iter()
            .filter_map(|(host, names)| {
                names
                    .iter()
                    .find_map(|name| env(name).as_deref().and_then(HostId::parse))
                    .map(|id| (*host, id))
            })
            .collect()
    }

    fn hint(self, env: &impl Fn(&str) -> Option<String>) -> Option<i32> {
        env(self.pid_hint()?)?
            .parse::<i32>()
            .ok()
            .filter(|pid| *pid > 0)
    }

    /// Whether an ancestor matches a host-specific PID hint or command name.
    fn runs_as(self, ancestor: &Ancestor, hint: Option<i32>, uid: u32) -> bool {
        if ancestor.uid != uid {
            return false;
        }
        let command = ancestor.command.as_str();
        match self {
            Self::Claude => hint == Some(ancestor.pid),
            Self::Codex => command.starts_with("codex"),
            Self::Opencode => hint == Some(ancestor.pid) || command == "opencode",
            Self::Pi => hint == Some(ancestor.pid) || command == "pi",
            Self::Cursor => command.starts_with("Cursor") || command == "cursor-agent",
            Self::Named => false,
        }
    }
}

name_type! {
    /// A host session's key: the first [`KEY_LEN`] hex digits of SHA-256 of
    /// "{host}:{id}". No host value ever reaches a path or another session's
    /// reply but through it.
    #[derive(Hash, PartialOrd, Ord)]
    pub struct Key;
    refused(text) = format!("{text} is not a host session key");
    display
}

impl Key {
    /// The key of `id` of `host`.
    pub fn of(host: Host, id: &str) -> Self {
        Self::digest(&format!("{}:{id}", host.as_str()))
    }

    /// The first [`KEY_LEN`] hex digits of SHA-256 of `text`: how every
    /// key Atlas makes from a label is drawn.
    pub fn digest(text: &str) -> Self {
        let mut hex = hex(&Sha256::digest(text.as_bytes()));
        hex.truncate(KEY_LEN);
        Self(hex)
    }

    /// `text` as a key, when it is exactly [`KEY_LEN`] lowercase hex digits.
    pub fn parse(text: &str) -> Option<Self> {
        lower_hex(text, KEY_LEN).then(|| Self(text.to_owned()))
    }
}

impl RecordName for Key {
    fn parse(text: &str) -> Option<Self> {
        Key::parse(text)
    }
}

name_type! {
    /// A host's own id for a session, or an ATLAS_SESSION label: 1 to
    /// [`ID_MAX`] bytes of ASCII letters, digits, `.`, `_`, `:` and `-`, so
    /// no other value is ever kept or shown.
    pub struct HostId;
    refused(_value) = "not a plain host session id".to_owned();
}

impl HostId {
    /// `value` as a host id, when it is a plain one.
    pub fn parse(value: &str) -> Option<Self> {
        crate::name::plain(value, ID_MAX).then(|| Self(value.to_owned()))
    }
}

/// What other sessions may know of a host session: its key, its host and
/// what it is named by, never a host's own id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub key: Key,
    pub host: Host,
    /// The project folder's name, never its path.
    pub folder: Option<String>,
    /// A named session's own label, its ATLAS_SESSION value, which names
    /// it; none for a host's session.
    pub named: Option<HostId>,
}

/// The host session one command comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostSession {
    key: Key,
    host: Host,
    /// The host's own id, or the ATLAS_SESSION label.
    id: HostId,
    /// The verified root host's exact process instance.
    pub process: Option<Instance>,
    /// The previous exact host instance the daemon proved has exited.
    pub replaces: Option<Instance>,
    /// The root host's project folder basename, or the legacy fallback.
    pub folder: Option<String>,
    /// When its command started, in milliseconds since 1970.
    began: Option<u64>,
    pub folder_from_host: bool,
    verified: bool,
    ambiguous: bool,
}

impl HostSession {
    /// The session `id` of `host`, when `id` is a plain id.
    pub fn new(host: Host, id: &str) -> Option<Self> {
        let id = HostId::parse(id)?;
        Some(Self {
            key: Key::of(host, id.as_str()),
            host,
            id,
            process: None,
            replaces: None,
            folder: None,
            began: None,
            folder_from_host: false,
            verified: false,
            ambiguous: false,
        })
    }

    /// This session, working in the project folder named `folder`.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn in_folder(mut self, folder: Option<&str>) -> Self {
        self.folder = folder.map(str::to_owned);
        self
    }

    /// This session, its command begun at `began`.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn begun(mut self, began: Option<u64>) -> Self {
        self.began = began;
        self
    }

    /// This session, run by host process `process`.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn run_by(mut self, process: Option<Instance>) -> Self {
        self.process = process;
        self
    }

    /// The key rows and records carry for it.
    pub fn key(&self) -> &Key {
        &self.key
    }

    pub fn host(&self) -> Host {
        self.host
    }

    /// The host's own id, or the ATLAS_SESSION label.
    pub fn id(&self) -> &HostId {
        &self.id
    }

    /// The verified root host's exact process instance.
    pub fn process(&self) -> Option<Instance> {
        self.process
    }

    /// The host's process when it is one of this command's ancestors and
    /// still the process it names: proof, beyond the environment's word,
    /// that this command runs in that host.
    pub fn ancestor(&self) -> Option<Instance> {
        self.process.filter(|process| {
            self.verified
                || (process.running()
                    && ancestry::ancestors(ANCESTORS)
                        .iter()
                        .any(|ancestor| ancestor.instance == *process))
        })
    }

    /// The root host's project folder basename, or the legacy fallback.
    pub fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }

    /// When its command started, in milliseconds since 1970: as its notes
    /// say, by this process's start.
    pub fn began(&self) -> Option<u64> {
        self.began
    }

    /// The host session of this process's command, from its environment,
    /// named by the verified root host's project and begun when this
    /// process started; None when no host names one or its value is not a
    /// plain id.
    pub fn from_env() -> Option<Self> {
        let session = Self::resolve(
            |name| std::env::var(name).ok(),
            || ancestry::ancestors(ANCESTORS),
            ancestry::process,
        )?;
        let session = session.with_folder(crate::process::cwd, std::env::current_dir().ok());
        let began = Instance::current().map(|process| process.started_ms());
        Some(Self { began, ..session })
    }

    /// Resolve labels using only same-account ancestry and exact process starts.
    pub(crate) fn resolve(
        env: impl Fn(&str) -> Option<String>,
        ancestors: impl FnOnce() -> Vec<Ancestor>,
        process: impl Fn(i32) -> Option<Facts>,
    ) -> Option<Self> {
        Self::resolve_for(env, ancestors, process, nix::unistd::geteuid().as_raw())
    }

    pub(crate) fn resolve_for(
        env: impl Fn(&str) -> Option<String>,
        ancestors: impl FnOnce() -> Vec<Ancestor>,
        process: impl Fn(i32) -> Option<Facts>,
        uid: u32,
    ) -> Option<Self> {
        let candidates = Host::sessions(&env);
        if candidates.is_empty() {
            return Self::new(Host::Named, env("ATLAS_SESSION")?.as_str());
        }
        let hint = |host: Host| host.hint(&env);
        let walk = ancestors();
        let (host, id) = if candidates.len() == 1 {
            candidates.into_iter().next()?
        } else {
            nearest(candidates, &walk, &hint, uid)?
        };
        let matching: Vec<_> = walk
            .iter()
            .filter(|ancestor| host.runs_as(ancestor, hint(host), uid))
            .collect();
        let complete = walk
            .last()
            .is_some_and(|ancestor| ancestor.pid == 1 || ancestor.parent == 1);
        let ambiguous = host == Host::Codex && (!complete || matching.len() != 1);
        let selected = matching
            .first()
            .filter(|_| !ambiguous && host != Host::Cursor && matching.len() == 1);
        let instance = selected.and_then(|ancestor| {
            let facts = process(ancestor.pid)?;
            (facts.uid == uid
                && facts.start == ancestor.instance.start
                && facts.command == ancestor.command)
                .then_some(ancestor.instance)
        });
        Some(Self {
            process: instance,
            verified: instance.is_some(),
            ambiguous,
            ..Self::new(host, id.as_str())?
        })
    }

    fn with_folder(
        mut self,
        cwd: impl FnOnce(Instance) -> Option<std::path::PathBuf>,
        command_cwd: Option<std::path::PathBuf>,
    ) -> Self {
        if let Some(process) = self.process {
            if let Some(directory) = cwd(process) {
                self.folder = project_folder(&directory);
                self.folder_from_host = true;
            }
        } else if !self.ambiguous {
            self.folder = command_cwd.and_then(|directory| project_folder(&directory));
        }
        self
    }

    /// What other sessions may know of this one.
    pub fn label(&self) -> Label {
        Label {
            key: self.key.clone(),
            host: self.host,
            folder: self.folder.clone(),
            named: (self.host == Host::Named).then(|| self.id.clone()),
        }
    }
}

/// Of hosts that each name a session, the one whose process is the nearest
/// of `ancestors`, when every one of them is found among them. A host not
/// found (Pi running as `node`, a host that detached, a walk cut short)
/// may be nearer than any found, so then none is chosen: never a guess.
fn nearest(
    candidates: Vec<(Host, HostId)>,
    ancestors: &[Ancestor],
    hint: &impl Fn(Host) -> Option<i32>,
    uid: u32,
) -> Option<(Host, HostId)> {
    candidates
        .into_iter()
        .map(|candidate| {
            let place = ancestors
                .iter()
                .position(|ancestor| candidate.0.runs_as(ancestor, hint(candidate.0), uid))?;
            Some((place, candidate))
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min_by_key(|(place, _)| *place)
        .map(|(_, candidate)| candidate)
}

/// The project a command works in: the name of the nearest folder from
/// `directory` up that holds a `.git` entry, else of `directory` itself.
/// None for the home folder and `/`, which name no project.
pub fn project_folder(directory: &Path) -> Option<String> {
    let project = directory
        .ancestors()
        .take(FOLDER_DEPTH)
        .find(|folder| fs::symlink_metadata(folder.join(".git")).is_ok())
        .unwrap_or(directory);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if home.as_deref() == Some(project) {
        return None;
    }

    project
        .file_name()
        .and_then(|name| name.to_str())
        .map(bounded)
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests;
