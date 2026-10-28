//! Apps installed into the helper's `~/Applications` for a task, run inside
//! the helper by the environment worker. An install goes only where
//! nothing is, and is removed only when it is still the exact copy Atlas
//! made: its recorded destination, its folder's inode and a print of every
//! entry inside it. A bundle identifier or a name alone never selects
//! anything to remove, and files the app writes outside its bundle are
//! reported, never removed. Every record change is written before the step
//! it announces, so a worker cut short is finished by the next request.
use super::{model::Node, *};
use sha2::{Digest, Sha256};
use std::{io::Read, os::unix::ffi::OsStrExt, process::Command};

/// Where install records live in the helper's home.
const INSTALLS: &str = ".config/atlas/installs";
/// Where installs go, in the helper's home.
const APPLICATIONS: &str = "Applications";
/// The most entries and bytes a bundle may have and still be printed.
const MAX_ENTRIES: u64 = 100_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// The most installs one read asks about.
const ASKED: usize = 64;
/// Where an app keeps what it writes outside its bundle, by its bundle
/// identifier: each folder, and what follows the identifier in its name.
const SIDE_EFFECTS: [(&str, &str); 6] = [
    ("Library/Preferences", ".plist"),
    ("Library/Application Support", ""),
    ("Library/Caches", ""),
    ("Library/Containers", ""),
    ("Library/HTTPStorages", ""),
    ("Library/Saved Application State", ".savedState"),
];

/// One install Atlas made, as the helper keeps it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Install {
    pub version: u32,
    pub id: String,
    /// The bundle's file name, under [`APPLICATIONS`].
    pub name: String,
    pub phase: InstallPhase,
    /// Whether anything was at the destination before. Atlas refuses to
    /// install over anything, so an install it made always says false.
    pub existed: bool,
    pub bundle_id: Option<String>,
    pub version_string: Option<String>,
    pub print: Option<BundlePrint>,
    /// Whether [`APPLICATIONS`] was made for this install; it is never
    /// removed.
    pub made_folder: bool,
    /// Which files keyed by the bundle identifier existed at install time.
    pub before: BTreeMap<String, bool>,
    /// Why it was kept, once it was: `changed` or `replaced`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<String>,
}

/// How far an install, or its removal, got.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum InstallPhase {
    /// Being copied into the store's staging folder.
    Copying,
    /// Printed, and being moved into place.
    Placing,
    Installed,
    /// Being moved out of place and deleted.
    Removing,
    Removed,
    /// Left in place on purpose: it changed, or something else is there.
    Kept,
}

/// What a bundle is, as far as removing it needs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BundlePrint {
    /// The bundle folder itself.
    pub root: Node,
    /// SHA-256 over every entry inside, sorted by path: its path, type,
    /// permissions, owner and inode, and for files and links their size,
    /// modification time and a link's target. Directory times and extended
    /// attributes are left out: macOS changes those itself when an app is
    /// opened.
    pub tree: String,
    pub entries: u64,
    pub bytes: u64,
    /// SHA-256 of Contents/Info.plist and of the main executable.
    pub info: String,
    pub executable: Option<String>,
}

/// The helper's processes, each with the executable it runs; tests stand
/// in for `ps`.
pub(super) type Processes<'a> = &'a dyn Fn() -> Result<Vec<(u32, String)>>;

/// The helper's own processes, from `ps`.
pub(super) fn helper_processes() -> Result<Vec<(u32, String)>> {
    let uid = nix::unistd::getuid().as_raw().to_string();
    let output = Command::new("/bin/ps")
        .args(["-U", &uid, "-o", "pid=,comm="])
        .output()?;
    if !output.status.success() {
        return Err(problem("Could not list the helper's processes"));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, executable) = line.trim_start().split_once(' ')?;
            Some((pid.parse().ok()?, executable.trim().to_owned()))
        })
        .collect())
}

/// The install store in `home`, locked: exclusively to change it, shared
/// to read it. None when reading a store that was never made.
struct Store {
    path: PathBuf,
    _lock: Option<fs::File>,
}

