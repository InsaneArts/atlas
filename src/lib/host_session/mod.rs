//! The agent conversation a command comes from, as its host tells the
//! shell: a Claude Code session, a Codex thread, an opencode or Pi session.
//! It is a label for the person and the agent, never a permission. Atlas's
//! own "session" is the helper's login; this is the host's.
pub mod activity;
pub(crate) mod ancestry;
pub(crate) mod brief;
pub mod explain;
pub(crate) mod liveness;
pub(crate) mod names;
pub(crate) mod record;
mod recovery;
pub mod store;
pub(crate) mod verification;

pub use crate::process::Instance;
pub use recovery::{Asker, Recovery};
pub use verification::HostClaim;

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
    fn pid_hint(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("CLAUDE_PID"),
            Self::Opencode => Some("ATLAS_OPENCODE_PID"),
            Self::Pi => Some("ATLAS_PI_PID"),
            _ => None,
        }
    }

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
    /// The verified root host's exact process instance.
    process: Option<Instance>,
    /// The previous exact host instance the daemon proved has exited.
    replaces: Option<Instance>,
    /// The root host's project folder basename, or the legacy fallback.
    folder: Option<String>,
    /// When its command started, in milliseconds since 1970.
    began: Option<u64>,
    folder_from_host: bool,
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

    /// The verified root host's exact process instance.
    pub fn process(&self) -> Option<Instance> {
        self.process
    }

    /// The host's process when it is one of this command's ancestors and
    /// still the process it names: proof, beyond the environment's word,
    /// that this command runs in that host.
    pub(crate) fn ancestor(&self) -> Option<Instance> {
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

    fn resolve_for(
        env: impl Fn(&str) -> Option<String>,
        ancestors: impl FnOnce() -> Vec<Ancestor>,
        process: impl Fn(i32) -> Option<Facts>,
        uid: u32,
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
        if candidates.is_empty() {
            return Self::new(Host::Named, env("ATLAS_SESSION")?.as_str());
        }
        let hint = |host: Host| {
            let name = host.pid_hint()?;
            env(name)?.parse::<i32>().ok().filter(|pid| *pid > 0)
        };
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
        let selected =
            (!ambiguous && host != Host::Cursor && matching.len() == 1).then(|| matching[0]);
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
                self.folder = store::project_folder(&directory);
                self.folder_from_host = true;
            }
        } else if !self.ambiguous {
            self.folder = command_cwd.and_then(|directory| store::project_folder(&directory));
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
    let found = |host: &Host| {
        ancestors
            .iter()
            .position(|ancestor| host.runs_as(ancestor, hint(*host), uid))
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
