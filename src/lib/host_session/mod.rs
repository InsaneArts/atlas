//! The agent conversation a command comes from, as its host tells the
//! shell: a Claude Code session, a Codex thread, an opencode or Pi session.
//! It is a label for the person and the agent, never a permission. Atlas's
//! own "session" is the helper's login; this is the host's.
pub mod activity;
pub(crate) mod ancestry;
pub(crate) mod brief;
pub mod explain;
pub(crate) mod names;
pub(crate) mod record;
mod recovery;
pub mod store;
#[allow(unsafe_code, reason = "FFI: localtime_r has no safe binding")]
mod sys;

pub use crate::process::Instance;
pub use recovery::{Asker, Recovery};

use crate::name::{hex, lower_hex, name_type};
use ancestry::{Ancestor, Facts};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hex digits of a host session's key, the only part rows carry.
pub(crate) const KEY_LEN: usize = 12;
/// The longest host id or label accepted.
pub(crate) const ID_MAX: usize = 128;
/// How many of this process's ancestors are looked at to tell nested hosts
/// apart.
const ANCESTORS: usize = 32;

/// Where each host names its session, the first plain value winning. A
/// name given with `ATLAS_SESSION` is read only when none of these is.
const SOURCES: [(Host, &[&str]); 5] = [
    (Host::Claude, &["CLAUDE_CODE_SESSION_ID"]),
    (Host::Codex, &["CODEX_SESSION_ID", "CODEX_THREAD_ID"]),
    (Host::Opencode, &["ATLAS_OPENCODE_SESSION"]),
    (Host::Pi, &["PI_SESSION_ID"]),
    (Host::Cursor, &["ATLAS_CURSOR_SESSION"]),
];

/// An agent host Atlas can tell a session of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Claude,
    Codex,
    Opencode,
    Pi,
    Cursor,
    /// A name given with ATLAS_SESSION, outside any host Atlas knows.
    Named,
}