impl Store {
    fn open(home: &Path, write: bool) -> Result<Option<Self>> {
        let path = home.join(INSTALLS);
        let folder = if write {
            Some(private_directory(&path)?)
        } else {
            directory(&path, false)?
        };
        let Some(folder) = folder else {
            return Ok(None);
        };
        validate_private(&folder, true)?;

        let lock = open_regular(&path.join("lock"), write)?;
        if let Some(lock) = &lock {
            validate_private(lock, false)?;
            let until = std::time::Instant::now() + Duration::from_millis(250);
            loop {
                let taken = if write {
                    lock.try_lock_exclusive()
                } else {
                    FileExt::try_lock_shared(lock)
                };
                if taken.is_ok() {
                    break;
                }
                if std::time::Instant::now() >= until {
                    return Err(problem("Another install in the helper is running"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(Some(Self { path, _lock: lock }))
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.path.join(format!("install-{id}"))
    }

    fn load(&self, id: &str) -> Result<Option<Install>> {
        let path = self.folder(id).join("record.json");
        if fs::symlink_metadata(&path).is_err() {
            return Ok(None);
        }
        let install: Install = decode(&read_limited(&path, MAX_RECORD)?)?;
        if install.id != id || install.version != 1 {
            return Err(problem("Invalid install record"));
        }
        Ok(Some(install))
    }

    fn save(&self, install: &Install) -> Result<()> {
        let folder = self.folder(&install.id);
        private_directory(&folder)?;
        atomic_write(
            &folder.join("record.json"),
            &serde_json::to_vec(install)?,
            0o600,
        )
    }

    /// A private folder of the install's own, `which` being "staging" or
    /// "removing", made when missing.
    fn place(&self, id: &str, which: &str) -> Result<(PathBuf, fs::File)> {
        let path = self.folder(id).join(which);
        let folder = private_directory(&path)?;
        Ok((path, folder))
    }
}

/// Whether `name` is a bundle's plain file name: `NAME.app`, not hidden.
fn bundle_name(name: &str) -> bool {
    name.len() > 4
        && name.len() <= 255
        && name.ends_with(".app")
        && !name.starts_with('.')
        && !name.contains(['/', '\0', '\n'])
}

/// Refuse an install before anything changes, as the person reads it.
fn refused(message: impl Into<String>) -> Error {
    Error::new("input", message)
}

/// Install the bundle at `source` (a path the helper can read) as
/// `~/Applications/NAME` for install `id`: copied into the store, printed,
/// then moved into place only where nothing is. A refusal before anything
/// changed is an error; a failure after is a reply that says the install
/// must be finished, which removing it by its ID does.
pub(super) fn install(
    home: &Path,
    id: &str,
    source: &Path,
    name: &str,
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    validate_id(id)?;
    if !bundle_name(name) || !source.is_absolute() {
        return Err(refused(
            "Atlas installs an app bundle, NAME.app, by its path",
        ));
    }
    guard()?;

    let store = Store::open(home, true)?.ok_or_else(|| problem("Missing install store"))?;
    if store.load(id)?.is_some() {
        return Err(refused("An install with this ID was already made"));
    }
    let applications = home.join(APPLICATIONS);
    let folder = directory(&applications, false)
        .map_err(|_| refused("The helper's Applications is not a folder Atlas may install into"))?;
    if let Some(folder) = &folder {
        installation_parent(folder).map_err(|_| {
            refused("The helper's Applications is not a folder only the helper can change")
        })?;
    }
    let destination = applications.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(refused(format!(
            "The helper already has Applications/{name}. Atlas never installs over an app it \
             did not install; open that one, or ask the person."
        )));
    }
    if !fs::symlink_metadata(source).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(refused(
            "The app to install is not a folder the helper can read",
        ));
    }

    let mut record = Install {
        version: 1,
        id: id.to_owned(),
        name: name.to_owned(),
        phase: InstallPhase::Copying,
        existed: false,
        bundle_id: None,
        version_string: None,
        print: None,
        made_folder: false,
        before: BTreeMap::new(),
        kept: None,
    };
    store.save(&record)?;

    match place(home, &store, &mut record, source, guard) {
        Ok(Placed::Installed) => Ok(json!({
            "ok": true,
            "installed": true,
            "installId": id,
            "app": name.trim_end_matches(".app"),
            "path": destination,
            "bundleId": record.bundle_id,
            "version": record.version_string,
        })),
        Ok(Placed::Refused(error)) => Err(error),
        Err(error) => Ok(json!({
            "ok": false,
            "installed": false,
            "installId": id,
            "phase": record.phase,
            "error": error.error,
        })),
    }
}

/// How placing an install ended.
enum Placed {
    Installed,
    /// Refused, with nothing left of it but its record, which says removed.
    Refused(Error),
}

fn place(
    home: &Path,
    store: &Store,
    record: &mut Install,
    source: &Path,
    guard: &impl Fn() -> Result<()>,
) -> Result<Placed> {
    let (staging, staging_folder) = store.place(&record.id, "staging")?;
    let staged = staging.join(&record.name);
    let give_up = |record: &mut Install, error: Error| -> Result<Placed> {
        let _ = fs::remove_dir_all(&staging);
        record.phase = InstallPhase::Removed;
        store.save(record)?;
        Ok(Placed::Refused(error))
    };

    guard()?;
    let copied = Command::new("/usr/bin/ditto")
        .arg("--noacl")
        .arg(source)
        .arg(&staged)
        .output()?;
    if !copied.status.success() {
        return give_up(
            record,
            refused("The helper could not copy that app; check it can be read"),
        );
    }

    let plist = staged.join("Contents/Info.plist");
    let value = |key: &str| bundle_value(&plist, key);
    record.bundle_id = value("CFBundleIdentifier").filter(|id| plain_identifier(id));
    record.version_string = match (
        value("CFBundleShortVersionString"),
        value("CFBundleVersion"),
    ) {
        (Some(short), Some(build)) if short != build => Some(format!("{short} ({build})")),
        (Some(short), _) => Some(short),
        (None, build) => build,
    };
    let executable = value("CFBundleExecutable").filter(|name| single_component(name).is_ok());
    let print = match print(&staged, executable.as_deref()) {
        Ok(print) => print,
        Err(error) => return give_up(record, error),
    };
    if let Some(bundle_id) = &record.bundle_id {
        record.before = side_effects(home, bundle_id).into_iter().collect();
    }
    record.print = Some(print);
    record.phase = InstallPhase::Placing;
    store.save(record)?;

    let applications = home.join(APPLICATIONS);
    let folder = match directory(&applications, false)? {
        Some(folder) => folder,
        None => {
            let folder = create_private_directory(&applications)?;
            record.made_folder = true;
            store.save(record)?;
            folder
        }
    };
    installation_parent(&folder)?;

    guard()?;
    let name = single_component(&record.name)?;
    match rename_between(&staging_folder, &name, &folder, &name) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return give_up(
                record,
                refused(format!(
                    "Something arrived at the helper's Applications/{} while Atlas was \
                     installing; it was left alone",
                    record.name
                )),
            );
        }
        Err(error) => return Err(error.into()),
    }

    record.phase = InstallPhase::Installed;
    store.save(record)?;
    let _ = fs::remove_dir_all(&staging);
    Ok(Placed::Installed)
}

/// Whether `id` is a bundle identifier of `A-Z a-z 0-9 . -` only, as files
/// keyed by it may be looked for.
fn plain_identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}

