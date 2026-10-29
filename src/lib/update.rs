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
    exact_version(name.strip_prefix("Atlas-")?.strip_suffix("-macos.zip")?)
}

/// `X.Y.Z` as numbers, only when every part is one.
fn exact_version(text: &str) -> Option<Vec<u64>> {
    text.split('.').map(|part| part.parse().ok()).collect()
}

/// The version an app says it is in its Info.plist.
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

/// How long a download may take to connect, and in all.
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

/// Run a tool, failing with `why` if it does not succeed.
fn check(program: &str, args: &[&std::ffi::OsStr], why: &str) -> Result<()> {
    let output = Command::new(program).args(args).output()?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| problem(why))
}

/// Where Atlas keeps its caches: update's scratch and lock among them.
fn caches() -> Result<PathBuf> {
    Ok(crate::config::home()?.join("Library/Caches/Atlas"))
}

/// One update at a time on this account, held until dropped. Updates share
/// a scratch folder and sweep each other's staging copies otherwise.
#[derive(Debug)]
pub struct Exclusive {
    _lock: fs::File,
}

/// Take the update lock, or say another update is running.
pub fn exclusive() -> Result<Exclusive> {
    exclusive_in(&caches()?)
}

fn exclusive_in(directory: &Path) -> Result<Exclusive> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|error| Error::writing(directory, error))?;

    let path = directory.join("update.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|error| Error::writing(&path, error))?;

    lock.try_lock_exclusive()
        .map_err(|_| problem("Another atlas update is running. Wait for it to finish."))?;

    Ok(Exclusive { _lock: lock })
}

/// A private scratch directory for one update, emptied first: an update
/// stopped midway leaves nothing behind for the next one to find.
fn scratch(caches: &Path) -> Result<PathBuf> {
    let path = caches.join("update");
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&path)?;

    Ok(path)
}

/// Download only the release's sums into `work`; the archive they name.
fn newest(source: &Source, work: &Path) -> Result<String> {
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

    Ok(name)
}

/// Whether a newer release than this one is published, without installing
/// anything or touching the helper: this version, the newest, and whether
/// the newest is newer.
pub fn newer(source: &Source) -> Result<Value> {
    newer_in(source, &caches()?)
}

