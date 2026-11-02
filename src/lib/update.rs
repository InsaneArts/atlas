//! `atlas update`: replace the installed Atlas.app with the latest
//! release after the same checks as install.sh — checksum, signature, the
//! Developer ID requirement and notarization — swapping by rename so a
//! running helper daemon never sees a half-written executable.
pub use crate::app_bundle::{INSTALL_COMMAND, installed_app, installed_executable};
use crate::{
    app_bundle::{BUNDLE_EXECUTABLE as EXECUTABLE, reachable_by_others, require_helper_access},
    error::{Code, Error, Result},
    files::{LockOpen, open_lock_file},
    setup,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::Command,
};

/// Where the website serves releases: one directory per version, and
/// `latest/` for the newest.
const RELEASES: &str = "https://atlas.insanearts.io/releases";

const NO_RELEASE: &str =
    "Atlas has no public release yet. Check https://atlas.insanearts.io for news.";

/// Where releases come from: a directory URL holding the archive and
/// SHA256SUMS (https, file:// for a local mirror, or http on this Mac for a
/// rehearsal).
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
    Error::new(Code::Update, message)
}

/// The version in a release archive's name, `Atlas-X.Y.Z-macos.zip`.
fn archive_version(name: &str) -> Option<Vec<u64>> {
    exact_version(name.strip_prefix("Atlas-")?.strip_suffix("-macos.zip")?)
}

/// `X.Y.Z` as numbers, only when every part is one: the versions a release
/// may have.
pub fn exact_version(text: &str) -> Option<Vec<u64>> {
    text.split('.').map(|part| part.parse().ok()).collect()
}

fn bundle_version(app: &Path) -> Option<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", "Print :CFBundleShortVersionString"])
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Refuse an app whose own version is not the one its archive is named
/// for: which release is newer is judged from that name.
fn require_named_version(app: &Path, name: &str) -> Result<()> {
    let says = bundle_version(app);
    let named = archive_version(name);
    if named.is_some() && says.as_deref().and_then(exact_version) == named {
        return Ok(());
    }

    Err(problem(format!(
        "The app in {name} says it is version {}, not the one its name gives",
        says.as_deref().unwrap_or("unknown")
    )))
}

fn version(text: &str) -> Vec<u64> {
    text.split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

const CONNECT_TIMEOUT: &str = "30";
const DOWNLOAD_TIMEOUT: &str = "900";

/// The one protocol curl may use for `url`: https, file:// for a local
/// mirror, or plain http only to this Mac (127.0.0.1 or localhost, with an
/// optional port), as a rehearsal serves releases. install.sh accepts the
/// same.
fn protocol(url: &str) -> Result<&'static str> {
    if url.starts_with("https://") {
        return Ok("=https");
    }

    if url.starts_with("file://") {
        return Ok("=file");
    }

    let loopback = url
        .strip_prefix("http://")
        .map(|rest| rest.split('/').next().unwrap_or_default())
        .is_some_and(|authority| {
            let (host, port) = authority.split_once(':').unwrap_or((authority, "80"));
            matches!(host, "127.0.0.1" | "localhost")
                && !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit())
        });
    if loopback {
        return Ok("=http");
    }

    Err(problem(format!(
        "Releases come over https, from a file:// folder, or over http from this Mac only, not {url}"
    )))
}

