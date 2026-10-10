//! `understudy update`: replace the installed Understudy.app with the latest
//! release after the same checks as install.sh — checksum, signature, the
//! Developer ID requirement and notarization — swapping by rename so a
//! running helper daemon never sees a half-written executable.
use crate::{
    error::{Error, Result},
    setup,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
};

const REPO: &str = "InsaneArts/understudy";

/// Where releases come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A directory URL holding the archive and SHA256SUMS (file:// or https).
    Mirror(String),
    /// A GitHub release: the latest, or a tag.
    GitHub { tag: Option<String> },
}

impl Source {
    /// The mirror in `UNDERSTUDY_DOWNLOAD_BASE`, else GitHub.
    pub fn from_env(tag: Option<String>) -> Self {
        match std::env::var("UNDERSTUDY_DOWNLOAD_BASE") {
            Ok(base) if !base.is_empty() => Self::Mirror(base.trim_end_matches('/').to_owned()),
            _ => Self::GitHub { tag },
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("update", message)
}

/// The Understudy.app this executable belongs to.
pub fn installed_app() -> Result<PathBuf> {
    let executable = std::env::current_exe().and_then(fs::canonicalize)?;
    app_of(&executable).ok_or_else(|| {
        problem("This understudy is not inside Understudy.app; install it with install.sh")
    })
}

/// The app bundle `executable` is the main executable of, if any.
pub(crate) fn app_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let app = macos.parent()?.parent()?;
    (macos.ends_with("Contents/MacOS") && app.extension().is_some_and(|ext| ext == "app"))
        .then(|| app.to_owned())
}

/// The version in a release archive's name, `Understudy-X.Y.Z-macos.zip`.
fn archive_version(name: &str) -> Option<Vec<u64>> {
    name.strip_prefix("Understudy-")?
        .strip_suffix("-macos.zip")?
        .split('.')
        .map(|part| part.parse().ok())
        .collect()
}

fn version(text: &str) -> Vec<u64> {
    text.split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// A GitHub token while the repository is private: the environment, else
/// the GitHub CLI's login.
fn token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|token| !token.is_empty()))
        .or_else(|| {
            let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
            let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
            (output.status.success() && !token.is_empty()).then_some(token)
        })
}

fn curl(url: &str, output: &Path, accept: &str, token: Option<&str>) -> Result<()> {
    let mut command = Command::new("/usr/bin/curl");
    command.args(["-fsSL", "-H", &format!("Accept: {accept}"), "-o"]);
    command.arg(output);
    if let Some(token) = token {
        command.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    let status = command.arg(url).status()?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| problem(format!("Could not download {url}")))
}

/// Run a tool, failing with `why` if it does not succeed.
fn check(program: &str, args: &[&std::ffi::OsStr], why: &str) -> Result<()> {
    let output = Command::new(program).args(args).output()?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| problem(why.to_owned()))
}

/// A private scratch directory for one update.
fn scratch() -> Result<PathBuf> {
    let path = crate::config::home()?
        .join("Library/Caches/Understudy")
        .join(format!("update-{}", crate::control::token()));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;
    Ok(path)
}

/// Download the release archive and its sums into `work`; the archive's name.
fn download(source: &Source, work: &Path) -> Result<String> {
    let sums = work.join("SHA256SUMS");
    match source {
        Source::Mirror(base) => {
            curl(&format!("{base}/SHA256SUMS"), &sums, "*/*", None)?;
            let name = fs::read_to_string(&sums)?
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .map(str::to_owned)
                .ok_or_else(|| problem("SHA256SUMS lists no archive"))?;
            curl(&format!("{base}/{name}"), &work.join(&name), "*/*", None)?;
            Ok(name)
        }
        Source::GitHub { tag } => {
            let token = token();
            let api = match tag {
                Some(tag) => format!("https://api.github.com/repos/{REPO}/releases/tags/{tag}"),
                None => format!("https://api.github.com/repos/{REPO}/releases/latest"),
            };
            let release = work.join("release.json");
            curl(
                &api,
                &release,
                "application/vnd.github+json",
                token.as_deref(),
            )
            .map_err(|_| {
                problem(
                    "Could not read the release; set GITHUB_TOKEN while the repository is private",
                )
            })?;
            let release: Value = serde_json::from_slice(&fs::read(&release)?)?;
            let asset = |wanted: &dyn Fn(&str) -> bool| {
                release["assets"].as_array().and_then(|assets| {
                    assets.iter().find_map(|asset| {
                        let name = asset["name"].as_str()?;
                        let url = asset["url"].as_str()?;
                        wanted(name).then(|| (name.to_owned(), url.to_owned()))
                    })
                })
            };
            let (_, sums_url) = asset(&|name| name == "SHA256SUMS")
                .ok_or_else(|| problem("The release has no SHA256SUMS"))?;
            let (name, url) = asset(&|name| archive_version(name).is_some())
                .ok_or_else(|| problem("The release has no Understudy archive"))?;
            let octets = "application/octet-stream";
            curl(&sums_url, &sums, octets, token.as_deref())?;
            curl(&url, &work.join(&name), octets, token.as_deref())?;
            Ok(name)
        }
    }
}

