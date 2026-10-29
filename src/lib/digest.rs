//! SHA-256 of executables and other regular files.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

/// Whether `text` is exactly `digits` lowercase hexadecimal digits, as
/// Atlas writes digests and random names.
pub(crate) fn lower_hex(text: &str, digits: usize) -> bool {
    text.len() == digits
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

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

/// SHA-256 of everything left to read in `file`.
pub(crate) fn sha256(mut file: &fs::File) -> Option<String> {
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

        let fifo = dir.path().join("fifo");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
        assert_eq!(file_sha256(&fifo), None, "never waits for a writer");
    }

    #[test]
    fn a_probe_follows_only_its_own_link_into_an_app() {
        use super::probe_executable;

        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let app = root.join("Atlas.app/Contents/MacOS/atlas");
        std::fs::create_dir_all(app.parent().unwrap()).unwrap();
        std::fs::write(&app, b"x").unwrap();

        let link = root.join("probe");
        std::os::unix::fs::symlink(&app, &link).unwrap();
        assert_eq!(probe_executable(&link), Some(app.clone()));
        assert_eq!(probe_executable(&app), Some(app.clone()), "a plain file");

        let elsewhere = root.join("other");
        std::fs::write(&elsewhere, b"y").unwrap();
        let stray = root.join("stray");
        std::os::unix::fs::symlink(&elsewhere, &stray).unwrap();
        assert_eq!(probe_executable(&stray), None, "links only into an app");
    }
}
