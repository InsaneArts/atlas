//! SHA-256 of executables and other regular files.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

/// The executable a shared probe path stands for: the file itself, or the
/// Atlas.app executable it links to when the link belongs to this
/// account. Any other link is not followed.
pub fn probe_executable(path: &Path) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.is_file() {
        return Some(path.to_owned());
    }

    if !metadata.file_type().is_symlink() || metadata.uid() != nix::unistd::getuid().as_raw() {
        return None;
    }

    let target = fs::canonicalize(path).ok()?;
    let in_app = target
        .parent()
        .is_some_and(|directory| directory.ends_with("Contents/MacOS"))
        && target
            .ancestors()
            .nth(3)
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "app");

    in_app.then_some(target)
}

/// SHA-256 of a regular file, read without following a final symlink.
pub fn file_sha256(path: &Path) -> Option<String> {
    sha256(&crate::files::open_plain(path)?)
}

/// SHA-256 of `bytes`, in hex.
#[must_use]
pub fn bytes_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// SHA-256 of everything left to read in `file`.
pub fn sha256(file: &fs::File) -> Option<String> {
    read_sha256(file).ok()
}

/// [`sha256`], saying why a read failed.
///
/// # Errors
///
/// Returns the I/O error of the read.
pub fn read_sha256(mut file: &fs::File) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests;
