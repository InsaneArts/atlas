//! `atlas update`: replace the installed Atlas.app with the latest
//! release after the same checks as install.sh — checksum, signature, the
//! Developer ID requirement and notarization — swapping by rename so a
//! running helper daemon never sees a half-written executable.
use crate::{
    error::{Error, Result},
    setup,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
};

/// The command that installs Atlas, served by its website.
pub const INSTALL_COMMAND: &str = "curl -fsSL https://atlas.insanearts.io/install.sh | sh";

/// Where the website serves releases: one directory per version, and
/// `latest/` for the newest.
const RELEASES: &str = "https://atlas.insanearts.io/releases";

/// What a missing newest release means.
const NO_RELEASE: &str =
    "Atlas has no public release yet. Check https://atlas.insanearts.io for news.";

/// Where releases come from: a directory URL holding the archive and
/// SHA256SUMS (https, or file:// for a local mirror).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    base: String,
    /// The website's newest release, which may not be published yet.
    latest: bool,
}

impl Source {
    /// The mirror in `ATLAS_DOWNLOAD_BASE`, else the website's release
    /// `tag`, or its newest.
    pub fn from_env(tag: Option<String>) -> Self {
        match std::env::var("ATLAS_DOWNLOAD_BASE") {
            Ok(base) if !base.is_empty() => Self {
                base: base.trim_end_matches('/').to_owned(),
                latest: false,
            },
            _ => Self::website(tag.as_deref()),
        }
    }

    fn website(tag: Option<&str>) -> Self {
        Self {
            base: format!("{RELEASES}/{}", tag.unwrap_or("latest")),
            latest: tag.is_none(),
        }
    }
}

fn problem(message: impl Into<String>) -> Error {
    Error::new("update", message)
}

/// The Atlas.app this executable belongs to.
pub fn installed_app() -> Result<PathBuf> {
    let executable = std::env::current_exe().and_then(fs::canonicalize)?;
    app_of(&executable).ok_or_else(|| {
        problem(format!(
            "This atlas is not inside Atlas.app; install it with: {INSTALL_COMMAND}"
        ))
    })
}

/// The `atlas` executable inside the installed Atlas.app.
pub fn installed_executable() -> Result<PathBuf> {
    Ok(installed_app()?.join(EXECUTABLE))
}

/// Where the main executable lives inside Atlas.app.
const EXECUTABLE: &str = "Contents/MacOS/atlas";

/// The app bundle `executable` is the main executable of, if any.
pub(crate) fn app_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let app = macos.parent()?.parent()?;
    (macos.ends_with("Contents/MacOS") && app.extension().is_some_and(|ext| ext == "app"))
        .then(|| app.to_owned())
}

/// The version in a release archive's name, `Atlas-X.Y.Z-macos.zip`.
fn archive_version(name: &str) -> Option<Vec<u64>> {
    name.strip_prefix("Atlas-")?
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

/// How long a download may take to connect, and in all.
const CONNECT_TIMEOUT: &str = "30";
const DOWNLOAD_TIMEOUT: &str = "900";

/// Download `url` to `output` over HTTPS only (redirects too), or from a
/// local file:// mirror.
fn curl(url: &str, output: &Path) -> Result<()> {
    let protocols = if url.starts_with("file://") {
        "=file"
    } else {
        "=https"
    };

    let finished = Command::new("/usr/bin/curl")
        .args(["-fsSL", "--proto", protocols, "--proto-redir", "=https"])
        .args(["--connect-timeout", CONNECT_TIMEOUT])
        .args(["--max-time", DOWNLOAD_TIMEOUT])
        .arg("-o")
        .arg(output)
        .arg(url)
        .output()?;
    if finished.status.success() {
        return Ok(());
    }

    let said = String::from_utf8_lossy(&finished.stderr);
    Err(problem(match said.trim() {
        "" => format!("Could not download {url}"),
        said => format!("Could not download {url}: {said}"),
    }))
}

/// A newest release the website does not have yet is not published, not a
/// failed download.
fn unpublished(error: Error, latest: bool) -> Error {
    if latest && error.error.contains("returned error: 404") {
        problem(NO_RELEASE)
    } else {
        error
    }
}

/// Run a tool, failing with `why` if it does not succeed.
fn check(program: &str, args: &[&std::ffi::OsStr], why: &str) -> Result<()> {
    let output = Command::new(program).args(args).output()?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| problem(why))
}