/// [`newer`], reading the sums in a folder of its own inside `caches`.
fn newer_in(source: &Source, caches: &Path) -> Result<Value> {
    sweep_checks(caches);
    // A folder of its own: an update running meanwhile keeps its scratch.
    let work = caches.join(format!("check-{}", std::process::id()));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&work)
        .map_err(|error| Error::writing(&work, error))?;
    let outcome = newest(source, &work).map(|name| {
        let available = archive_version(&name).unwrap_or_default();
        json!({
            "ok": true,
            "version": crate::VERSION,
            "latest": shown_version(&available),
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

/// A version's parts as people write it, such as "0.2.0".
fn shown_version(parts: &[u64]) -> String {
    parts
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(".")
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

    require_named_version(&app, name)?;

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

/// What every staging copy beside the installed app is named from.
const STAGED: &str = ".Atlas.app.";

/// The reserved prefix of the copies `atlas update` stages, followed by a
/// random token. Other folders beside the app (a person's own
/// `.Atlas.app.previous`, say) are never touched.
const UPDATE: &str = "update-";

/// The reserved prefixes of the copies install.sh stages, each followed by
/// its process ID.
const INSTALL: [&str; 2] = ["install-", "install-previous-"];

/// Whether `text` is exactly a token from [`crate::files::token`].
fn token_shaped(text: &str) -> bool {
    crate::files::Token::parse(text).is_some()
}

/// Whether `name` is a copy `atlas update` stages: `.Atlas.app.update-` and
/// a token, or (from Atlas before these names) `.Atlas.app.` and a token.
fn update_copy(name: &str) -> bool {
    name.strip_prefix(STAGED).is_some_and(|rest| {
        rest.strip_prefix(UPDATE).is_some_and(token_shaped) || token_shaped(rest)
    })
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

/// Whether `previous` is a copy `atlas update` staged in the folder that
/// holds `installed`.
fn staged_beside(previous: &Path, installed: Option<&Path>) -> bool {
    beside(previous, installed, update_copy)
}

/// The copies beside `installed` whose names `named` accepts.
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

/// The copies `atlas update` staged beside `installed`.
fn staged_copies(installed: &Path) -> Vec<PathBuf> {
    copies(installed, update_copy)
}

/// Every copy Atlas staged beside `installed`, by `atlas update` or by
/// install.sh: what uninstall clears.
pub(crate) fn atlas_copies(installed: &Path) -> Vec<PathBuf> {
    copies(installed, |name| update_copy(name) || install_copy(name))
}

/// Put `new` in place of `installed` with one atomic exchange, keeping
/// the previous app beside it (and recording where) until the helper's
/// handover succeeds.
async fn swap(
    new: &Path,
    installed: &Path,
    name: &str,
    record: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<()> {
    preflight(&new.join(EXECUTABLE)).await?;
    swap_recording(new, installed, name, record)
}

fn swap_recording(new: &Path, installed: &Path, name: &str, record: &Path) -> Result<()> {
    let parent = installed
        .parent()
        .ok_or_else(|| problem("The installed app has no parent directory"))?;
    let executable = crate::digest::file_sha256(&new.join(EXECUTABLE))
        .ok_or_else(|| problem("The new app's executable could not be read"))?;

    // A staging copy an earlier update left when it was stopped (no record
    // names it, or this update would be finishing it) goes first.
    for path in staged_copies(installed) {
        let _ = fs::remove_dir_all(&path);
    }

    let staged = parent.join(format!("{STAGED}{UPDATE}{}", crate::files::token()));
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
async fn resume(
    record: &Value,
    installed: &Path,
    preflight: impl AsyncFnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let previous = resume_source(record, installed);
    let next = previous.as_deref().unwrap_or(installed);
    preflight(&next.join(EXECUTABLE)).await?;
    if let Some(previous) = previous {
        exchange(&previous, installed)?;
    }
    Ok(())
}

/// The staged new app when the recorded exchange has not happened yet.
fn resume_source(record: &Value, installed: &Path) -> Option<PathBuf> {
    let expected = record["executable"].as_str()?;

    let previous = record["previous"].as_str().map(Path::new);
    let previous = previous.filter(|previous| staged_beside(previous, Some(installed)))?;

    let sha256 = |app: &Path| crate::digest::file_sha256(&app.join(EXECUTABLE));
    (sha256(installed).as_deref() != Some(expected)
        && sha256(previous).as_deref() == Some(expected))
    .then(|| previous.to_owned())
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
/// (or `force`). Reports whether it changed and the versions involved. The
/// caller holds the update lock throughout.
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
        return Ok(json!({"ok": true, "updated": true, "resumed": true, "record": record}));
    }

    if !reachable_by_others(&installed.join(EXECUTABLE)) {
        return Err(problem(
            "Atlas.app must be where the helper account can run it: install it in /Applications",
        ));
    }

    let work = scratch(caches)?;
    let outcome = async {
        let name = newest(source, &work)?;
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

        curl(&format!("{}/{name}", source.base), &work.join(&name))?;
        let app = verify(&work, &name, allow_unnotarized)?;
        swap(&app, installed, &name, record, preflight).await?;

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
mod tests {
    use super::*;

    /// An owned, reachable stand-in bundle and private update paths. No test
    /// runs the stand-in executable or resolves the account's real paths.
    fn installation(directory: &Path) -> (PathBuf, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let installed = directory.join("Applications/Atlas.app");
        let executable = installed.join(EXECUTABLE);
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"installed app stays").unwrap();
        for path in executable
            .ancestors()
            .take_while(|path| path.starts_with(directory))
        {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(reachable_by_others(&executable));
        (
            installed,
            directory.join("cache"),
            directory.join("update.json"),
        )
    }

    #[tokio::test]
    async fn unchanged_updates_need_no_archive_and_clean_their_scratch() {
        for available in [crate::VERSION, "0.0.0"] {
            let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
            let (installed, cache, record) = installation(dir.path());
            let _exclusive = exclusive_in(&cache).unwrap();
            let source = Source {
                base: format!("file://{}", dir.path().display()),
                latest: false,
            };
            let name = format!("Atlas-{available}-macos.zip");
            fs::write(
                dir.path().join("SHA256SUMS"),
                format!("{}  {name}\n", "0".repeat(64)),
            )
            .unwrap();
            fs::create_dir_all(cache.join("update")).unwrap();
            fs::write(cache.join("update/stale"), "previous attempt").unwrap();

            let result = install_in(
                &source,
                false,
                false,
                &installed,
                &cache,
                &record,
                async |_| panic!("a no-op must not preflight or swap"),
            )
            .await
            .unwrap();

            assert_eq!(
                result,
                json!({"ok": true, "updated": false, "version": crate::VERSION, "available": name})
            );
            assert_eq!(
                fs::read(installed.join(EXECUTABLE)).unwrap(),
                b"installed app stays"
            );
            assert!(!cache.join("update").exists());
            assert!(!record.exists());
            assert!(
                !dir.path().join(name).exists(),
                "the unused archive is absent"
            );
        }
    }

    #[tokio::test]
    async fn newer_and_forced_updates_still_download_and_verify_the_archive() {
        let newer = format!("{}.0.0", version(crate::VERSION)[0] + 1);
        for (available, force) in [
            (newer.as_str(), false),
            (crate::VERSION, true),
            ("0.0.0", true),
        ] {
            let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
            let (installed, cache, record) = installation(dir.path());
            let _exclusive = exclusive_in(&cache).unwrap();
            let source = Source {
                base: format!("file://{}", dir.path().display()),
                latest: false,
            };
            let name = format!("Atlas-{available}-macos.zip");
            let archive = dir.path().join(&name);
            let sums = dir.path().join("SHA256SUMS");
            fs::write(&sums, format!("{}  {name}\n", "0".repeat(64))).unwrap();

            for expected in [
                "Could not download",
                "Checksum mismatch",
                "could not be unpacked",
            ] {
                let error = install_in(
                    &source,
                    false,
                    force,
                    &installed,
                    &cache,
                    &record,
                    async |_| panic!("unverified archives must not preflight or swap"),
                )
                .await
                .unwrap_err();
                assert!(error.error.contains(expected), "{}", error.error);
                assert!(!cache.join("update").exists());
                assert!(!record.exists());
                assert_eq!(
                    fs::read(installed.join(EXECUTABLE)).unwrap(),
                    b"installed app stays"
                );

                // First supply an archive, then its correct checksum: neither
                // missing bytes nor corrupt bytes can reach installation.
                if !archive.exists() {
                    fs::write(&archive, b"not a zip archive").unwrap();
                } else {
                    let sum = crate::digest::file_sha256(&archive).unwrap();
                    fs::write(&sums, format!("{sum}  {name}\n")).unwrap();
                }
            }
        }
    }

    #[tokio::test]
    async fn invalid_release_metadata_never_reaches_installation() {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (installed, cache, record) = installation(dir.path());
        let _exclusive = exclusive_in(&cache).unwrap();
        let source = Source {
            base: format!("file://{}", dir.path().display()),
            latest: false,
        };
        for metadata in [
            "",
            "00  ../Atlas-0.0.0-macos.zip\n",
            "00  Atlas-bad-macos.zip\n",
        ] {
            fs::write(dir.path().join("SHA256SUMS"), metadata).unwrap();
            let error = install_in(
                &source,
                false,
                false,
                &installed,
                &cache,
                &record,
                async |_| panic!("invalid metadata must not preflight or swap"),
            )
            .await
            .unwrap_err();
            assert_eq!(error.error, "SHA256SUMS does not name an Atlas archive");
            assert!(!cache.join("update").exists());
            assert!(!record.exists());
            assert_eq!(
                fs::read(installed.join(EXECUTABLE)).unwrap(),
                b"installed app stays"
            );
        }
    }

    #[tokio::test]
    async fn an_unfinished_handover_precedes_downloads_and_scratch_cleanup() {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let (installed, cache, record_path) = installation(dir.path());
        let _exclusive = exclusive_in(&cache).unwrap();
        let source = Source {
            base: format!("file://{}/absent", dir.path().display()),
            latest: false,
        };
        let record = json!({"installed": installed, "to": "Atlas-9.9.9-macos.zip"});
        fs::write(&record_path, record.to_string()).unwrap();
        fs::create_dir_all(cache.join("update")).unwrap();
        fs::write(cache.join("update/keep"), "unfinished").unwrap();
        let mut checked = false;
        let result = install_in(
            &source,
            false,
            false,
            &installed,
            &cache,
            &record_path,
            async |program| {
                assert_eq!(program, installed.join(EXECUTABLE));
                checked = true;
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(checked);
        assert_eq!(
            result,
            json!({"ok": true, "updated": true, "resumed": true, "record": record})
        );
        assert_eq!(
            fs::read_to_string(cache.join("update/keep")).unwrap(),
            "unfinished"
        );
        assert_eq!(read_record(&record_path).unwrap(), Some(record));
        assert_eq!(
            fs::read(installed.join(EXECUTABLE)).unwrap(),
            b"installed app stays"
        );
    }

    #[tokio::test]
    async fn setup_refusal_prevents_the_bundle_exchange_and_update_record() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("Applications/Atlas.app");
        let new = dir.path().join("unpacked/Atlas.app");
        let record = dir.path().join("Support/update.json");
        for (app, version) in [(&installed, "old"), (&new, "new")] {
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::write(app.join(EXECUTABLE), version).unwrap();
        }
        let result = swap(
            &new,
            &installed,
            "Atlas-9.9.9-macos.zip",
            &record,
            async |program| {
                assert_eq!(program, new.join(EXECUTABLE));
                assert_eq!(
                    fs::read_to_string(installed.join(EXECUTABLE)).unwrap(),
                    "old"
                );
                Err(problem("setup refuses"))
            },
        )
        .await;
        assert_eq!(result.unwrap_err().error, "setup refuses");
        assert_eq!(
            fs::read_to_string(installed.join(EXECUTABLE)).unwrap(),
            "old"
        );
        assert_eq!(fs::read_to_string(new.join(EXECUTABLE)).unwrap(), "new");
        assert!(!record.exists());
        assert!(staged_copies(&installed).is_empty());
    }

    #[test]
    fn a_check_reads_the_newest_release_and_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let next = format!("Atlas-{}.0.0-macos.zip", version(crate::VERSION)[0] + 1);
        fs::write(
            dir.path().join("SHA256SUMS"),
            format!("{} {next}\n", "0".repeat(64)),
        )
        .unwrap();
        let source = Source {
            base: format!("file://{}", dir.path().display()),
            latest: true,
        };

        let caches = tempfile::tempdir().unwrap();
        let report = newer_in(&source, caches.path()).unwrap();
        assert_eq!(report["newer"], true);
        assert_eq!(report["version"], crate::VERSION);
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "only the sums were there to read"
        );

        let current = format!("Atlas-{}-macos.zip", crate::VERSION);
        fs::write(
            dir.path().join("SHA256SUMS"),
            format!("{} {current}\n", "0".repeat(64)),
        )
        .unwrap();
        assert_eq!(newer_in(&source, caches.path()).unwrap()["newer"], false);
    }

    #[test]
    fn a_check_stopped_midway_leaves_nothing_for_long() {
        let caches = tempfile::tempdir().unwrap();
        let stale = caches.path().join("check-999999");
        let other = caches.path().join("update");
        fs::create_dir_all(&stale).unwrap();
        fs::create_dir_all(&other).unwrap();

        sweep_checks(caches.path());

        assert!(!stale.exists(), "no process 999999 runs");
        assert!(other.exists(), "an update's scratch is not a check's");
    }

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
    fn an_app_must_be_the_version_its_archive_is_named_for() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Atlas.app");
        fs::create_dir_all(app.join("Contents")).unwrap();
        let name = "Atlas-1.2.3-macos.zip";
        assert!(require_named_version(&app, name).is_err(), "no Info.plist");

        for (version, matches) in [("1.2.3", true), ("1.2.4", false), ("1.2.3-beta", false)] {
            fs::write(
                app.join("Contents/Info.plist"),
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                     <key>CFBundleShortVersionString</key><string>{version}</string>\
                     </dict></plist>\n"
                ),
            )
            .unwrap();
            assert_eq!(
                require_named_version(&app, name).is_ok(),
                matches,
                "{version}"
            );
        }
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

    #[tokio::test]
    async fn a_swap_is_recorded_first_and_finishing_removes_only_the_staged_copy() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let installed = root.join("Applications/Atlas.app");
        let new = root.join("unpacked/Atlas.app");
        for (app, version) in [(&installed, "old"), (&new, "new")] {
            fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
            fs::write(app.join("version"), version).unwrap();
            fs::write(app.join(EXECUTABLE), version).unwrap();
        }

        // A staging copy a stopped update left behind is swept first; a
        // person's own folders beside the app, and install.sh's, stay.
        let stale = root.join(format!(
            "Applications/.Atlas.app.update-{}",
            crate::files::token()
        ));
        let legacy = root.join(format!("Applications/.Atlas.app.{}", crate::files::token()));
        let kept = [
            ".Atlas.app.feature",
            ".Atlas.app.previous",
            ".Atlas.app.install-42",
        ]
        .map(|name| root.join("Applications").join(name));
        for folder in [&stale, &legacy].into_iter().chain(&kept) {
            fs::create_dir_all(folder).unwrap();
        }

        let record = root.join("Support/update.json");
        swap_recording(&new, &installed, "Atlas-9.9.9-macos.zip", &record).unwrap();
        assert!(!stale.exists() && !legacy.exists());
        for folder in &kept {
            assert!(folder.exists(), "{}", folder.display());
        }
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "new"
        );

        let plan = read_record(&record).unwrap().unwrap();
        let previous = PathBuf::from(plan["previous"].as_str().unwrap());
        assert_eq!(previous.parent(), installed.parent());
        assert!(
            previous
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".Atlas.app.update-")),
            "{}",
            previous.display()
        );
        assert_eq!(staged_copies(&installed), vec![previous.clone()]);
        assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "old");
        assert_eq!(
            plan["executable"],
            crate::digest::file_sha256(&installed.join(EXECUTABLE)).unwrap()
        );

        // The exchange happened: resuming changes nothing.
        resume(&plan, &installed, async |_| Ok(())).await.unwrap();
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

        let refused = resume(&plan, &installed, async |program| {
            assert_eq!(program, previous.join(EXECUTABLE));
            Err(problem("setup refuses"))
        })
        .await;
        assert_eq!(refused.unwrap_err().error, "setup refuses");
        assert_eq!(
            fs::read_to_string(installed.join("version")).unwrap(),
            "old"
        );
        assert_eq!(fs::read_to_string(previous.join("version")).unwrap(), "new");
        resume(&plan, &installed, async |_| Ok(())).await.unwrap();
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

        let token = crate::files::token();
        for previous in [
            root.join("Documents"),
            root.join("Applications/Other.app"),
            root.join("Applications/.Atlas.app.previous"),
            root.join("Applications/.Atlas.app.feature"),
            root.join("Applications/.Atlas.app.install-42"),
            root.join(format!("Elsewhere/.Atlas.app.update-{token}")),
        ] {
            fs::create_dir_all(&previous).unwrap();
            let plan = json!({"previous": previous, "installed": installed, "to": "x"});
            fs::write(&record, plan.to_string()).unwrap();
            assert_eq!(finish_record(&record).unwrap()["removed"], false);
            assert!(previous.exists(), "{}", previous.display());
            assert!(!record.exists());
        }

        let staged = root.join(format!("Applications/.Atlas.app.update-{token}"));
        assert!(staged_beside(&staged, Some(&installed)));
        assert!(!staged_beside(&staged, None));

        // A record an earlier Atlas wrote names its copy without the prefix.
        let legacy = root.join(format!("Applications/.Atlas.app.{token}"));
        assert!(staged_beside(&legacy, Some(&installed)));

        for name in [
            ".Atlas.app.",
            ".Atlas.app.update-",
            ".Atlas.app.update-1",
            ".Atlas.app.1",
            ".Atlas.app.previous",
            &format!(".Atlas.app.update-{}", token.to_uppercase()),
            &format!(".Atlas.app.update-{token}0"),
        ] {
            let path = root.join("Applications").join(name);
            assert!(!staged_beside(&path, Some(&installed)), "{name}");
        }
    }

    /// Uninstall clears every copy Atlas staged, by update or by
    /// install.sh, and nothing else beside the app.
    #[test]
    fn only_copies_atlas_staged_are_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let installed = root.join("Atlas.app");
        let token = crate::files::token();

        let ours = [
            format!(".Atlas.app.update-{token}"),
            format!(".Atlas.app.{token}"),
            ".Atlas.app.install-123".to_owned(),
            ".Atlas.app.install-previous-123".to_owned(),
        ];
        let theirs = [
            ".Atlas.app.feature",
            ".Atlas.app.previous",
            ".Atlas.app.previous.123",
            ".Atlas.app.123",
            ".Atlas.app.install-",
            ".Atlas.app.install-12x",
            ".Atlas.app.install-previous-",
            "Atlas.app.update-1",
        ];
        for name in ours.iter().map(String::as_str).chain(theirs) {
            fs::create_dir_all(root.join(name)).unwrap();
        }

        let mut found = atlas_copies(&installed);
        found.sort();
        let mut expected: Vec<PathBuf> = ours.iter().map(|name| root.join(name)).collect();
        expected.sort();
        assert_eq!(found, expected);
        assert_eq!(
            staged_copies(&installed).len(),
            2,
            "update sweeps only its own"
        );
    }

    /// Two updates never run at once on one account.
    #[test]
    fn one_update_runs_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = exclusive_in(dir.path()).unwrap();
        let error = exclusive_in(dir.path()).unwrap_err();
        assert!(
            error.error.contains("Another atlas update"),
            "{}",
            error.error
        );

        drop(first);
        // A process another test starts at this moment may hold a copy of
        // the lock's file for an instant before it runs its program.
        let again = (0..50).find_map(|_| {
            exclusive_in(dir.path()).ok().or_else(|| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                None
            })
        });
        assert!(
            again.is_some(),
            "the lock is free once the first is dropped"
        );
    }

    /// Releases come over https or from a folder; plain http only from
    /// this Mac, as a rehearsal serves them.
    #[test]
    fn releases_come_only_over_https_a_folder_or_this_mac() {
        for (url, allowed) in [
            (
                "https://atlas.insanearts.io/releases/latest/SHA256SUMS",
                Some("=https"),
            ),
            ("file:///tmp/releases/latest/SHA256SUMS", Some("=file")),
            (
                "http://127.0.0.1:8787/releases/latest/SHA256SUMS",
                Some("=http"),
            ),
            ("http://localhost:8787/releases/latest", Some("=http")),
            ("http://localhost/releases", Some("=http")),
            ("http://127.0.0.1", Some("=http")),
            ("http://example.com/releases", None),
            ("http://127.0.0.1.example.com/releases", None),
            ("http://127.0.0.1:8787@example.com/releases", None),
            ("http://localhost@example.com/releases", None),
            ("http://127.0.0.1:/releases", None),
            ("ftp://127.0.0.1/releases", None),
        ] {
            assert_eq!(protocol(url).ok(), allowed, "{url}");
        }
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
