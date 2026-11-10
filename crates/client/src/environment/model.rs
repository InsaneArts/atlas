//! Versioned, private records. File contents live in separate bounded blobs.
use super::{reply::FileReport, *};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Identity {
    pub(super) uid: u32,
    pub(super) capture_pid: u32,
    pub(super) security_session_id: u64,
    pub(super) display_id: u64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl Identity {
    pub(super) fn from_epoch(epoch: &atlas_core::session::Epoch) -> Self {
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
        let epoch = atlas_core::session::InputEpoch::from_proof(proof, 1)?;
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
    pub(super) version: u32,
    pub(super) expected: Identity,
    pub(super) request: Request,
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

    /// Whether the worker's output is a command's own, not its JSON reply.
    pub(super) fn command_output(&self) -> bool {
        match self {
            Request::Run { .. } => true,
            Request::Preview { .. }
            | Request::Apply { .. }
            | Request::Rollback { .. }
            | Request::Transactions { .. }
            | Request::InstallApp { .. }
            | Request::RemoveApp { .. }
            | Request::Installs { .. } => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Payload {
    pub(super) source: PathBuf,
    pub(super) target: PathBuf,
    pub(super) sha256: String,
    pub(super) executable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Node {
    pub(crate) device: u64,
    pub(crate) inode: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Fingerprint {
    pub(super) node: Node,
    pub(super) sha256: String,
    pub(super) size: u64,
    pub(super) mode: u32,
    pub(super) uid: u32,
    pub(super) gid: u32,
    // Unsupported metadata is refused, not omitted from comparisons.
    pub(super) acl: bool,
    pub(super) xattrs: BTreeMap<String, String>,
    pub(super) flags: u32,
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
pub(super) struct Entry {
    pub(super) generation: String,
    pub(super) fingerprint: Fingerprint,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    pub(super) version: u32,
    pub(super) files: BTreeMap<PathBuf, Entry>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            version: 1,
            files: BTreeMap::new(),
        }
    }
}

impl Ledger {
    /// Put back the entry `target` had, or none when it had none.
    pub(super) fn restore(&mut self, target: &Path, previous: Option<Entry>) {
        match previous {
            Some(previous) => self.files.insert(target.to_owned(), previous),
            None => self.files.remove(target),
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Plan {
    pub(super) version: u32,
    pub(super) id: String,
    pub(super) uid: u32,
    pub(super) home: PathBuf,
    pub(super) files: Vec<PlannedFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PlannedFile {
    pub(super) payload: Payload,
    pub(super) before: Option<Fingerprint>,
    pub(super) previous: Option<Entry>,
    pub(super) operation: Operation,
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

impl Operation {
    /// Whether the plan puts a file of its own in place; one that finds the
    /// file as selected only adopts it.
    pub(super) fn places_a_copy(self) -> bool {
        match self {
            Self::Create | Self::Update | Self::Permissions | Self::Conflict => true,
            Self::Unchanged => false,
        }
    }

    /// Whether the plan brings contents new to a file that is already there.
    pub(super) fn new_contents(self) -> bool {
        match self {
            Self::Update => true,
            Self::Create | Self::Permissions | Self::Unchanged | Self::Conflict => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Directory {
    pub(super) target: PathBuf,
    // None is a write-ahead intent. After a crash it does not prove ownership.
    pub(super) node: Option<Node>,
}

/// Write-ahead evidence can outlive an interrupted effect or acknowledgement.
/// Restored files may have been untouched or kept, without ever being applied.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Progress {
    pub(super) prepared: Option<Fingerprint>,
    pub(super) applied: bool,
    pub(super) restore: Option<Fingerprint>,
    pub(super) reverting: bool,
    pub(super) restored: bool,
    /// Undoing left the file as the helper changed it. Written only when
    /// set, so records stay readable by earlier versions.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) kept: bool,
}

impl Progress {
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
            || (self.applied && file.operation.places_a_copy() && self.prepared.is_none())
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
pub enum Phase {
    Applying,
    Applied,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Transaction {
    pub(super) version: u32,
    pub(super) plan: Plan,
    pub(super) phase: Phase,
    pub(super) progress: Vec<Progress>,
    pub(super) directories: Vec<Directory>,
    #[serde(default)]
    pub(super) retained_directories: Vec<PathBuf>,
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
    if !atlas_core::name::lower_hex(id, 48) {
        return Err(problem(
            "Expected a 48-character lowercase plan ID, the planId a preview (atlas \
             environment MANIFEST) printed",
        ));
    }

    Ok(())
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    // Serde errors can include selected values; do not expose them in logs.
    serde_json::from_slice(bytes)
        .map_err(|cause| problem("Invalid environment record or schema").with_source(cause))
}

pub(super) fn file_report(plan: &Plan) -> Vec<FileReport> {
    plan.files
        .iter()
        .map(|file| FileReport {
            target: file.payload.target.clone(),
            operation: file.operation.into(),
            legacy_previous: false,
        })
        .collect()
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