/// A private scratch directory for one update, emptied first: an update
/// stopped midway leaves nothing behind for the next one to find.
fn scratch() -> Result<PathBuf> {
    let path = crate::config::home()?.join("Library/Caches/Atlas/update");
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;

    Ok(path)
}

/// Download the release archive and its sums into `work`; the archive's name.
fn download(source: &Source, work: &Path) -> Result<String> {
    let base = &source.base;
    let sums = work.join("SHA256SUMS");
    curl(&format!("{base}/SHA256SUMS"), &sums)
        .map_err(|error| unpublished(error, source.latest))?;

    // The name becomes a path: accept only a plain archive name.
    let name = fs::read_to_string(&sums)?
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .filter(|name| !name.contains('/') && archive_version(name).is_some())
        .map(str::to_owned)
        .ok_or_else(|| problem("SHA256SUMS does not name an Atlas archive"))?;
    curl(&format!("{base}/{name}"), &work.join(&name))?;

    Ok(name)
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
    let actual = crate::digest::file_sha256(&archive)
        .ok_or_else(|| problem(format!("{name} could not be read")))?;
    if actual != expected {
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

    let app = unpacked.join("Atlas.app");
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

    if !setup::developer_id(&app.join(EXECUTABLE)) {
        return Err(problem("The app is not Atlas signed by its Developer ID"));
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

/// Exchange two paths in one atomic step (both must exist).
#[allow(
    unsafe_code,
    reason = "FFI: renamex_np(RENAME_SWAP) has no safe binding"
)]
fn exchange(first: &Path, second: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let path = |path: &Path| {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| problem("Invalid app path"))
    };
    let (first, second) = (path(first)?, path(second)?);

    // SAFETY: two NUL-terminated paths; RENAME_SWAP exchanges them or fails
    // leaving both in place.
    if unsafe { libc::renamex_np(first.as_ptr(), second.as_ptr(), libc::RENAME_SWAP) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    Ok(())
}

/// Where an unfinished update is recorded until the helper is on it.
fn record_path() -> Result<PathBuf> {
    Ok(crate::config::home()?.join("Library/Application Support/Atlas/update.json"))
}

/// The update whose helper handover has not completed, if any.
pub fn unfinished() -> Result<Option<Value>> {
    read_record(&record_path()?)
}

/// The record at `path`, if any. One that cannot be read is an error that
/// asks to inspect it, never a reason to skip the handover it describes.
fn read_record(path: &Path) -> Result<Option<Value>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .ok()
            .filter(Value::is_object)
            .map(Some)
            .ok_or_else(|| {
                problem(format!(
                    "The record of an unfinished update, {}, cannot be read. Inspect it (it \
                     names the previous Atlas.app, kept beside the installed one), then remove \
                     it and run atlas update again.",
                    crate::ui::text::tilde(path)
                ))
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// The helper is on the new app: drop the previous bundle and the record.
/// A previous bundle that cannot be removed is reported, not an error.
pub fn finished() -> Result<Value> {
    finish_record(&record_path()?)
}

fn finish_record(path: &Path) -> Result<Value> {
    let Some(record) = read_record(path)? else {
        return Ok(json!({"previous": null}));
    };

    let previous = record["previous"].as_str().map(PathBuf::from);
    let installed = record["installed"].as_str().map(Path::new);
    // Only a staging copy beside the installed app is ever removed.
    let removed = previous.as_ref().is_some_and(|previous| {
        staged_beside(previous, installed) && fs::remove_dir_all(previous).is_ok()
    });

    fs::remove_file(path)?;
    Ok(json!({"previous": previous, "removed": removed}))
}

/// Where the next app is staged, beside the installed one.
const STAGED: &str = ".Atlas.app.";

/// Whether `previous` is a staging copy (`.Atlas.app.*`) in the folder
/// that holds `installed`.
fn staged_beside(previous: &Path, installed: Option<&Path>) -> bool {
    let named = previous
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(STAGED))
        .is_some_and(|rest| !rest.is_empty());

    named
        && previous.is_absolute()
        && installed.is_some_and(|installed| {
            installed.is_absolute() && installed.parent() == previous.parent()
        })
}

/// Put `new` in place of `installed` with one atomic exchange, keeping
/// the previous app beside it (and recording where) until the helper's
/// handover succeeds.
fn swap(new: &Path, installed: &Path, name: &str) -> Result<()> {
    swap_recording(new, installed, name, &record_path()?)
}

fn swap_recording(new: &Path, installed: &Path, name: &str, record: &Path) -> Result<()> {
    let parent = installed
        .parent()
        .ok_or_else(|| problem("The installed app has no parent directory"))?;
    let executable = crate::digest::file_sha256(&new.join(EXECUTABLE))
        .ok_or_else(|| problem("The new app's executable could not be read"))?;

    // A staging copy an earlier update left when it was stopped (no record
    // names it, or this update would be finishing it) goes first.
    for entry in fs::read_dir(parent)?.flatten() {
        let path = entry.path();
        if staged_beside(&path, Some(installed)) {
            let _ = fs::remove_dir_all(&path);
        }
    }

    let staged = parent.join(format!("{STAGED}{}", crate::control::token()));
    check(
        "/usr/bin/ditto",
        &[new.as_os_str(), staged.as_os_str()],
        "The new app could not be staged beside the installed one",
    )?;

    // Recorded, in one rename, before the exchange: whatever happens next,
    // the next update finds this handover and finishes it first. After the
    // exchange `staged` holds the previous app. The new executable's hash
    // tells that next update whether the exchange happened.
    let plan = json!({
        "previous": staged,
        "installed": installed,
        "to": name,
        "executable": executable,
    });
    let recorded = serde_json::to_vec_pretty(&plan)
        .map_err(Error::from)
        .and_then(|bytes| crate::config::save_private(record, &bytes));
    if let Err(error) = recorded {
        let _ = fs::remove_dir_all(&staged);
        return Err(error);
    }

    if let Err(error) = exchange(&staged, installed) {
        let _ = fs::remove_dir_all(&staged);
        let _ = fs::remove_file(record);
        return Err(error);
    }

    Ok(())
}

/// The exchange a recorded update may have been stopped before: when the
/// installed app is not the recorded new one but the staging copy beside it
/// is, exchange them now, so finishing never discards the new app.
fn resume(record: &Value, installed: &Path) -> Result<()> {
    let Some(expected) = record["executable"].as_str() else {
        return Ok(());
    };

    let previous = record["previous"].as_str().map(Path::new);
    let Some(previous) = previous.filter(|previous| staged_beside(previous, Some(installed)))
    else {
        return Ok(());
    };

    let sha256 = |app: &Path| crate::digest::file_sha256(&app.join(EXECUTABLE));
    if sha256(installed).as_deref() != Some(expected)
        && sha256(previous).as_deref() == Some(expected)
    {
        exchange(previous, installed)?;
    }

    Ok(())
}

/// Whether other accounts (the helper) can reach and run `executable`:
/// every directory above it searchable and the file readable and executable
/// by others. ACLs are not counted, so this errs on the side of refusing.
pub fn reachable_by_others(executable: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let mode = |path: &Path| fs::metadata(path).map(|metadata| metadata.permissions().mode());
    mode(executable).is_ok_and(|mode| mode & 0o005 == 0o005)
        && executable
            .ancestors()
            .skip(1)
            .all(|directory| mode(directory).is_ok_and(|mode| mode & 0o001 != 0))
}

/// Replace the installed app with the release from `source` when it is newer
/// (or `force`). Reports whether it changed and the versions involved.
pub fn install(source: &Source, allow_unnotarized: bool, force: bool) -> Result<Value> {
    let installed = installed_app()?;

    // A swap whose handover did not finish is resumed before anything else.
    if let Some(record) = unfinished()? {
        resume(&record, &installed)?;
        return Ok(json!({"ok": true, "updated": true, "resumed": true, "record": record}));
    }

    if !reachable_by_others(&installed.join(EXECUTABLE)) {
        return Err(problem(
            "Atlas.app must be where the helper account can run it: install it in /Applications",
        ));
    }

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
        swap(&app, &installed, &name)?;

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
            archive_version("Atlas-0.10.2-macos.zip"),
            Some(vec![0, 10, 2])
        );
        assert_eq!(archive_version("Atlas-x-macos.zip"), None);
        assert_eq!(archive_version("SHA256SUMS"), None);
        assert!(archive_version("Atlas-0.10.0-macos.zip").unwrap() > version("0.9.9"));
    }

    #[test]
    fn releases_come_from_the_website() {
        assert_eq!(
            Source::website(None),
            Source {
                base: "https://atlas.insanearts.io/releases/latest".into(),
                latest: true,
            }
        );
        assert_eq!(
            Source::website(Some("0.2.0")),
            Source {
                base: "https://atlas.insanearts.io/releases/0.2.0".into(),
                latest: false,
            }
        );
        assert!(INSTALL_COMMAND.contains("https://atlas.insanearts.io/install.sh"));
    }

    #[test]
    fn a_missing_newest_release_is_not_yet_published() {
        // curl says (22) over HTTP/1.1 and (56) over HTTP/2; the words are the same.
        for code in ["22", "56"] {
            let missing = || {
                problem(format!(
                    "Could not download x: curl: ({code}) The requested URL returned error: 404"
                ))
            };
            assert_eq!(unpublished(missing(), true).error, NO_RELEASE);
            assert_ne!(unpublished(missing(), false).error, NO_RELEASE);
        }

        let offline = problem("Could not download x: curl: (6) Could not resolve host");
        assert_ne!(unpublished(offline, true).error, NO_RELEASE);
    }

    #[test]
    fn only_an_app_executable_belongs_to_an_app() {
        assert_eq!(
            app_of(Path::new("/Applications/Atlas.app/Contents/MacOS/atlas")),
            Some(PathBuf::from("/Applications/Atlas.app"))
        );
        assert_eq!(app_of(Path::new("/usr/local/bin/atlas")), None);
        assert_eq!(app_of(Path::new("/x/Contents/MacOS/atlas")), None);
    }

    #[test]
    fn an_exchange_swaps_both_apps_in_one_step() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("Atlas.app");
        let new = dir.path().join(".Atlas.app.new");

        fs::create_dir_all(&installed).unwrap();
        fs::write(installed.join("version"), "old").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("version"), "new").unwrap();

        exchange(&new, &installed).unwrap();

        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(new.join("version")).unwrap(),
            "old",
            "kept"
        );

        assert!(exchange(&dir.path().join("missing"), &installed).is_err());
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
    }

    #[test]
    fn a_swap_is_recorded_first_and_finishing_removes_only_the_staged_copy() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let installed = root.join("Applications/Atlas.app");
        let new = root.join("unpacked/Atlas.app");
        for (app, version) in [(&installed, "old"), (&new, "new")] {
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::write(app.join("version"), version).unwrap();
            fs::write(app.join(EXECUTABLE), version).unwrap();
        }

        // A staging copy a stopped update left behind is swept first.
        let stale = root.join("Applications/.Atlas.app.stale");
        fs::create_dir_all(&stale).unwrap();
        let record = root.join("Support/update.json");
        swap_recording(&new, &installed, "Atlas-9.9.9-macos.zip", &record).unwrap();
        assert!(!stale.exists());
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );

        let plan = read_record(&record).unwrap().unwrap();
        let previous = PathBuf::from(plan["previous"].as_str().unwrap());
        assert_eq!(previous.parent(), installed.parent());
        assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "old");
        assert_eq!(
            plan["executable"],
            crate::digest::file_sha256(&installed.join(EXECUTABLE)).unwrap()
        );

        // The exchange happened: resuming changes nothing.
        resume(&plan, &installed).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );

        // Stopped between the record and the exchange: resuming exchanges.
        exchange(&previous, &installed).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "old"
        );

        resume(&plan, &installed).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );
        assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "old");

        let report = finish_record(&record).unwrap();
        assert_eq!(report["removed"], true);
        assert!(!previous.exists() && !record.exists());
        assert_eq!(finish_record(&record).unwrap()["previous"], Value::Null);

        // A swap that cannot exchange leaves no record and no staged copy.
        let missing = root.join("Elsewhere/Atlas.app");
        fs::create_dir_all(missing.parent().unwrap()).unwrap();
        assert!(swap_recording(&new, &missing, "x", &record).is_err());
        assert!(!record.exists());
        assert_eq!(fs::read_dir(missing.parent().unwrap()).unwrap().count(), 0);
    }

    #[test]
    fn finishing_never_removes_what_is_not_a_staged_copy_beside_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let installed = root.join("Applications/Atlas.app");
        let record = root.join("update.json");

        for previous in [
            root.join("Documents"),
            root.join("Applications/Other.app"),
            root.join("Elsewhere/.Atlas.app.1"),
        ] {
            fs::create_dir_all(&previous).unwrap();
            let plan = json!({"previous": previous, "installed": installed, "to": "x"});
            fs::write(&record, plan.to_string()).unwrap();
            assert_eq!(finish_record(&record).unwrap()["removed"], false);
            assert!(previous.exists(), "{}", previous.display());
            assert!(!record.exists());
        }

        let staged = root.join("Applications/.Atlas.app.1");
        assert!(staged_beside(&staged, Some(&installed)));
        assert!(!staged_beside(&staged, None));
        assert!(!staged_beside(
            &root.join("Applications/.Atlas.app."),
            Some(&installed)
        ));
    }

    #[test]
    fn an_unreadable_record_asks_to_be_inspected() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("update.json");
        assert!(read_record(&record).unwrap().is_none());

        for corrupt in ["{\"previous\": ", "[]", ""] {
            fs::write(&record, corrupt).unwrap();
            let error = read_record(&record).unwrap_err();
            assert_eq!(error.code, "update");
            assert!(error.error.contains("Inspect it"), "{}", error.error);
            assert!(finish_record(&record).is_err());
            assert!(record.exists(), "an unreadable record is kept");
        }
    }

    #[test]
    fn archives_must_match_their_listed_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let name = "Atlas-1.0.0-macos.zip";
        fs::write(dir.path().join(name), b"archive").unwrap();
        fs::write(dir.path().join("SHA256SUMS"), "00  Other.zip\n").unwrap();
        let unlisted = verify(dir.path(), name, false).unwrap_err();
        assert!(
            unlisted.error.contains("does not list"),
            "{}",
            unlisted.error
        );

        fs::write(
            dir.path().join("SHA256SUMS"),
            format!("{}  {name}\n", "0".repeat(64)),
        )
        .unwrap();
        let mismatch = verify(dir.path(), name, false).unwrap_err();
        assert!(
            mismatch.error.contains("Checksum mismatch"),
            "{}",
            mismatch.error
        );

        let sum = crate::digest::file_sha256(&dir.path().join(name)).unwrap();
        fs::write(dir.path().join("SHA256SUMS"), format!("{sum}  {name}\n")).unwrap();
        // The right bytes get past the checksum to unpacking, which fails here.
        let unpacked = verify(dir.path(), name, false).unwrap_err();
        assert!(unpacked.error.contains("unpacked"), "{}", unpacked.error);
    }

    #[test]
    fn downloads_report_why_they_failed() {
        let dir = tempfile::tempdir().unwrap();
        let missing = format!("file://{}/missing", dir.path().display());
        let error = curl(&missing, &dir.path().join("out")).unwrap_err();
        assert!(error.error.starts_with("Could not download"));
        assert!(error.error.contains("curl"), "{}", error.error);

        let plain = curl("http://127.0.0.1:9/x", &dir.path().join("out")).unwrap_err();
        assert!(
            plain.error.contains("Could not download"),
            "{}",
            plain.error
        );

        let source = dir.path().join("source");
        fs::write(&source, b"bytes").unwrap();
        let local = format!("file://{}", source.display());
        curl(&local, &dir.path().join("copy")).unwrap();
        assert_eq!(fs::read(dir.path().join("copy")).unwrap(), b"bytes");
    }

    #[test]
    fn only_paths_others_can_reach_are_runnable_by_the_helper() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("home");
        let file = private.join("Applications/atlas");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(
            !reachable_by_others(&file),
            "a private home blocks the helper"
        );
        assert!(reachable_by_others(Path::new("/bin/ls")));
    }
}