/// One string `plist` holds at `key`, by plutil.
fn bundle_value(plist: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(plist)
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (output.status.success() && !value.is_empty() && value.len() <= 255).then(|| value.to_owned())
}

/// Which of the files an app with bundle identifier `id` keeps outside its
/// bundle exist now, each by its path in `home`. Each is looked at without
/// following a link, and none is opened.
fn side_effects(home: &Path, id: &str) -> Vec<(String, bool)> {
    SIDE_EFFECTS
        .iter()
        .map(|(folder, suffix)| {
            let path = format!("{folder}/{id}{suffix}");
            let exists = fs::symlink_metadata(home.join(&path)).is_ok();
            (path, exists)
        })
        .collect()
}

/// The print of the bundle at `bundle`, whose main executable is called
/// `executable`, walked without ever following a link.
pub(super) fn print(bundle: &Path, executable: Option<&str>) -> Result<BundlePrint> {
    print_within(
        bundle,
        executable,
        Limits {
            entries: MAX_ENTRIES,
            bytes: MAX_BYTES,
        },
    )
}

/// How large a bundle may be and still be printed.
#[derive(Clone, Copy)]
struct Limits {
    entries: u64,
    bytes: u64,
}

/// [`print`], refusing a bundle larger than `limits`.
fn print_within(bundle: &Path, executable: Option<&str>, limits: Limits) -> Result<BundlePrint> {
    let root = fs::symlink_metadata(bundle)?;
    if !root.is_dir() {
        return Err(problem("An app bundle is a folder"));
    }
    let oversized =
        || refused("That app is too large for Atlas to check it later, so it was not installed");

    let mut entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let (mut count, mut bytes) = (0u64, 0u64);
    let mut folders = vec![bundle.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            count += 1;
            if count > limits.entries {
                return Err(oversized());
            }
            let relative = path
                .strip_prefix(bundle)
                .map_err(|_| problem("An entry lies outside its bundle"))?
                .as_os_str()
                .as_bytes()
                .to_vec();
            let kind = metadata.file_type();
            let mut line = format!(
                "{}:{:o}:{}:{}:{}",
                if kind.is_dir() {
                    'd'
                } else if kind.is_symlink() {
                    'l'
                } else if kind.is_file() {
                    'f'
                } else {
                    'o'
                },
                metadata.mode() & 0o7777,
                metadata.uid(),
                metadata.gid(),
                metadata.ino(),
            )
            .into_bytes();
            if kind.is_file() || kind.is_symlink() {
                line.extend(
                    format!(
                        ":{}:{}.{}",
                        metadata.len(),
                        metadata.mtime(),
                        metadata.mtime_nsec()
                    )
                    .bytes(),
                );
            }
            if kind.is_symlink() {
                line.push(b':');
                line.extend(fs::read_link(&path)?.as_os_str().as_bytes());
            }
            if kind.is_file() {
                bytes += metadata.len();
                if bytes > limits.bytes {
                    return Err(oversized());
                }
            }
            if kind.is_dir() {
                folders.push(path);
            }
            entries.push((relative, line));
        }
    }
    entries.sort();

    let mut tree = Sha256::new();
    for (path, line) in &entries {
        tree.update(path);
        tree.update([0]);
        tree.update(line);
        tree.update([b'\n']);
    }

    let contents = bundle.join("Contents");
    let info = file_digest(&contents.join("Info.plist"))?
        .ok_or_else(|| problem("The app has no Contents/Info.plist"))?;
    let executable = match executable {
        Some(name) => file_digest(&contents.join("MacOS").join(name))?,
        None => None,
    };

    Ok(BundlePrint {
        root: node(&root),
        tree: hex(&tree.finalize()),
        entries: count,
        bytes,
        info,
        executable,
    })
}

