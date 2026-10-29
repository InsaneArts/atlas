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
pub mod store;
#[allow(unsafe_code, reason = "FFI: localtime_r has no safe binding")]
mod sys;

pub use crate::process::Instance;

use crate::{
    control::Cause,
    name::{hex, lower_hex, name_type},
    ui::hints,
};
use ancestry::{Ancestor, Facts};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hex digits of a host session's key, the only part rows carry.
pub const KEY_LEN: usize = 12;
/// The longest host id or label accepted.
pub const ID_MAX: usize = 128;
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

/// Whether every one of `causes` (the pending changes) was claimed by
/// session `own` and none is still being carried out: only then may that
/// session recover them itself. A command an agent started that still runs
/// is work under way, not an unknown outcome of anyone's, so it is passed
/// over. Every surface that says so asks this.
pub(crate) fn owns_every_unknown(causes: &[Cause], own: &Key) -> bool {
    let mut unknown = causes.iter().filter(|cause| !cause.command).peekable();
    unknown.peek().is_some()
        && unknown.all(|cause| cause.session.as_ref() == Some(own) && !cause.running)
}

/// Who asks what to do about changes of unknown outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asker<'a> {
    /// An agent's command, of this host session when it has one.
    Agent(Option<&'a Key>),
    /// The person: from Atlas, or at a terminal with no host session.
    Person,
}

/// What to do about pending changes of unknown outcome: the one rule every
/// surface words its step by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Every one is the asking session's own and none still runs: it
    /// recovers them itself.
    Own,
    /// As [`Recovery::Own`], while a command an agent started still runs:
    /// recovery waits for it to end.
    OwnOnceCommandEnds,
    /// One may be another's: the agent waits, or tells the person.
    Others,
    /// The person asks, and recovers.
    Person,
    /// As [`Recovery::Person`], while a command an agent started still
    /// runs: recovery waits for it to end.
    PersonOnceCommandEnds,
}

impl Recovery {
    /// What `asker` does about the pending changes `causes`; none while
    /// every one is still being carried out, which is work in progress.
    pub fn of(causes: &[Cause], asker: Asker<'_>) -> Option<Self> {
        if causes.iter().all(|cause| cause.running) {
            return None;
        }
        let command_runs = causes.iter().any(|cause| cause.command);
        Some(match asker {
            Asker::Agent(Some(own)) if owns_every_unknown(causes, own) => {
                if command_runs {
                    Self::OwnOnceCommandEnds
                } else {
                    Self::Own
                }
            }
            Asker::Agent(_) => Self::Others,
            Asker::Person if command_runs => Self::PersonOnceCommandEnds,
            Asker::Person => Self::Person,
        })
    }

    /// Whether the asking session recovers them itself.
    pub fn own(self) -> bool {
        matches!(self, Self::Own | Self::OwnOnceCommandEnds)
    }

