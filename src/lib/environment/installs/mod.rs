//! Apps installed into the helper's `~/Applications` for a task, run inside
//! the helper by the environment worker. An install goes only where
//! nothing is, and is taken away only when it is still the exact copy
//! Atlas made: its recorded destination, its folder's node, mode and
//! owner, and a print of every entry inside it. Taking it away never
//! deletes it: it is moved, never over anything and never by copying, into
//! the helper's Trash under a name of its own, so a wrong judgement can
//! always be undone by the person. A bundle identifier or a name alone
//! never selects anything, and files the app writes outside its bundle are
//! reported, never touched. Every record change is written before the step
//! it announces, so a worker cut short is finished by the next request. A
//! record this build cannot read is never acted on: its install is kept as
//! it is, and said to be unreadable.
mod print;
mod processes;
#[cfg(test)]
mod tests;
#[allow(
    unsafe_code,
    reason = "FFI: Security's SecTranslocateCreateOriginalPathForURL has no safe binding"
)]
mod translocate;

use super::*;
use crate::fs_move::{FileName, Moved, move_between};
pub(super) use print::{BundlePrint, Limits};
use print::{Unprinted, print_at};
pub(super) use processes::Looks;
use processes::{Runs, running_from};
use std::{cell::Cell, process::Command, time::Instant};

/// Where install records live in the helper's home.
const INSTALLS: &str = ".config/atlas/installs";
/// Where installs go, in the helper's home.
const APPLICATIONS: &str = "Applications";
/// The most installs one read asks about.
pub(crate) const ASKED: usize = 64;
/// How long one read may spend printing installs to say whether each is
/// unchanged; past it, the rest are said to be unknown.
const READ_PRINTS: Duration = Duration::from_secs(10);
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
    /// The main executable's file name, as the bundle said when installed.
    #[serde(default)]
    pub executable: Option<String>,
    pub print: Option<BundlePrint>,
    /// Whether [`APPLICATIONS`] was made for this install; it is never
    /// removed.
    pub made_folder: bool,
    /// Which files keyed by the bundle identifier existed at install time.
    pub before: BTreeMap<String, bool>,
    /// Why it was kept, once it was: `changed`, `replaced` or
    /// `other-volume`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<String>,
    /// Its name in the helper's Trash, once moving it there was announced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed: Option<String>,
}

/// How far an install, or taking it away, got.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum InstallPhase {
    /// Being copied into the store's staging folder.
    Copying,
    /// Printed, and being moved into place.
    Placing,
    Installed,
    /// Checked to be the copy Atlas made, and being moved into the
    /// helper's Trash under its recorded name.
    Trashing,
    /// In the helper's Trash.
    Trashed,
    /// Never placed: nothing of it is left.
    Removed,
    /// Left in place on purpose: it changed, something else is there, or
    /// it could not be moved to the Trash without copying.
    Kept,
}

/// What the store holds for one install ID.
enum Held {
    Missing,
    /// A record this build cannot read: never acted on.
    Unreadable,
    Record(Box<Install>),
}

