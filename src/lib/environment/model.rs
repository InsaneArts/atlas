//! Versioned, private records. File contents live in separate bounded blobs.
use super::{reply::FileReport, *};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Identity {
    pub uid: u32,
    pub capture_pid: u32,
    pub security_session_id: u64,
    pub display_id: u64,
    pub width: f64,
    pub height: f64,
}

impl Identity {
    pub(super) fn from_epoch(epoch: &crate::session::Epoch) -> Self {
        Self {
            uid: epoch.uid,
            capture_pid: epoch.capture_pid,
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            width: epoch.display.width,
            height: epoch.display.height,
        }
    }

    pub(super) fn from_proof(proof: &Value) -> Result<Self> {
        let epoch = crate::session::InputEpoch::from_proof(proof, 1)?;
        Ok(Self {
            uid: epoch.uid,
            capture_pid: proof["parentPID"]
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|pid| *pid > 0)
                .ok_or_else(|| problem("Missing environment worker parent identity"))?,
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            width: epoch.width,
            height: epoch.height,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Packet {
    pub version: u32,
    pub expected: Identity,
    pub request: Request,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Preview {
        id: String,
        files: Vec<Payload>,
    },
    Apply {
        plan: Plan,
    },
    Rollback {
        id: String,
        apply: bool,
    },
    Run {
        argv: Vec<String>,
    },
    Transactions {
        all: bool,
        ids: Vec<String>,
    },
    InstallApp {
        id: String,
        source: PathBuf,
        name: String,
    },
    RemoveApp {
        id: String,
    },
    Installs {
        ids: Vec<String>,
    },
}

impl Request {
    /// Whether the worker may change the helper for this request. Every
    /// other request only reads, so it takes no control and leaves nothing
    /// pending; a new request must say which it is.
    pub(super) fn changes_helper(&self) -> bool {
        match self {
            Request::Preview { .. } | Request::Transactions { .. } | Request::Installs { .. } => {
                false
            }
            Request::Rollback { apply, .. } => *apply,
            Request::Apply { .. }
            | Request::Run { .. }
            | Request::InstallApp { .. }
            | Request::RemoveApp { .. } => true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Payload {
    pub source: PathBuf,
    pub target: PathBuf,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Node {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Fingerprint {
    pub node: Node,
    pub sha256: String,
    pub size: u64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    // Unsupported metadata is refused, not omitted from comparisons.
    pub acl: bool,
    pub xattrs: BTreeMap<String, String>,
    pub flags: u32,
}

/// Two fingerprints of one file match whatever macOS's own attributes say:
/// the system sets those itself, so they are kept but never compared.
impl PartialEq for Fingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.same_contents_and_metadata(other)
    }
}

impl Fingerprint {
    pub(super) fn same_contents_and_metadata(&self, other: &Self) -> bool {
        let Self {
            node: _,
            sha256,
            size,
            mode,
            uid,
            gid,
            acl,
            xattrs,
            flags,
        } = self;

        *sha256 == other.sha256
            && *size == other.size
            && *mode == other.mode
            && *uid == other.uid
            && *gid == other.gid
            && *acl == other.acl
            && owned_attributes(xattrs).eq(owned_attributes(&other.xattrs))
            && *flags == other.flags
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(super) enum Entry {
    Legacy(String),
    Managed {
        generation: String,
        fingerprint: Fingerprint,
    },
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    #[serde(default)]
    pub version: u32,
    pub files: BTreeMap<PathBuf, Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Plan {
    pub version: u32,
    pub id: String,
    pub uid: u32,
    pub home: PathBuf,
    pub files: Vec<PlannedFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PlannedFile {
    pub payload: Payload,
    pub before: Option<Fingerprint>,
    pub previous: Option<Entry>,
    pub operation: Operation,
}

/// What a plan does to one destination, stored by its lowercase name.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Operation {
    Create,
    Update,
    Permissions,
    Unchanged,
    Conflict,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Directory {
    pub target: PathBuf,
    // None is a write-ahead intent. After a crash it does not prove ownership.
    pub node: Option<Node>,
}

/// Write-ahead evidence can outlive an interrupted effect or acknowledgement.
/// Restored files may have been untouched or kept, without ever being applied.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Progress {
    prepared: Option<Fingerprint>,
    applied: bool,
    restore: Option<Fingerprint>,
    reverting: bool,
    restored: bool,
    /// Undoing left the file as the helper changed it. Written only when
    /// set, so records stay readable by earlier versions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    kept: bool,
}

impl Progress {
    pub(super) fn prepared(&self) -> &Option<Fingerprint> {
        &self.prepared
    }

    pub(super) fn applied(&self) -> bool {
        self.applied
    }

    pub(super) fn restore(&self) -> &Option<Fingerprint> {
        &self.restore
    }

    pub(super) fn reverting(&self) -> bool {
        self.reverting
    }

    pub(super) fn restored(&self) -> bool {
        self.restored
    }

    pub(super) fn kept(&self) -> bool {
        self.kept
    }

    pub(super) fn record_prepared(&mut self, fingerprint: Fingerprint) {
        self.prepared = Some(fingerprint);
    }

    pub(super) fn acknowledge_applied(&mut self) {
        self.applied = true;
    }

    pub(super) fn record_restore(&mut self, fingerprint: Option<Fingerprint>) {
        self.restore = fingerprint;
    }

    pub(super) fn begin_reverting(&mut self) {
        self.reverting = true;
    }

    pub(super) fn acknowledge_restored(&mut self) {
        self.restored = true;
    }

    pub(super) fn complete_kept(&mut self) {
        self.kept = true;
        self.restored = true;
    }

    pub(super) fn release_untouched(&mut self) {
        self.prepared = None;
        self.restored = true;
    }

    fn valid(&self, file: &PlannedFile, phase: Phase) -> bool {
        if (self.kept && !self.restored)
            || (self.applied && file.operation != Operation::Unchanged && self.prepared.is_none())
            || (self.restore.is_some() && file.before.is_none())
            || (self.reverting && file.before.is_some() && self.restore.is_none())
            || (self.restored && !self.kept && !self.reverting && self.applied)
        {
            return false;
        }

        match phase {
            Phase::Applying | Phase::Applied => {
                self.restore.is_none()
                    && !self.reverting
                    && !self.restored
                    && !self.kept
                    && (phase != Phase::Applied || self.applied)
            }
            Phase::RollingBack => true,
            Phase::RolledBack => self.restored,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Phase {
    Applying,
    Applied,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Transaction {
    pub version: u32,
    pub plan: Plan,
    pub phase: Phase,
    pub progress: Vec<Progress>,
    pub directories: Vec<Directory>,
    #[serde(default)]
    pub retained_directories: Vec<PathBuf>,
}

impl Transaction {
    pub(super) fn valid_progress(&self) -> bool {
        self.progress.len() == self.plan.files.len()
            && self
                .progress
                .iter()
                .zip(&self.plan.files)
                .all(|(progress, file)| progress.valid(file, self.phase))
    }
}

pub(super) fn validate_id(id: &str) -> Result<()> {
    if !crate::name::lower_hex(id, 48) {
        return Err(problem(
            "Expected a 48-character lowercase plan ID, the planId a preview (atlas \
             environment MANIFEST) printed",
        ));
    }

    Ok(())
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    // Serde errors can include selected values; do not expose them in logs.
    serde_json::from_slice(bytes).map_err(|_| problem("Invalid environment record or schema"))
}

pub(super) fn file_report(plan: &Plan) -> Vec<FileReport> {
    plan.files
        .iter()
        .map(|file| FileReport {
            target: file.payload.target.clone(),
            operation: file.operation.into(),
            legacy_previous: matches!(file.previous, Some(Entry::Legacy(_))),
        })
        .collect()
}
