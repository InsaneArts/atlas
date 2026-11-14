//! What a guest's one-file stores share: reading a record no link or pipe
//! can stand in for, its number read without trusting the rest, and its
//! name kept for good.
use std::{
    io::{ErrorKind, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// The file at `path`, when it is a file of its own (never a link or a
/// pipe) of at most `limit` bytes; none when there is none.
///
/// # Errors
///
/// Fails when it cannot be opened or read, or is not such a file.
pub(super) fn read_regular(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    let file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(std::io::Error::other("not a record"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// The record's number, as it says, without trusting the rest.
pub(super) fn number(bytes: &[u8]) -> u64 {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|record| record["number"].as_u64())
        .unwrap_or(u64::MAX)
}

/// Keep what changed beside `path`, its name included, for good.
///
/// # Errors
///
/// Fails when its folder cannot be synced.
pub(super) fn sync_folder(path: &Path) -> std::io::Result<()> {
    let folder = path.parent().unwrap_or(Path::new("."));
    std::fs::File::open(folder)?.sync_all()
}