/// The install store in `home`, locked: exclusively to change it, shared
/// to read it. None when reading a store that was never made.
struct Store {
    path: PathBuf,
    _lock: Option<fs::File>,
    /// Whether this request has written a record yet, so a failure after
    /// it is said as one to finish, never as a refusal.
    changed: Cell<bool>,
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
            let until = Instant::now() + Duration::from_millis(250);
            loop {
                let taken = if write {
                    lock.try_lock_exclusive()
                } else {
                    FileExt::try_lock_shared(lock)
                };
                if taken.is_ok() {
                    break;
                }
                if Instant::now() >= until {
                    return Err(problem("Another install in the helper is running"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Ok(Some(Self {
            path,
            _lock: lock,
            changed: Cell::new(false),
        }))
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.path.join(format!("install-{id}"))
    }

    /// The record of install `id`. One that is there but does not read as
    /// this build writes it, of any version, is unreadable, and is left
    /// exactly as it is.
    fn load(&self, id: &str) -> Held {
        let path = self.folder(id).join("record.json");
        if fs::symlink_metadata(&path)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Held::Missing;
        }
        let install = read_limited(&path, MAX_RECORD)
            .and_then(|bytes| decode::<Install>(&bytes))
            .ok()
            .filter(|install| install.id == id && install.version == 1);
        match install {
            Some(install) => Held::Record(Box::new(install)),
            None => Held::Unreadable,
        }
    }

    fn save(&self, install: &Install) -> Result<()> {
        let folder = self.folder(&install.id);
        private_directory(&folder)?;
        self.changed.set(true);
        atomic_write(
            &folder.join("record.json"),
            &serde_json::to_vec(install)?,
            0o600,
        )
    }

    /// The install's own staging folder, made when missing.
    fn staging(&self, id: &str) -> Result<(PathBuf, fs::File)> {
        let path = self.folder(id).join("staging");
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
        && FileName::new(name).is_some()
        && !name.contains('\n')
}

/// Refuse an install before anything changes, as the person reads it.
fn refused(message: impl Into<String>) -> Error {
    Error::new("input", message)
}

/// `name` as a file name, which every recorded name is.
fn file_name(name: &str) -> Result<FileName> {
    FileName::new(name).ok_or_else(|| problem("Invalid file name"))
}

/// Remove `path` and everything in it; absence is success, any other
/// failure is said, so nothing is taken for gone that is not.
fn delete(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
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
    install_moving(home, id, source, name, guard, &move_between)
}

/// [`install`], moving with `mover`.
fn install_moving(
    home: &Path,
    id: &str,
    source: &Path,
    name: &str,
    guard: &impl Fn() -> Result<()>,
    mover: Mover,
) -> Result<Value> {
    validate_id(id)?;
    if !bundle_name(name) || !source.is_absolute() {
        return Err(refused(
            "Atlas installs an app bundle, NAME.app, by its path",
        ));
    }
    guard()?;

    let store = Store::open(home, true)?.ok_or_else(|| problem("Missing install store"))?;
    if !matches!(store.load(id), Held::Missing) {
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
        executable: None,
        print: None,
        made_folder: false,
        before: BTreeMap::new(),
        kept: None,
        trashed: None,
    };
    store.save(&record)?;

    match place(home, &store, &mut record, source, guard, mover) {
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
        Err(error) => Ok(unfinished(id, &record, &error)),
    }
}

/// The reply to a request that changed the helper and then failed: one to
/// finish, never a refusal.
fn unfinished(id: &str, record: &Install, error: &Error) -> Value {
    json!({
        "ok": false,
        "installId": id,
        "phase": record.phase,
        "error": error.error,
    })
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
    mover: Mover,
) -> Result<Placed> {
    let (staging, staging_folder) = store.staging(&record.id)?;
    let staged = staging.join(&record.name);
    let give_up = |record: &mut Install, error: Error| -> Result<Placed> {
        delete(&staging)?;
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
    record.executable = value("CFBundleExecutable").filter(|name| FileName::new(name).is_some());
    let staged_folder = directory(&staged, false)?.ok_or_else(|| problem("The copy is missing"))?;
    let print = match print_at(
        &staged_folder,
        record.executable.as_deref(),
        Limits::default(),
    ) {
        Ok(print) => print,
        Err(unprinted) => return give_up(record, print::refusal(unprinted)),
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
    let name = file_name(&record.name)?;
    match mover(&staging_folder, &name, &folder, &name) {
        Ok(Moved::Synced) => {}
        // Moved, but not written through: the record stays placing, with
        // the print that proves the bundle at the destination is this one,
        // until a removal finds it there and takes it away.
        Ok(Moved::Unsynced(error)) => return Err(error.into()),
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
    delete(&staging)?;
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

/// Where the helper keeps its Trash, in its home.
const TRASH: &str = ".Trash";

/// The name install `id` of the bundle `name` takes in the helper's
/// Trash: its own, so it never meets another.
fn trash_name(name: &str, id: &str) -> String {
    format!("{name} (atlas {})", &id[..id.len().min(16)])
}

/// What taking an install away came to, as its reply names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Moved into the helper's Trash, under this name.
    Trashed(String),
    AlreadyGone,
    Kept(&'static str),
    InUse,
}

/// Take install `id` away, into the helper's Trash, when it is still the
/// exact copy Atlas made and nothing in the helper runs from it. Anything
/// else is kept and said. One cut short is finished, every check made
/// again first; a failure after anything changed is a reply to finish,
/// never a refusal.
pub(super) fn remove(
    home: &Path,
    id: &str,
    guard: &impl Fn() -> Result<()>,
    looks: &Looks,
) -> Result<Value> {
    remove_moving(home, id, guard, looks, &move_between)
}

/// What moves an entry between two folders; tests stand in for it to cut
/// a move short.
type Mover<'a> = &'a dyn Fn(&fs::File, &FileName, &fs::File, &FileName) -> std::io::Result<Moved>;

/// [`remove`], moving with `mover`.
fn remove_moving(
    home: &Path,
    id: &str,
    guard: &impl Fn() -> Result<()>,
    looks: &Looks,
    mover: Mover,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;
    if directory(&home.join(INSTALLS), false)?.is_none() {
        return Ok(removal(id, &Outcome::AlreadyGone, None, home));
    }
    let store = Store::open(home, true)?.ok_or_else(|| problem("Missing install store"))?;
    let mut record = match store.load(id) {
        Held::Missing => return Ok(removal(id, &Outcome::AlreadyGone, None, home)),
        Held::Unreadable => return Ok(removal(id, &Outcome::Kept("unreadable"), None, home)),
        Held::Record(record) => *record,
    };
    let removing = Removing {
        home,
        store: &store,
        guard,
        looks,
        mover,
    };
    match removing.recorded(&mut record) {
        Ok(outcome) => Ok(removal(id, &outcome, Some(&record), home)),
        Err(error) if store.changed.get() => Ok(unfinished(id, &record, &error)),
        Err(error) => Err(error),
    }
}

/// One removal: where, through which store, under which guard, seeing the
/// helper's processes through `looks` and moving with `mover`.
struct Removing<'a, G> {
    home: &'a Path,
    store: &'a Store,
    guard: &'a G,
    looks: &'a Looks<'a>,
    mover: Mover<'a>,
}

impl<G: Fn() -> Result<()>> Removing<'_, G> {
    fn destination(&self, record: &Install) -> PathBuf {
        self.home.join(APPLICATIONS).join(&record.name)
    }

    /// Whether the destination holds the recorded bundle's folder.
    fn placed(&self, record: &Install) -> bool {
        fs::symlink_metadata(self.destination(record))
            .ok()
            .zip(record.print.as_ref())
            .is_some_and(|(metadata, print)| metadata.is_dir() && node(&metadata) == print.root)
    }

    fn recorded(&self, record: &mut Install) -> Result<Outcome> {
        match record.phase {
            InstallPhase::Removed => Ok(Outcome::AlreadyGone),
            InstallPhase::Trashed => Ok(match &record.trashed {
                Some(name) => Outcome::Trashed(name.clone()),
                None => Outcome::AlreadyGone,
            }),
            InstallPhase::Kept => Ok(Outcome::Kept(match record.kept.as_deref() {
                Some("replaced") => "replaced",
                Some("other-volume") => "other-volume",
                _ => "changed",
            })),
            InstallPhase::Copying => self.unplaced(record),
            InstallPhase::Placing if !self.placed(record) => self.unplaced(record),
            InstallPhase::Trashing => self.finish_trashing(record),
            InstallPhase::Placing | InstallPhase::Installed => self.in_place(record),
        }
    }

    /// An install never placed: what it staged (Atlas's own copy, made
    /// for this install alone) goes, and it is removed.
    fn unplaced(&self, record: &mut Install) -> Result<Outcome> {
        delete(&self.store.folder(&record.id).join("staging"))?;
        record.phase = InstallPhase::Removed;
        self.store.save(record)?;
        Ok(Outcome::AlreadyGone)
    }

    /// The bundle at its destination: checked to be the copy Atlas made
    /// and in use by nothing, then moved into the helper's Trash.
    fn in_place(&self, record: &mut Install) -> Result<Outcome> {
        let destination = self.destination(record);
        let Some(print) = record.print.clone() else {
            return self.keep(record, "changed");
        };
        let metadata = match fs::symlink_metadata(&destination) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                record.phase = InstallPhase::Removed;
                self.store.save(record)?;
                return Ok(Outcome::AlreadyGone);
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_dir() || node(&metadata) != print.root {
            return self.keep(record, "replaced");
        }
        if !self.same_print(&destination, record, &print) {
            return self.keep(record, "changed");
        }
        let processes = (self.looks.processes)()?;
        if !running_from(
            self.looks,
            &processes,
            &destination,
            record.bundle_id.as_deref(),
        )
        .is_empty()
        {
            return Ok(Outcome::InUse);
        }

        let trashed = trash_name(&record.name, &record.id);
        record.trashed = Some(trashed.clone());
        record.phase = InstallPhase::Trashing;
        self.store.save(record)?;
        (self.guard)()?;
        let trash = self.trash()?;
        let applications = self.applications()?;
        let (name, into) = (file_name(&record.name)?, file_name(&trashed)?);
        match (self.mover)(&applications, &name, &trash, &into) {
            Ok(Moved::Synced) => self.trashed(record, trashed),
            // Moved, but not written through: the record says trashing, so
            // the next removal finds it in the Trash and is done.
            Ok(Moved::Unsynced(error)) => Err(error.into()),
            Err(error) => {
                // Nothing moved: the bundle is where it was.
                record.phase = InstallPhase::Installed;
                record.trashed = None;
                self.store.save(record)?;
                if error.raw_os_error() == Some(libc::EXDEV) {
                    return self.keep(record, "other-volume");
                }
                Err(error.into())
            }
        }
    }

    /// A move into the Trash cut short: done when the Trash holds the
    /// install under its recorded name; never begun when the bundle is
    /// still at its destination, which is then checked again from the
    /// start.
    fn finish_trashing(&self, record: &mut Install) -> Result<Outcome> {
        let Some(trashed) = record.trashed.clone() else {
            record.phase = InstallPhase::Installed;
            self.store.save(record)?;
            return self.in_place(record);
        };
        let in_trash = self.home.join(TRASH).join(&trashed);
        if fs::symlink_metadata(&in_trash).is_ok() {
            return self.trashed(record, trashed);
        }
        record.phase = InstallPhase::Installed;
        record.trashed = None;
        self.store.save(record)?;
        self.in_place(record)
    }

    fn trashed(&self, record: &mut Install, trashed: String) -> Result<Outcome> {
        record.phase = InstallPhase::Trashed;
        record.trashed = Some(trashed.clone());
        self.store.save(record)?;
        Ok(Outcome::Trashed(trashed))
    }

    /// Whether the bundle at `path` prints as `print`; one that cannot be
    /// printed in full is never taken for unchanged.
    fn same_print(&self, path: &Path, record: &Install, print: &BundlePrint) -> bool {
        directory(path, false)
            .ok()
            .flatten()
            .and_then(|folder| {
                print_at(&folder, record.executable.as_deref(), Limits::default()).ok()
            })
            .is_some_and(|now| now == *print)
    }

    fn applications(&self) -> Result<fs::File> {
        let folder = directory(&self.home.join(APPLICATIONS), false)?
            .ok_or_else(|| problem("The helper's Applications disappeared"))?;
        installation_parent(&folder)?;
        Ok(folder)
    }

    /// The helper's Trash, made as macOS makes it when it is missing, and
    /// only ever a folder the helper alone can change.
    fn trash(&self) -> Result<fs::File> {
        let path = self.home.join(TRASH);
        let folder = match directory(&path, false)? {
            Some(folder) => folder,
            None => create_private_directory(&path)?,
        };
        installation_parent(&folder)?;
        Ok(folder)
    }

    /// Keep the install for `reason`, and say so from now on.
    fn keep(&self, record: &mut Install, reason: &'static str) -> Result<Outcome> {
        record.phase = InstallPhase::Kept;
        record.kept = Some(reason.to_owned());
        self.store.save(record)?;
        Ok(Outcome::Kept(reason))
    }
}

/// The reply to taking an install away: what became of the bundle, and,
/// apart from it, which files keyed by its identifier exist now and
/// whether each existed when it was installed. Those are never touched.
fn removal(id: &str, outcome: &Outcome, record: Option<&Install>, home: &Path) -> Value {
    let mut reply = json!({"ok": true, "installId": id});
    match outcome {
        Outcome::Trashed(name) => {
            reply["outcome"] = json!("trashed");
            reply["trashName"] = json!(name);
        }
        Outcome::AlreadyGone => reply["outcome"] = json!("alreadyGone"),
        Outcome::Kept(reason) => {
            reply["outcome"] = json!("kept");
            reply["reason"] = json!(reason);
        }
        Outcome::InUse => reply["outcome"] = json!("inUse"),
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

/// How each of `ids` stands, read only. Where an install is, and what
/// runs from it, is said only while its destination holds the exact folder
/// Atlas placed there; whether it is unchanged is said while printing it
/// fits in [`READ_PRINTS`], else it is unknown. A process running from a
/// translocated copy macOS cannot trace to it is said apart, as one that
/// may be its own.
pub(super) fn installs(
    home: &Path,
    ids: &[String],
    guard: &impl Fn() -> Result<()>,
    looks: &Looks,
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
        (looks.processes)()?
    };
    let until = Instant::now() + READ_PRINTS;

    let listed: Vec<Value> = ids
        .iter()
        .map(|id| {
            let record = match store.as_ref().map(|store| store.load(id)) {
                None | Some(Held::Missing) => {
                    return json!({"installId": id, "phase": "missing"});
                }
                Some(Held::Unreadable) => {
                    return json!({"installId": id, "phase": "unreadable", "verified": false});
                }
                Some(Held::Record(record)) => *record,
            };
            let mut said = json!({
                "installId": id,
                "phase": record.phase,
                "app": record.name.trim_end_matches(".app"),
                "bundleId": record.bundle_id,
                "verified": false,
            });
            let placed_phase = matches!(
                record.phase,
                InstallPhase::Placing | InstallPhase::Installed | InstallPhase::Trashing
            );
            let destination = home.join(APPLICATIONS).join(&record.name);
            let Some(print) = record.print.as_ref().filter(|_| placed_phase) else {
                return said;
            };
            let in_place = fs::symlink_metadata(&destination)
                .is_ok_and(|metadata| metadata.is_dir() && node(&metadata) == print.root);
            if !in_place {
                return said;
            }

            let limits = Limits {
                until: Some(until),
                ..Limits::default()
            };
            let unchanged = match directory(&destination, false) {
                Ok(Some(folder)) => match print_at(&folder, record.executable.as_deref(), limits) {
                    Ok(now) => json!(now == *print),
                    Err(Unprinted::TooSlow) => Value::Null,
                    Err(_) => json!(false),
                },
                _ => json!(false),
            };
            let runs = running_from(looks, &running, &destination, record.bundle_id.as_deref());
            let listed = |which: Runs| -> Vec<Value> {
                runs.iter()
                    .filter(|(.., runs)| *runs == which)
                    .map(|(pid, executable, _)| json!({"pid": pid, "executable": executable}))
                    .collect()
            };
            said["verified"] = json!(true);
            said["path"] = json!(destination);
            said["root"] = json!(print.root);
            said["present"] = json!(true);
            said["unchanged"] = unchanged;
            said["processes"] = json!(listed(Runs::From));
            said["unproven"] = json!(listed(Runs::Maybe));
            said
        })
        .collect();
    Ok(json!({"ok": true, "installs": listed}))
}
