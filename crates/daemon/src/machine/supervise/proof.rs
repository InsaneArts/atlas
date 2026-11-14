//! A supervisor's word, left on disk, that it ended everything it started:
//! what a daemon that never saw it exit goes by. It is kept beside the
//! records, one file per supervisor, written whole and kept for good before
//! the supervisor exits.
use atlas_core::{files::write_atomic, process::Instance};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The longest a proof may be.
const LIMIT: u64 = 256;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    supervisor: Instance,
    code: u8,
}

/// Where `supervisor`'s proof is kept in `folder`.
fn path(folder: &Path, supervisor: &Instance) -> PathBuf {
    let [seconds, micros] = supervisor.start;
    folder.join(format!("{}-{seconds}-{micros}.json", supervisor.pid))
}

/// Leave the proof that `supervisor` ended with `code`, kept for good.
///
/// # Errors
///
/// Fails when it cannot be written or kept.
pub fn leave(folder: &Path, supervisor: &Instance, code: u8) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&Proof {
        supervisor: *supervisor,
        code,
    })?;
    let path = path(folder, supervisor);
    write_atomic(&path, &bytes, 0o600).map_err(std::io::Error::other)?;
    std::fs::File::open(folder)?.sync_all()
}

/// The code `supervisor` left as its proof; none when it left none, or
/// what is there is not its proof.
pub fn left(folder: &Path, supervisor: &Instance) -> Option<u8> {
    let bytes =
        super::super::record_file::read_regular(&path(folder, supervisor), LIMIT).ok()??;
    let proof: Proof = serde_json::from_slice(&bytes).ok()?;
    (proof.supervisor == *supervisor).then_some(proof.code)
}

/// Forget `supervisor`'s proof, once its record is gone.
///
/// # Errors
///
/// Fails when it is there and cannot be removed.
pub fn forget(folder: &Path, supervisor: &Instance) -> std::io::Result<()> {
    match std::fs::remove_file(path(folder, supervisor)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