/// The SHA-256 of the plain file at `path`, read without following a link;
/// none when nothing plain is there.
fn file_digest(path: &Path) -> Result<Option<String>> {
    use std::os::unix::fs::OpenOptionsExt;

    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Ok(None);
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hex(&hasher.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What removing an install came to, as its reply names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Removed,
    AlreadyGone,
    Kept(&'static str),
    InUse,
}

impl Outcome {
    fn said(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Removed => ("removed", None),
            Self::AlreadyGone => ("alreadyGone", None),
            Self::Kept(reason) => ("kept", Some(reason)),
            Self::InUse => ("inUse", None),
        }
    }
}

/// Remove install `id`, when it is still the exact copy Atlas made and
/// nothing in the helper runs from it. Anything else is kept and said. A
/// removal cut short is finished, every check made again first.
pub(super) fn remove(
    home: &Path,
    id: &str,
    guard: &impl Fn() -> Result<()>,
    processes: Processes,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;
    if directory(&home.join(INSTALLS), false)?.is_none() {
        return Ok(removal(id, Outcome::AlreadyGone, None, home));
    }
    let store = Store::open(home, true)?.ok_or_else(|| problem("Missing install store"))?;
    let Some(mut record) = store.load(id)? else {
        return Ok(removal(id, Outcome::AlreadyGone, None, home));
    };
    let outcome = remove_recorded(home, &store, &mut record, guard, processes)?;
    Ok(removal(id, outcome, Some(&record), home))
}

fn remove_recorded(
    home: &Path,
    store: &Store,
    record: &mut Install,
    guard: &impl Fn() -> Result<()>,
    processes: Processes,
) -> Result<Outcome> {
    let destination = home.join(APPLICATIONS).join(&record.name);
    let placed = || {
        fs::symlink_metadata(&destination)
            .ok()
            .is_some_and(|metadata| {
                record
                    .print
                    .as_ref()
                    .is_some_and(|print| metadata.is_dir() && node(&metadata) == print.root)
            })
    };

    match record.phase {
        InstallPhase::Removed => return Ok(Outcome::AlreadyGone),
        InstallPhase::Kept => {
            return Ok(Outcome::Kept(match record.kept.as_deref() {
                Some("replaced") => "replaced",
                _ => "changed",
            }));
        }
        InstallPhase::Copying => return finish_unplaced(store, record),
        InstallPhase::Placing if !placed() => return finish_unplaced(store, record),
        InstallPhase::Removing => {
            discard_removing(store, record);
            if !placed() {
                record.phase = InstallPhase::Removed;
                store.save(record)?;
                return Ok(Outcome::Removed);
            }
        }
        InstallPhase::Placing | InstallPhase::Installed => {}
    }

    // Placed, or found in place: it goes only as the copy Atlas made.
    let Ok(metadata) = fs::symlink_metadata(&destination) else {
        record.phase = InstallPhase::Removed;
        store.save(record)?;
        return Ok(Outcome::AlreadyGone);
    };
    let Some(print) = record.print.clone() else {
        return keep(store, record, "changed");
    };
    if !metadata.is_dir() || node(&metadata) != print.root {
        return keep(store, record, "replaced");
    }
    let executable = bundle_value(
        &destination.join("Contents/Info.plist"),
        "CFBundleExecutable",
    )
    .filter(|name| single_component(name).is_ok());
    if self::print(&destination, executable.as_deref())
        .ok()
        .as_ref()
        != Some(&print)
    {
        return keep(store, record, "changed");
    }
    let inside = format!("{}/", destination.display());
    if processes()?
        .iter()
        .any(|(_, executable)| executable.starts_with(&inside))
    {
        return Ok(Outcome::InUse);
    }

    record.phase = InstallPhase::Removing;
    store.save(record)?;
    guard()?;
    let (removing, removing_folder) = store.place(&record.id, "removing")?;
    let applications = directory(&home.join(APPLICATIONS), false)?
        .ok_or_else(|| problem("The helper's Applications disappeared"))?;
    installation_parent(&applications)?;
    let name = single_component(&record.name)?;
    rename_between(&applications, &name, &removing_folder, &name)?;

    // What was moved must be what was checked; anything else goes back.
    let moved = removing.join(&record.name);
    let same = fs::symlink_metadata(&moved).is_ok_and(|metadata| node(&metadata) == print.root);
    if !same {
        rename_between(&removing_folder, &name, &applications, &name)?;
        return keep(store, record, "replaced");
    }
    fs::remove_dir_all(&moved)?;
    record.phase = InstallPhase::Removed;
    store.save(record)?;
    let _ = fs::remove_dir(&removing);
    Ok(Outcome::Removed)
}

/// An install never placed: what it staged goes, and it is removed.
fn finish_unplaced(store: &Store, record: &mut Install) -> Result<Outcome> {
    let _ = fs::remove_dir_all(store.folder(&record.id).join("staging"));
    record.phase = InstallPhase::Removed;
    store.save(record)?;
    Ok(Outcome::AlreadyGone)
}

/// Delete what a removal cut short left in the install's own folder.
fn discard_removing(store: &Store, record: &Install) {
    let _ = fs::remove_dir_all(store.folder(&record.id).join("removing"));
}

/// Keep the install in place for `reason`, and say so from now on.
fn keep(store: &Store, record: &mut Install, reason: &'static str) -> Result<Outcome> {
    record.phase = InstallPhase::Kept;
    record.kept = Some(reason.to_owned());
    store.save(record)?;
    Ok(Outcome::Kept(reason))
}

/// The reply to a removal: what became of the bundle, and, apart from it,
/// which files keyed by its identifier exist now and whether each existed
/// when it was installed. Those are never removed.
fn removal(id: &str, outcome: Outcome, record: Option<&Install>, home: &Path) -> Value {
    let (said, reason) = outcome.said();
    let mut reply = json!({"ok": true, "installId": id, "outcome": said});
    if let Some(reason) = reason {
        reply["reason"] = json!(reason);
    }
    if let Some(record) = record {
        reply["app"] = json!(record.name.trim_end_matches(".app"));
        if let Some(bundle_id) = &record.bundle_id {
            let kept: Vec<Value> = side_effects(home, bundle_id)
                .into_iter()
                .filter(|(_, exists)| *exists)
                .map(|(path, _)| {
                    let before = record.before.get(&path).copied().unwrap_or(false);
                    json!({"path": path, "existedBefore": before})
                })
                .collect();
            reply["sideEffects"] = json!({"kept": kept});
        }
    }
    reply
}

/// How each of `ids` stands, read only: its phase, where it is, whether it
/// is there and unchanged, and the helper's processes running from it.
pub(super) fn installs(
    home: &Path,
    ids: &[String],
    guard: &impl Fn() -> Result<()>,
    processes: Processes,
) -> Result<Value> {
    guard()?;
    if ids.len() > ASKED {
        return Err(problem("Too many installs were asked about"));
    }
    for id in ids {
        validate_id(id)?;
    }
    let store = Store::open(home, false)?;
    let running = if ids.is_empty() {
        Vec::new()
    } else {
        processes()?
    };

    let listed: Vec<Value> = ids
        .iter()
        .map(|id| {
            let record = store
                .as_ref()
                .and_then(|store| store.load(id).ok().flatten());
            let Some(record) = record else {
                return json!({"installId": id, "phase": "missing"});
            };
            let destination = home.join(APPLICATIONS).join(&record.name);
            let inside = format!("{}/", destination.display());
            let present = fs::symlink_metadata(&destination).is_ok();
            let executable = bundle_value(
                &destination.join("Contents/Info.plist"),
                "CFBundleExecutable",
            )
            .filter(|name| single_component(name).is_ok());
            let unchanged = present
                && record.print.as_ref().is_some_and(|recorded| {
                    print(&destination, executable.as_deref()).ok().as_ref() == Some(recorded)
                });
            let from: Vec<Value> = running
                .iter()
                .filter(|(_, executable)| executable.starts_with(&inside))
                .map(|(pid, executable)| json!({"pid": pid, "executable": executable}))
                .collect();
            json!({
                "installId": id,
                "phase": record.phase,
                "app": record.name.trim_end_matches(".app"),
                "path": destination,
                "present": present,
                "unchanged": unchanged,
                "bundleId": record.bundle_id,
                "processes": from,
            })
        })
        .collect();
    Ok(json!({"ok": true, "installs": listed}))
}

#[cfg(test)]
mod tests;
