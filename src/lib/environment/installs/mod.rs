//! Apps installed into the helper's `~/Applications` for a task, run inside
//! the helper by the environment worker. An install goes only where
//! nothing is, and is removed only when it is still the exact copy Atlas
//! made: its recorded destination, its folder's node, mode and owner, and
//! a print of every entry inside it. Removal moves it into the install's
//! own private folder first, prints it again there, and deletes it only
//! when it is still that copy; anything else is put back or kept, never
//! deleted. A bundle identifier or a name alone never selects anything to
//! remove, and files the app writes outside its bundle are reported, never
//! removed. Every record change is written before the step it announces,
//! so a worker cut short is finished by the next request.
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
    /// Why it was kept, once it was: `changed` or `replaced`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<String>,
    /// Whether what was kept is held aside in the install's own folder,
    /// as it could not be put back.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub held_aside: bool,
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
    /// Being moved into the install's own folder, where what was moved has
    /// not yet been checked: nothing is deleted in this phase.
    Moving,
    /// Moved and checked to be the copy Atlas made: being deleted.
    Deleting,
    Removed,
    /// Left in place on purpose: it changed, or something else is there.
    Kept,
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
        self.changed.set(true);
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
        executable: None,
        print: None,
        made_folder: false,
        before: BTreeMap::new(),
        kept: None,
        held_aside: false,
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
) -> Result<Placed> {
    let (staging, staging_folder) = store.place(&record.id, "staging")?;
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
    match move_between(&staging_folder, &name, &folder, &name) {
        // Moved: a folder not yet written through changes nothing here.
        Ok(Moved::Synced | Moved::Unsynced(_)) => {}
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
/// removal cut short is finished, every check made again first; a failure
/// after anything changed is a reply to finish, never a refusal.
pub(super) fn remove(
    home: &Path,
    id: &str,
    guard: &impl Fn() -> Result<()>,
    looks: &Looks,
) -> Result<Value> {
    validate_id(id)?;
    guard()?;
    if directory(&home.join(INSTALLS), false)?.is_none() {
        return Ok(removal(id, Outcome::AlreadyGone, None, home));
    }
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
    let store = Store::open(home, true)?.ok_or_else(|| problem("Missing install store"))?;
    let Some(mut record) = store.load(id)? else {
        return Ok(removal(id, Outcome::AlreadyGone, None, home));
    };
    let removing = Removing {
        home,
        store: &store,
        guard,
        looks,
        mover,
    };
    match removing.recorded(&mut record) {
        Ok(outcome) => Ok(removal(id, outcome, Some(&record), home)),
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

    /// Where a removal moves the bundle before deleting it.
    fn moved(&self, record: &Install) -> PathBuf {
        self.store
            .folder(&record.id)
            .join("removing")
            .join(&record.name)
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
            InstallPhase::Kept => Ok(Outcome::Kept(match record.kept.as_deref() {
                Some("replaced") => "replaced",
                _ => "changed",
            })),
            InstallPhase::Copying => self.unplaced(record),
            InstallPhase::Placing if !self.placed(record) => self.unplaced(record),
            InstallPhase::Moving => self.check_moved(record),
            InstallPhase::Deleting => self.finish_deleting(record),
            InstallPhase::Placing | InstallPhase::Installed => self.in_place(record),
        }
    }

    /// An install never placed: what it staged goes, and it is removed.
    fn unplaced(&self, record: &mut Install) -> Result<Outcome> {
        delete(&self.store.folder(&record.id).join("staging"))?;
        record.phase = InstallPhase::Removed;
        self.store.save(record)?;
        Ok(Outcome::AlreadyGone)
    }

    /// The bundle at its destination: checked to be the copy Atlas made
    /// and in use by nothing, then moved into the install's own folder and
    /// checked again there.
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

        record.phase = InstallPhase::Moving;
        self.store.save(record)?;
        (self.guard)()?;
        let (_, removing) = self.store.place(&record.id, "removing")?;
        let applications = self.applications()?;
        let name = file_name(&record.name)?;
        match (self.mover)(&applications, &name, &removing, &name) {
            Ok(Moved::Synced) => self.check_moved(record),
            // Moved, but not written through: nothing is deleted on a move
            // a power cut could undo. The record says moving, so the next
            // removal checks what was moved before it deletes anything.
            Ok(Moved::Unsynced(error)) => Err(error.into()),
            Err(error) => {
                // Nothing moved: the bundle is where it was.
                record.phase = InstallPhase::Installed;
                self.store.save(record)?;
                Err(error.into())
            }
        }
    }

    /// What the removal moved aside, not yet checked: deleted only when it
    /// is still the copy Atlas made, else put back, or held aside when it
    /// cannot be. With nothing moved aside, the bundle is looked for where
    /// it was.
    fn check_moved(&self, record: &mut Install) -> Result<Outcome> {
        let moved = self.moved(record);
        let Some(print) = record.print.clone() else {
            return self.keep(record, "changed");
        };
        let folder = match fs::symlink_metadata(&moved) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // The move never happened.
                record.phase = InstallPhase::Installed;
                self.store.save(record)?;
                return if self.placed(record) {
                    self.in_place(record)
                } else {
                    record.phase = InstallPhase::Removed;
                    self.store.save(record)?;
                    Ok(Outcome::AlreadyGone)
                };
            }
            Err(error) => return Err(error.into()),
            Ok(_) => directory(&moved, false)?,
        };
        let printed = folder
            .as_ref()
            .map(|folder| print_at(folder, record.executable.as_deref(), Limits::default()));
        let same_node = folder
            .as_ref()
            .and_then(|folder| folder.metadata().ok())
            .is_some_and(|metadata| node(&metadata) == print.root);

        if same_node && matches!(&printed, Some(Ok(now)) if *now == print) {
            record.phase = InstallPhase::Deleting;
            self.store.save(record)?;
            return self.finish_deleting(record);
        }

        let reason = if same_node { "changed" } else { "replaced" };
        self.put_back(record, reason)
    }

    /// Put what was moved aside back where it was, never over anything,
    /// and keep it for `reason`; held aside when it cannot go back.
    fn put_back(&self, record: &mut Install, reason: &'static str) -> Result<Outcome> {
        let (_, removing) = self.store.place(&record.id, "removing")?;
        let applications = self.applications();
        let name = file_name(&record.name)?;
        let back = applications.ok().is_some_and(|applications| {
            (self.mover)(&removing, &name, &applications, &name).is_ok()
        });
        record.held_aside = !back;
        self.keep(record, reason)
    }

    /// Delete what was checked to be the copy Atlas made, in the install's
    /// own folder. A cut short deletion goes on only while what is there
    /// is still that folder.
    fn finish_deleting(&self, record: &mut Install) -> Result<Outcome> {
        let moved = self.moved(record);
        match fs::symlink_metadata(&moved) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(metadata) => {
                let ours = record
                    .print
                    .as_ref()
                    .is_some_and(|print| metadata.is_dir() && node(&metadata) == print.root);
                if !ours {
                    record.held_aside = true;
                    return self.keep(record, "replaced");
                }
                delete(&moved)?;
            }
        }
        record.phase = InstallPhase::Removed;
        self.store.save(record)?;
        let _ = fs::remove_dir(self.store.folder(&record.id).join("removing"));
        Ok(Outcome::Removed)
    }

    /// Whether the bundle at `path` prints as `print`; one that cannot be
    /// printed is never taken for unchanged.
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

    /// Keep the install for `reason`, and say so from now on.
    fn keep(&self, record: &mut Install, reason: &'static str) -> Result<Outcome> {
        record.phase = InstallPhase::Kept;
        record.kept = Some(reason.to_owned());
        self.store.save(record)?;
        Ok(Outcome::Kept(reason))
    }
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
        if record.held_aside {
            reply["heldAside"] = json!(true);
        }
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
            let record = store
                .as_ref()
                .and_then(|store| store.load(id).ok().flatten());
            let Some(record) = record else {
                return json!({"installId": id, "phase": "missing"});
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
                InstallPhase::Placing | InstallPhase::Installed | InstallPhase::Moving
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
