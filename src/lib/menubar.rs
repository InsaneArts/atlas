//! The macOS menu bar companion: a small AppKit status item built from Swift
//! source embedded in this binary. It renders the local viewer's status and
//! frame and can ask it to release control; it reaches nothing else. The
//! capability URL travels on its stdin, never in argv, and closing stdin
//! makes it quit.
use crate::{
    config::home,
    control::token,
    error::{Error, Result},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, Command},
};

const SOURCE: &str = include_str!("../../menubar/UnderstudyBar.swift");
const SWIFTC: &str = "/usr/bin/swiftc";
const NAME: &str = "UnderstudyBar";

/// One build per source version: the first 16 hex digits of its digest.
pub fn version() -> String {
    format!("{:x}", Sha256::digest(SOURCE.as_bytes()))[..16].to_owned()
}

/// Only `uid` or root can change `path`: it is not a symlink, one of them
/// owns it, no group or other may write it, and no ACL entry allows a write.
fn exclusive(path: &Path, uid: u32) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let owner = metadata.uid();
    if metadata.file_type().is_symlink()
        || (owner != uid && owner != 0)
        || metadata.mode() & 0o022 != 0
        || crate::acl::allows_write(path)?
    {
        return Err(Error::new(
            "menubar",
            format!(
                "{} can be changed by another account; the menu bar will not run from it",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// The canonical form of `path` once every directory from the root down to
/// it is exclusive, so nothing but root can swap what it names.
fn exclusive_chain(path: &Path, uid: u32) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    for component in canonical.ancestors() {
        exclusive(component, uid)?;
    }
    Ok(canonical)
}

/// Create `path` 0700 if absent; it must be a directory this user owns.
fn private_dir(path: &Path, uid: u32) -> Result<()> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid {
        return Err(Error::new(
            "menubar",
            format!("{} must be a directory you own", path.display()),
        ));
    }
    exclusive(path, uid)
}

/// The first non-empty line of compiler output, bounded for reports.
fn first_line(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no output")
        .chars()
        .take(300)
        .collect()
}

/// The embedded source in a new file only this user can write: created
/// exclusively (never an existing inode, hard link or symlink), 0600, and
/// checked for ACLs before the compiler reads it.
fn fresh_source(directory: &Path, uid: u32) -> Result<PathBuf> {
    let path = directory.join(format!("{NAME}.{}.swift", token()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    std::io::Write::write_all(&mut file, SOURCE.as_bytes())?;
    file.sync_all()?;
    if let Err(error) = exclusive(&path, uid) {
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

/// The companion executable under `root`, compiled on first use of this
/// source version.
fn build(root: &Path, uid: u32) -> Result<PathBuf> {
    private_dir(root, uid)?;
    private_dir(&root.join(version()), uid)?;
    let directory = exclusive_chain(&root.join(version()), uid)?;
    let executable = directory.join(NAME);
    match fs::symlink_metadata(&executable) {
        Ok(metadata) if metadata.is_file() && metadata.uid() == uid => {
            exclusive(&executable, uid)?;
            return Ok(executable);
        }
        Ok(_) => {
            return Err(Error::new(
                "menubar",
                format!("{} is not a file you own", executable.display()),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let source = fresh_source(&directory, uid)?;
    let temporary = directory.join(format!("{NAME}.{}.new", token()));
    let output = std::process::Command::new(SWIFTC)
        .args(["-O", "-framework", "AppKit", "-o"])
        .arg(&temporary)
        .arg(&source)
        .stdin(Stdio::null())
        .output();
    let _ = fs::remove_file(&source);
    let output = output.map_err(|_| {
        Error::new(
            "menubar",
            "Building the menu bar needs the Xcode Command Line Tools; run xcode-select --install",
        )
    })?;
    if !output.status.success() {
        let _ = fs::remove_file(&temporary);
        return Err(Error::new(
            "menubar",
            format!("The menu bar did not build: {}", first_line(&output.stderr)),
        ));
    }
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))?;
    fs::rename(&temporary, &executable)?;
    exclusive(&executable, uid)?;
    Ok(executable)
}

/// The companion executable for this user, built if needed.
pub fn executable() -> Result<PathBuf> {
    let uid = nix::unistd::getuid().as_raw();
    build(
        &home()?.join("Library/Application Support/Understudy/menubar"),
        uid,
    )
}

/// Start the companion with `args` and hand it the URL on stdin.
async fn spawn(executable: &Path, url: &str, args: &[&str]) -> Result<(Child, ChildStdin)> {
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::new("menubar", "The menu bar has no input"))?;
    stdin.write_all(format!("{url}\n").as_bytes()).await?;
    stdin.flush().await?;
    Ok((child, stdin))
}

/// A running companion. Dropping it closes its stdin, which makes it quit,
/// and kills it if it has not.
pub struct Companion {
    child: Child,
    _stdin: ChildStdin,
}

impl Companion {
    pub async fn launch(executable: &Path, url: &str) -> Result<Self> {
        let (child, stdin) = spawn(executable, url, &[]).await?;
        Ok(Self {
            child,
            _stdin: stdin,
        })
    }
    /// Resolves when the person quits the companion.
    pub async fn exited(&mut self) {
        let _ = self.child.wait().await;
    }
}

/// The arguments that render the panel to `path` in one appearance.
fn render_args(path: &str, dark: bool) -> Vec<&str> {
    let mut args = vec!["--render", path];
    if dark {
        args.push("--dark");
    }
    args
}

/// Draw the companion's panel, fed by the viewer at `url`, to a PNG.
pub async fn render(executable: &Path, url: &str, path: &Path, dark: bool) -> Result<()> {
    let target = path
        .to_str()
        .ok_or_else(|| Error::new("usage", "The render path must be UTF-8"))?;
    let (mut child, _stdin) = spawn(executable, url, &render_args(target, dark)).await?;
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .map_err(|_| Error::new("menubar", "The menu bar did not finish rendering"))??;
    if !status.success() {
        return Err(Error::new(
            "menubar",
            format!("The menu bar could not render ({status})"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uid() -> u32 {
        nix::unistd::getuid().as_raw()
    }

    #[test]
    fn the_version_follows_the_embedded_source() {
        assert_eq!(version().len(), 16);
        assert_eq!(version(), version());
        assert!(SOURCE.contains("readLine()"), "the URL arrives on stdin");
        assert!(!SOURCE.contains("CommandLine.arguments.first"));
    }

    fn allow_write(path: &Path) {
        let user = std::env::var("USER").unwrap();
        let added = std::process::Command::new("/bin/chmod")
            .args(["+a", &format!("user:{user} allow write")])
            .arg(path)
            .status()
            .unwrap();
        assert!(added.success());
    }

    #[test]
    fn build_directories_must_be_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("a/b");
        private_dir(&fresh, uid()).unwrap();
        assert_eq!(fs::metadata(&fresh).unwrap().mode() & 0o777, 0o700);
        exclusive_chain(&fresh, uid()).unwrap();
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(private_dir(&shared, uid()).unwrap_err().code, "menubar");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&fresh, &link).unwrap();
        assert!(private_dir(&link, uid()).is_err(), "never a symlink itself");
        assert!(
            private_dir(&fresh, uid() + 1).is_err(),
            "never someone else's"
        );
        let granted = dir.path().join("granted");
        private_dir(&granted, uid()).unwrap();
        allow_write(&granted);
        assert!(
            private_dir(&granted, uid()).is_err(),
            "no ACL may allow writes"
        );
    }

    #[test]
    fn every_directory_above_the_build_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        // A writable parent could swap the build between check and launch.
        let open = dir.path().join("open");
        fs::create_dir(&open).unwrap();
        let inner = open.join("cache");
        private_dir(&inner, uid()).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(exclusive_chain(&inner, uid()).is_err());
        // So could one reached through an intermediate symlink.
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&open, &alias).unwrap();
        assert!(exclusive_chain(&alias.join("cache"), uid()).is_err());
        fs::set_permissions(&open, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            exclusive_chain(&alias.join("cache"), uid()).unwrap(),
            fs::canonicalize(&inner).unwrap(),
            "a symlink resolves to its exclusive target"
        );
    }

    #[test]
    fn an_existing_build_is_reused_only_when_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let built = dir.path().join(version());
        private_dir(&built, uid()).unwrap();
        let executable = built.join(NAME);
        fs::write(&executable, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let canonical = fs::canonicalize(&executable).unwrap();
        assert_eq!(build(dir.path(), uid()).unwrap(), canonical);
        allow_write(&executable);
        assert_eq!(build(dir.path(), uid()).unwrap_err().code, "menubar");
        crate::acl::clear(&fs::File::open(&executable).unwrap()).unwrap();
        assert_eq!(build(dir.path(), uid()).unwrap(), canonical);
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o722)).unwrap();
        assert_eq!(build(dir.path(), uid()).unwrap_err().code, "menubar");
    }

    #[test]
    fn the_compiler_reads_a_new_private_source_every_time() {
        let dir = tempfile::tempdir().unwrap();
        // A leftover, permissive source from an older layout is never used.
        let stale = dir.path().join(format!("{NAME}.swift"));
        fs::write(&stale, b"tampered").unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o666)).unwrap();
        let first = fresh_source(dir.path(), uid()).unwrap();
        let second = fresh_source(dir.path(), uid()).unwrap();
        assert_ne!(first, second);
        assert_ne!(first, stale);
        let metadata = fs::metadata(&first).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.nlink(), 1, "no other hard link");
        assert_eq!(fs::read_to_string(&first).unwrap(), SOURCE);
    }

    #[test]
    fn rendering_selects_the_appearance() {
        assert_eq!(render_args("/tmp/a.png", false), ["--render", "/tmp/a.png"]);
        assert_eq!(
            render_args("/tmp/a.png", true),
            ["--render", "/tmp/a.png", "--dark"]
        );
        assert_eq!(first_line(b"\n\nerror: x\nmore"), "error: x");
    }
}