    /// Its step, in one sentence.
    pub fn step(self) -> &'static str {
        match self {
            Self::Own => hints::UNKNOWN_OWN,
            Self::OwnOnceCommandEnds => hints::UNKNOWN_OWN_ONCE_COMMAND_ENDS,
            Self::Others => hints::UNKNOWN_OTHERS,
            Self::Person => hints::UNKNOWN_FOR_PERSON,
            Self::PersonOnceCommandEnds => hints::UNKNOWN_FOR_PERSON_ONCE_COMMAND_ENDS,
        }
    }
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
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap};

    const CLAUDE_ID: &str = "d9031471-5e2a-4c8b-9f1d-2b7c3a6e0f44";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn knows(pid: i32) -> impl Fn(i32) -> Option<Facts> {
        move |asked| {
            (asked == pid).then(|| Facts {
                parent: 1,
                command: "claude".to_owned(),
                start: [1_761_655_000, 412_233],
            })
        }
    }

    fn ancestor(pid: i32, command: &str) -> Ancestor {
        Ancestor {
            pid,
            command: command.to_owned(),
        }
    }

    fn resolved(pairs: &[(&str, &str)], ancestors: Vec<Ancestor>) -> Option<HostSession> {
        HostSession::resolve(env(pairs), || ancestors, knows(4242))
    }

    #[test]
    fn a_claude_code_command_is_its_session_and_names_its_process() {
        let session = resolved(
            &[
                ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
                ("CLAUDE_PID", "4242"),
            ],
            Vec::new(),
        )
        .unwrap();
        let hex = hex(&Sha256::digest(format!("claude:{CLAUDE_ID}").as_bytes()));

        assert_eq!(session.host, Host::Claude);
        assert_eq!(session.key.as_str(), &hex[..KEY_LEN]);
        assert_eq!(session.id.as_str(), CLAUDE_ID);
        assert_eq!(
            session.process,
            Some(Instance::new(4242, [1_761_655_000, 412_233]))
        );
    }

    #[test]
    fn a_claude_pid_that_is_not_running_is_left_out() {
        let session = HostSession::resolve(
            env(&[
                ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
                ("CLAUDE_PID", "4242"),
            ]),
            Vec::new,
            |_| None,
        )
        .unwrap();

        assert_eq!(session.key, Key::of(Host::Claude, CLAUDE_ID));
        assert_eq!(session.process, None);
    }

    #[test]
    fn codex_prefers_the_root_session_id_to_the_thread_id() {
        let both = resolved(
            &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "thread")],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(both.key, Key::of(Host::Codex, "root"));

        let thread = resolved(&[("CODEX_THREAD_ID", "thread")], Vec::new()).unwrap();
        assert_eq!(thread.key, Key::of(Host::Codex, "thread"));
        assert_eq!(thread.process, None, "Codex names no process");
    }

    #[test]
    fn a_codex_subagent_and_its_parent_are_one_session() {
        let parent = resolved(
            &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "root")],
            Vec::new(),
        );
        let subagent = resolved(
            &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "child")],
            Vec::new(),
        );

        assert_eq!(parent.unwrap().key, subagent.unwrap().key);
    }

    #[test]
    fn pi_and_opencode_are_read_from_their_own_variables() {
        let pi = resolved(&[("PI_SESSION_ID", "pi-1")], Vec::new()).unwrap();
        assert_eq!((pi.host, pi.id.as_str()), (Host::Pi, "pi-1"));

        let opencode = resolved(&[("ATLAS_OPENCODE_SESSION", "ses_7f3a")], Vec::new()).unwrap();
        assert_eq!(
            (opencode.host, opencode.id.as_str()),
            (Host::Opencode, "ses_7f3a")
        );
    }

    #[test]
    fn a_named_session_is_used_only_without_a_host() {
        let named = resolved(&[("ATLAS_SESSION", "release")], Vec::new()).unwrap();
        assert_eq!((named.host, named.id.as_str()), (Host::Named, "release"));

        let hosted = resolved(
            &[("ATLAS_SESSION", "release"), ("CODEX_THREAD_ID", "thread")],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(hosted.host, Host::Codex);
    }

    #[test]
    fn a_value_that_is_not_a_plain_id_is_no_session() {
        let long = "a".repeat(ID_MAX + 1);
        for bad in [long.as_str(), "two words", "a/b", "nul\0", "", "café"] {
            assert_eq!(
                resolved(&[("PI_SESSION_ID", bad)], Vec::new()),
                None,
                "{bad:?}"
            );
            let next = resolved(
                &[("CODEX_SESSION_ID", bad), ("CODEX_THREAD_ID", "thread")],
                Vec::new(),
            )
            .unwrap();
            assert_eq!(next.key, Key::of(Host::Codex, "thread"), "{bad:?}");
        }
        assert!(plain(&"a".repeat(ID_MAX)));
    }

    #[test]
    fn the_nearest_host_wins_when_hosts_are_nested() {
        let pairs = [
            ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
            ("CLAUDE_PID", "4242"),
            ("CODEX_THREAD_ID", "thread"),
        ];
        let codex_inside = vec![
            ancestor(90, "zsh"),
            ancestor(80, "codex"),
            ancestor(70, "zsh"),
            ancestor(4242, "claude"),
        ];
        let codex = resolved(&pairs, codex_inside).unwrap();
        assert_eq!(codex.host, Host::Codex);
        assert_eq!(codex.process, None, "only Claude Code's process is kept");

        let claude_inside = vec![
            ancestor(90, "zsh"),
            ancestor(4242, "claude"),
            ancestor(70, "zsh"),
            ancestor(60, "codex"),
        ];
        let claude = resolved(&pairs, claude_inside).unwrap();
        assert_eq!(claude.host, Host::Claude);
        assert!(claude.process.is_some());
    }

    #[test]
    fn nested_hosts_the_walk_cannot_place_name_no_session() {
        let pairs = [
            ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
            ("CLAUDE_PID", "4242"),
            ("CODEX_THREAD_ID", "thread"),
        ];
        let neither = vec![ancestor(90, "zsh"), ancestor(1, "launchd")];
        assert_eq!(resolved(&pairs, neither), None);

        // Pi runs as node: Claude Code is found, but not Pi inside it.
        let pi_in_claude = [
            ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
            ("CLAUDE_PID", "4242"),
            ("PI_SESSION_ID", "pi-1"),
        ];
        let walk = vec![
            ancestor(90, "zsh"),
            ancestor(80, "node"),
            ancestor(70, "zsh"),
            ancestor(4242, "claude"),
        ];
        assert_eq!(resolved(&pi_in_claude, walk), None);

        // A walk cut short (by its limit, or a process it could not read)
        // before the outer host.
        let cut_short = vec![ancestor(90, "zsh"), ancestor(80, "codex")];
        assert_eq!(resolved(&pairs, cut_short), None);
    }

    #[test]
    fn ancestry_is_asked_only_when_two_hosts_are_present() {
        let asked = RefCell::new(0);
        let ancestors = || {
            *asked.borrow_mut() += 1;
            Vec::new()
        };
        HostSession::resolve(env(&[("PI_SESSION_ID", "pi-1")]), ancestors, |_| None).unwrap();
        assert_eq!(*asked.borrow(), 0);
    }

    #[test]
    fn nothing_reserved_or_private_is_read() {
        let read = RefCell::new(Vec::new());
        let counting = |name: &str| {
            read.borrow_mut().push(name.to_owned());
            None
        };
        assert_eq!(HostSession::resolve(counting, Vec::new, |_| None), None);

        let read = read.into_inner();
        for never in [
            "PI_SESSION_FILE",
            "CLAUDE_CODE_BRIDGE_SESSION_ID",
            "CODEX_TOOL_CALL_ID",
            "CURSOR_TRANSCRIPT_PATH",
            "AI_AGENT",
        ] {
            assert!(!read.iter().any(|name| name == never), "{never}");
        }
        assert!(
            read.iter()
                .all(|name| !name.contains("TRANSCRIPT") && !name.contains("COMPANION")),
            "{read:?}"
        );
    }

    #[test]
    fn a_session_owns_the_unknown_only_when_every_one_is_its_own_and_none_runs() {
        let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
        let cause = |session: Option<&Key>, running: bool| Cause {
            generation: crate::files::token(),
            running,
            command: false,
            session: session.cloned(),
        };

        assert!(!owns_every_unknown(&[], &own), "nothing is unknown");
        assert!(owns_every_unknown(&[cause(Some(&own), false)], &own));
        assert!(!owns_every_unknown(&[cause(Some(&own), true)], &own));
        assert!(!owns_every_unknown(
            &[cause(Some(&own), false), cause(Some(&other), false)],
            &own
        ));
        assert!(!owns_every_unknown(
            &[cause(Some(&own), false), cause(None, false)],
            &own
        ));
    }

    #[test]
    fn a_label_names_the_folder_and_only_a_named_sessions_own_label() {
        let claude = HostSession::new(Host::Claude, CLAUDE_ID)
            .unwrap()
            .in_folder(Some("left"));
        let label = claude.label();
        assert_eq!(label.folder.as_deref(), Some("left"));
        assert_eq!(label.named, None, "a host's own id stays private");

        let named = HostSession::new(Host::Named, "release").unwrap().label();
        assert_eq!(named.named.as_ref().map(HostId::as_str), Some("release"));
        assert_eq!(named.folder, None);
    }

    #[test]
    fn a_running_command_is_nobodys_unknown_outcome_and_recovery_waits_for_it() {
        let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
        let unknown = Cause {
            generation: crate::files::token(),
            running: false,
            command: false,
            session: Some(own.clone()),
        };
        let command = Cause {
            generation: crate::files::token(),
            running: true,
            command: true,
            session: Some(other),
        };
        let of = Recovery::of;
        let beside = [unknown.clone(), command.clone()];

        assert!(owns_every_unknown(&beside, &own));
        assert_eq!(
            of(&beside, Asker::Agent(Some(&own))),
            Some(Recovery::OwnOnceCommandEnds)
        );
        assert_eq!(
            of(&beside, Asker::Person),
            Some(Recovery::PersonOnceCommandEnds)
        );
        assert_eq!(
            of(&[unknown], Asker::Agent(Some(&own))),
            Some(Recovery::Own)
        );
        assert_eq!(
            of(std::slice::from_ref(&command), Asker::Person),
            None,
            "work under way"
        );
        assert!(!owns_every_unknown(&[command], &own), "nothing of its own");
        for step in [
            Recovery::OwnOnceCommandEnds.step(),
            Recovery::PersonOnceCommandEnds.step(),
        ] {
            assert!(step.contains("once the running command ends"), "{step}");
        }
        assert!(Recovery::OwnOnceCommandEnds.own());
    }

    /// The guide tells an agent whose command its host stopped to run
    /// doctor and restart only when it says every unknown action is its
    /// own. Doctor's step is [`Recovery::step`]: it names a restart only for
    /// a session that owns every unknown change, never for mixed sessions'
    /// changes or one whose session is not known, and a command stopped
    /// while it only waited left nothing to recover.
    #[test]
    fn the_guide_restarts_after_a_stopped_command_only_when_recovery_says_so() {
        let core = crate::agent::CORE
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(core.contains(
            "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours."
        ));

        let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
        let cause = |session: Option<&Key>| Cause {
            generation: crate::files::token(),
            running: false,
            command: false,
            session: session.cloned(),
        };
        let asker = Asker::Agent(Some(&own));
        assert_eq!(Recovery::of(&[], asker), None, "only waiting");
        for (causes, yours) in [
            (vec![cause(Some(&own))], true),
            (vec![cause(Some(&own)), cause(Some(&other))], false),
            (vec![cause(Some(&own)), cause(None)], false),
            (vec![cause(Some(&other))], false),
        ] {
            let recovery = Recovery::of(&causes, asker).unwrap();
            assert_eq!(recovery.own(), yours, "{recovery:?}");
            assert_eq!(
                recovery.step().contains("atlas restart"),
                yours,
                "{}",
                recovery.step()
            );
        }
    }

    #[test]
    fn only_the_session_that_claimed_every_unknown_change_recovers_it_itself() {
        let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
        let cause = |session: Option<&Key>, running: bool| Cause {
            generation: crate::files::token(),
            running,
            command: false,
            session: session.cloned(),
        };
        let of = Recovery::of;

        assert_eq!(
            of(&[], Asker::Agent(Some(&own))),
            None,
            "nothing is pending"
        );
        assert_eq!(
            of(&[cause(Some(&own), true)], Asker::Person),
            None,
            "a change still being carried out is work in progress"
        );
        let mine = [cause(Some(&own), false)];
        assert_eq!(of(&mine, Asker::Agent(Some(&own))), Some(Recovery::Own));
        assert_eq!(
            of(&mine, Asker::Agent(Some(&other))),
            Some(Recovery::Others)
        );
        assert_eq!(of(&mine, Asker::Agent(None)), Some(Recovery::Others));
        assert_eq!(of(&mine, Asker::Person), Some(Recovery::Person));
        for shared in [
            [cause(Some(&own), false), cause(Some(&other), false)],
            [cause(Some(&own), false), cause(None, false)],
            [cause(Some(&own), false), cause(Some(&own), true)],
        ] {
            assert_eq!(
                of(&shared, Asker::Agent(Some(&own))),
                Some(Recovery::Others)
            );
        }

        assert!(Recovery::Own.step().contains("atlas restart"));
        assert!(Recovery::Person.step().contains("atlas restart"));
        let others = Recovery::Others.step();
        assert!(
            !others.contains("atlas "),
            "no command for another's action"
        );
        assert!(others.contains("tell the person"), "{others}");
    }

    #[test]
    fn a_key_is_twelve_lowercase_hex_digits() {
        let key = Key::of(Host::Pi, "pi-1");
        assert_eq!(Key::parse(key.as_str()), Some(key.clone()));
        for bad in [
            "3F9A1C07B2E4",
            "3f9a1c07b2e",
            "3f9a1c07b2e4f",
            "3f9a1c07b2eg",
        ] {
            assert_eq!(Key::parse(bad), None, "{bad}");
        }
        let through: Key = serde_json::from_value(serde_json::json!(key.as_str())).unwrap();
        assert_eq!(through, key);
        assert!(serde_json::from_value::<Key>(serde_json::json!("../x")).is_err());
    }
}