/// Download `url` to `output` with the one protocol [`protocol`] allows,
/// following redirects only to https.
fn curl(url: &str, output: &Path) -> Result<()> {
    let protocols = protocol(url)?;

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

fn check(command: &mut Command, why: &str) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    Err(problem(why).with_source(std::io::Error::other(format!(
        "{}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))))
}

/// Where Atlas keeps its caches: update's scratch and lock among them.
pub(crate) fn caches() -> Result<PathBuf> {
    Ok(crate::config::home()?.join("Library/Caches/Atlas"))
}

fn private_folder(path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// One update at a time on this account, held until dropped. Updates share
/// a scratch folder and sweep each other's staging copies otherwise.
#[derive(Debug)]
pub struct Exclusive {
    _lock: crate::files::LockFile,
}

/// Take the update lock, or say another update is running.
///
/// # Errors
///
/// Returns update-lock storage failures or refuses a concurrent update owner.
pub fn exclusive() -> Result<Exclusive> {
    exclusive_in(&caches()?)
}

fn exclusive_in(directory: &Path) -> Result<Exclusive> {
    use fs2::FileExt;

    private_folder(directory).map_err(|error| Error::writing(directory, error))?;

    let path = directory.join("update.lock");
    let lock =
        open_lock_file(&path, LockOpen::ReadWrite).map_err(|error| Error::writing(&path, error))?;

    lock.try_lock_exclusive().map_err(|error| {
        problem("Another atlas update is running. Wait for it to finish.").with_source(error)
    })?;

    Ok(Exclusive {
        _lock: crate::files::LockFile(lock),
    })
}

/// A private scratch directory for one update, emptied first: an update
/// stopped midway leaves nothing behind for the next one to find.
fn scratch(caches: &Path) -> Result<PathBuf> {
    let path = caches.join("update");
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }

    private_folder(&path)?;

    Ok(path)
}

/// Download only the release's sums into `work`; the archive they name, and
/// its version.
fn newest(source: &Source, work: &Path) -> Result<(String, Vec<u64>)> {
    let base = &source.base;
    let sums = work.join("SHA256SUMS");
    curl(&format!("{base}/SHA256SUMS"), &sums)
        .map_err(|error| unpublished(error, source.latest))?;

    // The name becomes a path: accept only a plain archive name.
    fs::read_to_string(&sums)?
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .filter(|name| !name.contains('/'))
        .and_then(|name| Some((name.to_owned(), archive_version(name)?)))
        .ok_or_else(|| problem("SHA256SUMS does not name an Atlas archive"))
}

/// Whether a newer release than this one is published, without installing
/// anything or touching the helper: this version, the newest, and whether
/// the newest is newer.
///
/// # Errors
///
/// Returns source, download or release-description failures. Checking does not install the
/// offered release.
pub fn newer(source: &Source) -> Result<Value> {
    newer_in(source, &caches()?)
}

fn newer_in(source: &Source, caches: &Path) -> Result<Value> {
    sweep_checks(caches);
    // A folder of its own: an update running meanwhile keeps its scratch.
    let work = caches.join(format!("check-{}", std::process::id()));
    private_folder(&work).map_err(|error| Error::writing(&work, error))?;
    let outcome = newest(source, &work).map(|(_, available)| {
        let latest: Vec<String> = available.iter().map(u64::to_string).collect();
        json!({
            "ok": true,
            "version": crate::VERSION,
            "latest": latest.join("."),
            "newer": available > version(crate::VERSION),
        })
    });

    let _ = fs::remove_dir_all(&work);
    outcome
}

/// Remove what checks that were stopped midway left in `caches`: a folder
/// named for a process that is no longer running.
fn sweep_checks(caches: &Path) {
    let Ok(entries) = fs::read_dir(caches) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("check-"))
            .and_then(|pid| pid.parse::<i32>().ok())
        else {
            continue;
        };
        let running = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok();
        if !running && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

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
        Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(&archive)
            .arg(&unpacked),
        "The archive could not be unpacked",
    )?;

    let app = unpacked.join("Atlas.app");
    check(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--deep"])
            .arg(&app),
        "The app's signature is invalid",
    )?;

    if !setup::developer_id(&app.join(EXECUTABLE)) {
        return Err(problem("The app is not Atlas signed by its Developer ID"));
    }

    require_named_version(&app, name)?;

    let notarized = check(
        Command::new("/usr/sbin/spctl")
            .args(["--assess", "--type", "execute"])
            .arg(&app),
        "",
    );
    if let Err(cause) = notarized
        && !allow_unnotarized
    {
        return Err(
            problem("The release is not notarized; --allow-unnotarized accepts it")
                .with_source(cause),
        );
    }

    Ok(app)
}

fn exchange(first: &Path, second: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let path = |path: &Path| {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| problem("Invalid app path"))
    };
    let (first, second) = (path(first)?, path(second)?);

    crate::fs_move::rename_path(&first, &second, crate::fs_move::Replace::Swap)?;
    Ok(())
}

fn record_path() -> Result<PathBuf> {
    Ok(crate::config::home()?.join("Library/Application Support/Atlas/update.json"))
}

/// An unfinished update as its record describes it. Any JSON object is a
/// record, and a field missing or of another type is unknown.
#[derive(Debug)]
struct Handover {
    previous: Option<PathBuf>,
    installed: Option<PathBuf>,
    /// The new executable's hash.
    executable: Option<String>,
    /// The record as written, for the report.
    written: Value,
}

impl From<Value> for Handover {
    fn from(written: Value) -> Self {
        let text = |key: &str| written[key].as_str().map(str::to_owned);
        Self {
            previous: text("previous").map(PathBuf::from),
            installed: text("installed").map(PathBuf::from),
            executable: text("executable"),
            written,
        }
    }
}

/// The record at `path`, if any. One that cannot be read is an error that
/// asks to inspect it, never a reason to skip the handover it describes.
fn read_record(path: &Path) -> Result<Option<Handover>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let unreadable = || {
        problem(format!(
            "The record of an unfinished update, {}, cannot be read. Inspect it (it names the \
             previous Atlas.app, kept beside the installed one), then remove it and run atlas \
             update again.",
            crate::config::tilde(path)
        ))
    };

    let written: Value =
        serde_json::from_slice(&bytes).map_err(|error| unreadable().with_source(error))?;
    if !written.is_object() {
        return Err(unreadable());
    }

    Ok(Some(written.into()))
}

