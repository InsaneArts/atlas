//! SHA-256 of executables and other regular files.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

/// SHA-256 of a regular file, read without following a final symlink.
pub fn file_sha256(path: &Path) -> Option<String> {
    if !fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::file_sha256;

    #[test]
    fn hashes_regular_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            file_sha256(&file).as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert_eq!(file_sha256(&link), None);
        assert_eq!(file_sha256(dir.path()), None);
    }
}