impl Host {
    /// The host as people know it: "Claude Code", "Codex", "opencode", "Pi",
    /// "Cursor"; empty for a named session, whose name is its label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Opencode => "opencode",
            Self::Pi => "Pi",
            Self::Cursor => "Cursor",
            Self::Named => "",
        }
    }

    /// The host's word in records and keys: "claude", "codex" and so on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::Named => "named",
        }
    }

    /// Whether `ancestor` is this host's own process: Claude Code's when it
    /// is `claude_pid`, the others' by their command names.
    fn runs_as(self, ancestor: &Ancestor, claude_pid: Option<i32>) -> bool {
        let command = ancestor.command.as_str();
        match self {
            Self::Claude => claude_pid == Some(ancestor.pid),
            Self::Codex => command.starts_with("codex"),
            Self::Opencode => command == "opencode",
            Self::Pi => command == "pi",
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
    pub(crate) fn digest(text: &str) -> Self {
        let mut hex = hex(&Sha256::digest(text.as_bytes()));
        hex.truncate(KEY_LEN);
        Self(hex)
    }

    /// `text` as a key, when it is exactly [`KEY_LEN`] lowercase hex digits.
    pub fn parse(text: &str) -> Option<Self> {
        lower_hex(text, KEY_LEN).then(|| Self(text.to_owned()))
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
        plain(value).then(|| Self(value.to_owned()))
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
    /// The host's process, where the host names it (Claude Code).
    process: Option<Instance>,
    /// The project folder's name its command works in, never its path.
    folder: Option<String>,
    /// When its command started, in milliseconds since 1970.
    began: Option<u64>,
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
            folder: None,
            began: None,
        })
    }

    /// This session, working in the project folder named `folder`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn in_folder(mut self, folder: Option<&str>) -> Self {
        self.folder = folder.map(str::to_owned);
        self
    }

    /// This session, its command begun at `began`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn begun(mut self, began: Option<u64>) -> Self {
        self.began = began;
        self
    }

    /// This session, run by host process `process`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn run_by(mut self, process: Option<Instance>) -> Self {
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

    /// The host's process, where the host names it (Claude Code).
    pub fn process(&self) -> Option<Instance> {
        self.process
    }

    /// The host's process when it is one of this command's ancestors and
    /// still the process it names: proof, beyond the environment's word,
    /// that this command runs in that host.
    pub(crate) fn ancestor(&self) -> Option<Instance> {
        self.process.filter(|process| {
            process.running()
                && ancestry::ancestors(ANCESTORS)
                    .iter()
                    .any(|ancestor| ancestor.pid == process.pid)
        })
    }

    /// The project folder's name its command works in, never its path.
    pub fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }

    /// When its command started, in milliseconds since 1970: as its notes
    /// say, by this process's start.
    pub fn began(&self) -> Option<u64> {
        self.began
    }

    /// The host session of this process's command, from its environment,
    /// working in the project of its working directory and begun when this
    /// process started; None when no host names one or its value is not a
    /// plain id.
    pub fn from_env() -> Option<Self> {
        let session = Self::resolve(
            |name| std::env::var(name).ok(),
            || ancestry::ancestors(ANCESTORS),
            ancestry::process,
        )?;
        let folder = std::env::current_dir()
            .ok()
            .and_then(|directory| store::project_folder(&directory));
        let began = Instance::current().map(|process| process.started_ms());
        Some(Self {
            folder,
            began,
            ..session
        })
    }

    /// [`HostSession::from_env`] over `env`, with `ancestors` asked only
    /// when two hosts' variables are present, and `process` only for the
    /// process Claude Code names. With two hosts, the nearest is the
    /// session only when the walk finds every one of them; otherwise which
    /// is innermost is not known, and the command has no session.
    pub(crate) fn resolve(
        env: impl Fn(&str) -> Option<String>,
        ancestors: impl FnOnce() -> Vec<Ancestor>,
        process: impl Fn(i32) -> Option<Facts>,
    ) -> Option<Self> {
        let candidates: Vec<(Host, HostId)> = SOURCES
            .iter()
            .filter_map(|(host, names)| {
                names
                    .iter()
                    .find_map(|name| env(name).as_deref().and_then(HostId::parse))
                    .map(|id| (*host, id))
            })
            .collect();
        let claude_pid = candidates
            .iter()
            .any(|(host, _)| *host == Host::Claude)
            .then(|| env("CLAUDE_PID").and_then(|pid| pid.parse::<i32>().ok()))
            .flatten()
            .filter(|pid| *pid > 0);

        let (host, id) = match candidates.len() {
            0 => (
                Host::Named,
                env("ATLAS_SESSION").as_deref().and_then(HostId::parse)?,
            ),
            1 => candidates.into_iter().next()?,
            _ => nearest(candidates, &ancestors(), claude_pid)?,
        };

        let process = claude_pid
            .filter(|_| host == Host::Claude)
            .and_then(|pid| process(pid).map(|facts| Instance::new(pid, facts.start)));

        Some(Self {
            key: Key::of(host, id.as_str()),
            host,
            id,
            process,
            folder: None,
            began: None,
        })
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
    claude_pid: Option<i32>,
) -> Option<(Host, HostId)> {
    let found = |host: &Host| {
        ancestors
            .iter()
            .position(|ancestor| host.runs_as(ancestor, claude_pid))
    };
    let places: Vec<usize> = candidates
        .iter()
        .map(|(host, _)| found(host))
        .collect::<Option<_>>()?;
    let chosen = places
        .iter()
        .enumerate()
        .min_by_key(|(_, place)| **place)
        .map(|(index, _)| index)?;

    candidates.into_iter().nth(chosen)
}

/// Whether `value` may be a host id: 1 to [`ID_MAX`] bytes of ASCII
/// letters, digits, `.`, `_`, `:` and `-`.
pub(crate) fn plain(value: &str) -> bool {
    crate::name::plain(value, ID_MAX)
}

/// Milliseconds since 1970, as records keep times.
pub(crate) fn now_ms() -> u64 {
    u64::try_from(crate::client::timestamp() / 1_000_000).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