/// Verify the archive against SHA256SUMS, unpack it and check the app.
fn verify(work: &Path, name: &str, allow_unnotarized: bool) -> Result<PathBuf> {
    let expected = fs::read_to_string(work.join("SHA256SUMS"))?
        .lines()
        .find_map(|line| {
            let (sum, file) = line.split_once("  ")?;
            (file == name).then(|| sum.to_owned())
        })
        .ok_or_else(|| problem(format!("SHA256SUMS does not list {name}")))?;
    let archive = work.join(name);
    if format!("{:x}", Sha256::digest(fs::read(&archive)?)) != expected {
        return Err(problem(format!("Checksum mismatch for {name}")));
    }
    let unpacked = work.join("unpacked");
    check(
        "/usr/bin/ditto",
        &[
            "-x".as_ref(),
            "-k".as_ref(),
            archive.as_os_str(),
            unpacked.as_os_str(),
        ],
        "The archive could not be unpacked",
    )?;
    let app = unpacked.join("Understudy.app");
    check(
        "/usr/bin/codesign",
        &[
            "--verify".as_ref(),
            "--strict".as_ref(),
            "--deep".as_ref(),
            app.as_os_str(),
        ],
        "The app's signature is invalid",
    )?;
    if !setup::developer_id(&app.join("Contents/MacOS/understudy")) {
        return Err(problem(
            "The app is not Understudy signed by its Developer ID",
        ));
    }
    let notarized = check(
        "/usr/sbin/spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "execute".as_ref(),
            app.as_os_str(),
        ],
        "",
    )
    .is_ok();
    if !notarized && !allow_unnotarized {
        return Err(problem(
            "The release is not notarized; --allow-unnotarized accepts it",
        ));
    }
    Ok(app)
}

/// Put `new` in place of `installed` by renames within one directory.
fn swap(new: &Path, installed: &Path) -> Result<()> {
    let parent = installed
        .parent()
        .ok_or_else(|| problem("The installed app has no parent directory"))?;
    let token = crate::control::token();
    let staged = parent.join(format!(".Understudy.app.{token}"));
    let previous = parent.join(format!(".Understudy.app.previous.{token}"));
    check(
        "/usr/bin/ditto",
        &[new.as_os_str(), staged.as_os_str()],
        "The new app could not be staged beside the installed one",
    )?;
    fs::rename(installed, &previous)?;
    if let Err(error) = fs::rename(&staged, installed) {
        // Put the working app back before reporting.
        fs::rename(&previous, installed)?;
        let _ = fs::remove_dir_all(&staged);
        return Err(error.into());
    }
    fs::remove_dir_all(&previous)?;
    Ok(())
}

/// Replace the installed app with the release from `source` when it is newer
/// (or `force`). Reports whether it changed and the versions involved.
pub fn install(source: &Source, allow_unnotarized: bool, force: bool) -> Result<Value> {
    let installed = installed_app()?;
    let work = scratch()?;
    let outcome = (|| {
        let name = download(source, &work)?;
        let available = archive_version(&name).unwrap_or_default();
        let current = version(crate::VERSION);
        if available <= current && !force {
            return Ok(json!({
                "ok": true,
                "updated": false,
                "version": crate::VERSION,
                "available": name,
            }));
        }
        let app = verify(&work, &name, allow_unnotarized)?;
        swap(&app, &installed)?;
        Ok(json!({
            "ok": true,
            "updated": true,
            "from": crate::VERSION,
            "to": name,
            "app": installed,
        }))
    })();
    let _ = fs::remove_dir_all(&work);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_name_their_version() {
        assert_eq!(
            archive_version("Understudy-0.10.2-macos.zip"),
            Some(vec![0, 10, 2])
        );
        assert_eq!(archive_version("Understudy-x-macos.zip"), None);
        assert_eq!(archive_version("SHA256SUMS"), None);
        assert!(archive_version("Understudy-0.10.0-macos.zip").unwrap() > version("0.9.9"));
    }

    #[test]
    fn only_an_app_executable_belongs_to_an_app() {
        assert_eq!(
            app_of(Path::new(
                "/Applications/Understudy.app/Contents/MacOS/understudy"
            )),
            Some(PathBuf::from("/Applications/Understudy.app"))
        );
        assert_eq!(app_of(Path::new("/usr/local/bin/understudy")), None);
        assert_eq!(app_of(Path::new("/x/Contents/MacOS/understudy")), None);
    }

    #[test]
    fn a_swap_replaces_the_app_by_rename_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("Understudy.app");
        let new = dir.path().join("new/Understudy.app");
        fs::create_dir_all(&installed).unwrap();
        fs::write(installed.join("version"), "old").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("version"), "new").unwrap();
        swap(&new, &installed).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".Understudy"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