/// The helper is on the new app: drop the previous bundle and the record.
/// A previous bundle that cannot be removed is reported, not an error.
///
/// # Errors
///
/// Returns update-handover record or completion failures. An unfinished handover is not
/// reported as a completed update.
pub fn finished() -> Result<Value> {
    finish_record(&record_path()?)
}

fn finish_record(path: &Path) -> Result<Value> {
    let Some(record) = read_record(path)? else {
        return Ok(json!({"previous": null}));
    };

    let installed = record.installed.as_deref();
    // Only a staging copy beside the installed app is ever removed.
    let removed = record.previous.as_ref().is_some_and(|previous| {
        beside(previous, installed, update_copy) && fs::remove_dir_all(previous).is_ok()
    });

    fs::remove_file(path)?;
    Ok(json!({"previous": record.previous, "removed": removed}))
}

/// What every staging copy beside the installed app is named from.
const STAGED: &str = ".Atlas.app.";

/// The reserved prefix of the copies `atlas update` stages, followed by a
/// random token. Other folders beside the app (a person's own
/// `.Atlas.app.previous`, say) are never touched.
const UPDATE: &str = "update-";

/// The reserved prefixes of the copies install.sh stages, each followed by
/// its process ID.
const INSTALL: [&str; 2] = ["install-", "install-previous-"];

/// Whether `name` is a copy `atlas update` stages: `.Atlas.app.update-` and
/// a token.
fn update_copy(name: &str) -> bool {
    name.strip_prefix(STAGED)
        .and_then(|rest| rest.strip_prefix(UPDATE))
        .is_some_and(|token| crate::files::Token::parse(token).is_some())
}

/// Whether `name` is a copy install.sh stages: `.Atlas.app.install-` or
/// `.Atlas.app.install-previous-`, and a process ID.
fn install_copy(name: &str) -> bool {
    let pid = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());

    name.strip_prefix(STAGED).is_some_and(|rest| {
        INSTALL
            .iter()
            .any(|prefix| rest.strip_prefix(prefix).is_some_and(pid))
    })
}

/// Whether `path` is named as `named` accepts and sits in the folder that
/// holds `installed`.
fn beside(path: &Path, installed: Option<&Path>, named: fn(&str) -> bool) -> bool {
    let named = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(named);

    named
        && path.is_absolute()
        && installed
            .is_some_and(|installed| installed.is_absolute() && installed.parent() == path.parent())
}

fn copies(installed: &Path, named: fn(&str) -> bool) -> Vec<PathBuf> {
    let Some(entries) = installed
        .parent()
        .and_then(|parent| fs::read_dir(parent).ok())
    else {
        return Vec::new();
    };

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| beside(path, Some(installed), named))
        .collect()
}

/// Every copy Atlas staged beside `installed`, by `atlas update` or by
/// install.sh: what uninstall clears.
pub(crate) fn atlas_copies(installed: &Path) -> Vec<PathBuf> {
    copies(installed, |name| update_copy(name) || install_copy(name))
}

fn stage(new: &Path, staged: &Path) -> Result<()> {
    check(
        Command::new("/usr/bin/ditto").arg(new).arg(staged),
        "The new app could not be staged beside the installed one",
    )
}

