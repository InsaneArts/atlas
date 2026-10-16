//! Versioned, private records. File contents live in separate bounded blobs.
use super::*;

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
    pub fn from_epoch(epoch: &crate::session::Epoch) -> Self {
        Self {
            uid: epoch.uid,
            capture_pid: epoch.capture_pid,
            security_session_id: epoch.security_session_id,
            display_id: epoch.display_id,
            width: epoch.display.width,
            height: epoch.display.height,
        }
    }

    pub fn from_proof(proof: &Value) -> Result<Self> {
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
    Preview { id: String, files: Vec<Payload> },
    Apply { plan: Plan },
    Rollback { id: String, apply: bool },
    Run { argv: Vec<String> },
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
pub(super) struct Node {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

impl Fingerprint {
    pub fn same_contents_and_metadata(&self, other: &Self) -> bool {
        let mut comparable = self.clone();
        comparable.node = other.node.clone();
        comparable == *other
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
pub(super) enum Operation {
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Progress {
    pub prepared: Option<Fingerprint>,
    pub applied: bool,
    pub restore: Option<Fingerprint>,
    pub reverting: bool,
    pub restored: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum Phase {
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

pub(super) fn validate_id(id: &str) -> Result<()> {
    if id.len() != 48
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(problem(
            "Expected a 48-character lowercase environment plan ID",
        ));
    }
    Ok(())
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    // Serde errors can include selected values; do not expose them in logs.
    serde_json::from_slice(bytes).map_err(|_| problem("Invalid environment record or schema"))
}

pub(super) fn file_report(plan: &Plan) -> Vec<Value> {
    plan.files
        .iter()
        .map(|file| {
            json!({
                "target": file.payload.target,
                "operation": file.operation,
                "legacyPrevious": matches!(file.previous, Some(Entry::Legacy(_))),
            })
        })
        .collect()
}