/// Put `new` in place of `installed` with one atomic exchange, keeping
/// the previous app beside it (and recording where) until the helper's
/// handover succeeds.
async fn swap_using(
    new: &Path,
    installed: &Path,
    name: &str,
    record: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
    stage: impl FnOnce(&Path, &Path) -> Result<()>,
) -> Result<()> {
    let parent = installed
        .parent()
        .ok_or_else(|| problem("The installed app has no parent directory"))?;
    let executable = crate::digest::file_sha256(&new.join(EXECUTABLE))
        .ok_or_else(|| problem("The new app's executable could not be read"))?;

    // A staging copy an earlier update left when it was stopped (no record
    // names it, or this update would be finishing it) goes first.
    for path in copies(installed, update_copy) {
        let _ = fs::remove_dir_all(&path);
    }

    let staged = parent.join(format!("{STAGED}{UPDATE}{}", crate::files::token()));
    stage(new, &staged)?;

    // Staging can take time. Check the staged executable only once that
    // work is done, while the caller still owns controller exclusion.
    let program = staged.join(EXECUTABLE);
    let prepared = preflight(&program)
        .await
        .and_then(|()| require_helper_access(&program));
    if let Err(error) = prepared {
        // No handover was recorded or bundle exchanged; a leftover staging
        // copy is best-effort housekeeping, not acknowledged helper work.
        let _ = fs::remove_dir_all(&staged);
        return Err(error);
    }

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
        .and_then(|bytes| crate::files::save_private(record, &bytes));
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
async fn resume(
    record: &Handover,
    installed: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let previous = resume_source(record, installed);
    let next = previous.as_deref().unwrap_or(installed);
    let program = next.join(EXECUTABLE);
    preflight(&program).await?;
    // Refusal preserves both the recorded handover and candidate for recovery.
    require_helper_access(&program)?;
    if let Some(previous) = previous {
        exchange(&previous, installed)?;
    }
    Ok(())
}

/// The staged new app when the recorded exchange has not happened yet.
fn resume_source(record: &Handover, installed: &Path) -> Option<PathBuf> {
    let expected = record.executable.as_deref()?;
    let previous = record
        .previous
        .as_deref()
        .filter(|previous| beside(previous, Some(installed), update_copy))?;

    let sha256 = |app: &Path| crate::digest::file_sha256(&app.join(EXECUTABLE));
    (sha256(installed).as_deref() != Some(expected)
        && sha256(previous).as_deref() == Some(expected))
    .then(|| previous.to_owned())
}

/// Replace the installed app with the release from `source` when it is newer
/// (or `force`). Reports whether it changed and the versions involved. The
/// caller holds the update lock throughout.
///
/// # Errors
///
/// Returns ownership, source, archive, signature, replacement or handover failures. Unknown
/// effects retain the update recovery record; a verified archive alone does not prove the
/// running service was replaced.
pub async fn install(
    source: &Source,
    allow_unnotarized: bool,
    force: bool,
    _exclusive: &Exclusive,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<Value> {
    let installed = installed_app()?;
    install_in(
        source,
        allow_unnotarized,
        force,
        &installed,
        &caches()?,
        &record_path()?,
        preflight,
    )
    .await
}

/// Install using the given app, cache and handover paths; the caller holds
/// the update lock, as for [`install`].
async fn install_in(
    source: &Source,
    allow_unnotarized: bool,
    force: bool,
    installed: &Path,
    caches: &Path,
    record: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<Value> {
    // A swap whose handover did not finish is resumed before anything else.
    if let Some(record) = read_record(record)? {
        resume(&record, installed, preflight).await?;
        return Ok(json!({"ok": true, "updated": true, "resumed": true, "record": record.written}));
    }

    if !reachable_by_others(&installed.join(EXECUTABLE)) {
        return Err(problem(
            "Atlas.app must be where the helper account can run it: install it in /Applications",
        ));
    }

    let work = scratch(caches)?;
    let outcome = async {
        let (name, available) = newest(source, &work)?;
        if available <= version(crate::VERSION) && !force {
            return Ok(json!({
                "ok": true,
                "updated": false,
                "version": crate::VERSION,
                "available": name,
            }));
        }

        curl(&format!("{}/{name}", source.base), &work.join(&name))?;
        let app = verify(&work, &name, allow_unnotarized)?;
        swap_using(&app, installed, &name, record, preflight, stage).await?;

        Ok(json!({
            "ok": true,
            "updated": true,
            "from": crate::VERSION,
            "to": name,
            "app": installed,
        }))
    }
    .await;

    let _ = fs::remove_dir_all(&work);
    outcome
}

#[cfg(test)]
mod tests;
