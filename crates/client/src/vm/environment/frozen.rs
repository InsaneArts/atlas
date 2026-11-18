//! A VM's plans frozen on this Mac, each in a folder of its own in the
//! VM's records, `environment/plan-<id>/`: its lock, the payloads copied
//! from the files it selects, the plan the guest made of them, the last
//! result, the guest's copy of the payloads, a marker for each of its
//! changes whose outcome is unknown, and this Mac's own record of an apply
//! that may have been carried out, kept before it is said and marked once
//! the guest acknowledged it.
//!
//! The records folder is Atlas's own, inside the very folder no file an
//! agent names may be in: everything here is kept as Atlas keeps every
//! record of its own (private folders, private files written whole and
//! renamed into place, read only when they are plain files of this
//! account's), never through the guard for files an agent names, which
//! refuses that folder by design.
use crate::environment::{
    MAX_RECORD,
    list::Frozen as Listed,
    model::{Plan, decode, file_report, validate_id},
    problem,
    reply::ShownFile,
};
use atlas_core::{
    error::{Code, Error, Result},
    files::{self, Lock, LockFile, Token},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// The folder of the plans, in a VM's records folder.
const FOLDER: &str = "environment";
/// What a plan's folder is named with, before its ID.
const PREFIX: &str = "plan-";
/// What a folder being removed is first renamed to, so no command ever sees
/// it half removed.
const DISCARDED: &str = ".discarded-";
const LOCK: &str = "controller.lock";
const PLAN: &str = "plan.json";
const RESULT: &str = "last-result.json";
const PAYLOADS: &str = "payloads";
const COPY: &str = "copy.json";
const APPLIED: &str = "applied.json";
/// What a marker of a change whose outcome is unknown is named with, before
/// the change.
const MARKER: &str = "request-";
/// How long a plan's lock that looks held is tried again.
const PATIENCE: Duration = Duration::from_millis(250);
/// The longest copy record read.
const COPY_RECORD: u64 = 4096;

/// The folder of a frozen plan's payloads on this Mac, made only here: what
/// [`crate::vm::copies::stage_frozen`] stages, and nothing an agent names.
pub(in crate::vm) struct Payloads {
    folder: PathBuf,
    plan: Token,
}

impl Payloads {
    /// Where the payloads are.
    pub(in crate::vm) fn folder(&self) -> &Path {
        &self.folder
    }

    /// The plan they are frozen for.
    pub(in crate::vm) fn plan(&self) -> &Token {
        &self.plan
    }
}

/// What a plan's record of its guest copy says.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Copy {
    /// It keeps none.
    None,
    /// The guest's copy of its payloads, staged as this change.
    Kept(Token),
    /// A record there that cannot be read.
    Unreadable,
}

/// One plan's folder.
pub(super) struct Frozen {
    folder: PathBuf,
    id: Token,
}

impl Frozen {
    /// Plan `id`'s folder in the VM's records folder `records`.
    pub(super) fn of(records: &Path, id: &str) -> Result<Self> {
        validate_id(id)?;
        let id = Token::parse(id).ok_or_else(|| problem("Invalid plan ID"))?;
        Ok(Self {
            folder: root(records).join(format!("{PREFIX}{}", id.as_str())),
            id,
        })
    }

    /// Every plan folder in `records`: real folders of this account's,
    /// named with a valid ID. Anything else there is left alone.
    pub(super) fn all(records: &Path) -> Vec<Self> {
        let Ok(entries) = fs::read_dir(root(records)) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| files::own_folder(&entry.path()))
            .filter_map(|entry| {
                let name = entry.file_name();
                let id = name.to_str()?.strip_prefix(PREFIX)?;
                Self::of(records, id).ok()
            })
            .collect()
    }

    pub(super) fn id(&self) -> &Token {
        &self.id
    }

    pub(super) fn folder(&self) -> &Path {
        &self.folder
    }

    /// Whether the folder is there, a real folder of this account's.
    pub(super) fn exists(&self) -> bool {
        files::own_folder(&self.folder)
    }

    /// Make the folder, private, and hold its lock.
    pub(super) fn create(&self) -> Result<LockFile> {
        let parent = self
            .folder
            .parent()
            .ok_or_else(|| problem("Missing plans folder"))?;
        files::private_folder(parent, Code::State)?;
        files::private_folder(&self.folder, Code::State)?;
        self.lock()
    }

    /// The folder's lock, held by this command alone, or a refusal when
    /// another command holds it past a moment.
    pub(super) fn lock(&self) -> Result<LockFile> {
        self.locked(PATIENCE)
    }

    /// [`Frozen::lock`], tried once: for a sweep, which skips a plan in use.
    pub(super) fn try_lock(&self) -> Result<LockFile> {
        self.locked(Duration::ZERO)
    }

    fn locked(&self, patience: Duration) -> Result<LockFile> {
        if !self.exists() {
            return Err(problem("Frozen plan is missing"));
        }
        files::private_folder(&self.folder, Code::State)?;
        let lock = files::open_lock(&self.folder.join(LOCK))?;
        files::lock_within(&lock, Lock::Exclusive, patience).map_err(|files::Held(cause)| {
            problem("This environment plan is already running").with_source(cause)
        })?;
        Ok(lock)
    }

    /// The folder locked, made first when it is not there, and whether it
    /// was made.
    pub(super) fn lock_or_create(&self) -> Result<(LockFile, bool)> {
        if self.exists() {
            return Ok((self.lock()?, false));
        }
        Ok((self.create()?, true))
    }

    /// Whether a command holds the folder's lock right now. It is looked at
    /// with a shared lock let go of at once.
    pub(super) fn held(&self) -> bool {
        files::open_plain(&self.folder.join(LOCK))
            .is_some_and(|lock| fs2::FileExt::try_lock_shared(&lock).is_err())
    }

    /// Keep payload `index`'s bytes.
    pub(super) fn keep_payload(&self, index: usize, bytes: &[u8]) -> Result<()> {
        let payloads = self.folder.join(PAYLOADS);
        files::private_folder(&payloads, Code::State)?;
        files::write_atomic(&payloads.join(payload(index)), bytes, 0o600)
    }

    /// The folder of the payloads, to stage.
    pub(super) fn payloads(&self) -> Payloads {
        Payloads {
            folder: self.folder.join(PAYLOADS),
            plan: self.id.clone(),
        }
    }

    /// Whether it keeps its payloads, or may: one that cannot be looked for
    /// counts as kept.
    pub(super) fn has_payloads(&self) -> bool {
        self.folder.join(PAYLOADS).try_exists().unwrap_or(true)
    }

    pub(super) fn keep_plan(&self, plan: &Plan) -> Result<()> {
        files::write_atomic(
            &self.folder.join(PLAN),
            &serde_json::to_vec_pretty(plan)?,
            0o600,
        )
    }

    /// The plan frozen here, when one reads and it is this folder's.
    pub(super) fn plan(&self) -> Option<Plan> {
        files::read_own(&self.folder.join(PLAN), MAX_RECORD)
            .and_then(|bytes| decode::<Plan>(&bytes).ok())
            .filter(|plan| plan.id == self.id.as_str())
    }

    /// Whether it holds a frozen plan, or may: one that cannot be looked
    /// for counts as there, so it is never removed for want of a plan.
    pub(super) fn planned(&self) -> bool {
        self.folder.join(PLAN).try_exists().unwrap_or(true)
    }

    pub(super) fn keep_result(&self, result: &serde_json::Value) -> Result<()> {
        files::write_atomic(
            &self.folder.join(RESULT),
            &serde_json::to_vec_pretty(result)?,
            0o600,
        )
    }

    /// Keep `copy` as the change its payloads are staged into the guest as,
    /// before anything of it is said.
    pub(super) fn keep_copy(&self, copy: &Token) -> Result<()> {
        files::write_atomic(&self.folder.join(COPY), &serde_json::to_vec(copy)?, 0o600)
    }

    /// What its record of its guest copy says.
    pub(super) fn copy(&self) -> Copy {
        let path = self.folder.join(COPY);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Copy::None,
            Err(_) => return Copy::Unreadable,
            Ok(_) => {}
        }
        files::read_own(&path, COPY_RECORD)
            .and_then(|bytes| serde_json::from_slice::<Token>(&bytes).ok())
            .map_or(Copy::Unreadable, Copy::Kept)
    }

    /// Forget its guest copy, once it is known gone, and its payloads.
    pub(super) fn forget_copy(&self) -> Result<()> {
        let payloads = self.folder.join(PAYLOADS);
        match fs::remove_dir_all(&payloads) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::writing(&payloads, error));
            }
            Ok(()) | Err(_) => {}
        }
        remove(&self.folder.join(COPY))
    }

    /// Keep this Mac's own record that an apply of the plan may have been
    /// carried out, before its change is claimed or said; `acknowledged`
    /// once the guest answered it applied part or all of it.
    pub(super) fn keep_applied(&self, acknowledged: bool) -> Result<()> {
        let record = serde_json::to_vec(&serde_json::json!({"version": 1,
            "acknowledged": acknowledged}))?;
        files::write_atomic(&self.folder.join(APPLIED), &record, 0o600)
    }

    /// Forget the record that an apply may have been carried out: only on a
    /// certain answer that it changed nothing.
    pub(super) fn forget_applied(&self) -> Result<()> {
        remove(&self.folder.join(APPLIED))
    }

    /// Whether an apply of the plan may have been carried out, as this Mac
    /// keeps it, acknowledged or not: a record that cannot be looked for
    /// counts.
    pub(super) fn applied(&self) -> bool {
        self.folder.join(APPLIED).try_exists().unwrap_or(true)
    }

    /// Mark `change` as one of the plan's whose outcome is unknown, before
    /// it is claimed or said.
    pub(super) fn mark(&self, change: &Token) -> Result<()> {
        files::write_atomic(&self.folder.join(marker(change)), b"", 0o600)
    }

    /// Drop the marker of `change`, once its outcome is known.
    pub(super) fn unmark(&self, change: &Token) {
        let _ = remove(&self.folder.join(marker(change)));
    }

    /// The changes it marks; none when the folder cannot be read, which
    /// may mark any.
    pub(super) fn markers(&self) -> Option<Vec<Token>> {
        let mut changes = Vec::new();
        for entry in fs::read_dir(&self.folder).ok()? {
            let name = entry.ok()?.file_name();
            let Some(change) = name.to_str().and_then(|name| name.strip_prefix(MARKER)) else {
                continue;
            };
            changes.push(Token::parse(change)?);
        }
        Some(changes)
    }

    /// Whether it marks a change whose outcome is unknown, or may.
    pub(super) fn marked(&self) -> bool {
        self.markers().is_none_or(|changes| !changes.is_empty())
    }

    /// How long since the folder last changed; none when that cannot be
    /// told.
    pub(super) fn age(&self) -> Option<Duration> {
        fs::symlink_metadata(&self.folder)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()
    }

    /// The plan as `environment list` reports one the guest did not list:
    /// its files, whether its outcome is unknown here (a change it marks,
    /// or an apply that may have been carried out), whether a command holds it,
    /// and when it last changed. None for a folder with none of these.
    pub(super) fn listed(&self) -> Option<Listed> {
        let files: Option<Vec<ShownFile>> = self
            .plan()
            .map(|plan| file_report(&plan).iter().map(ShownFile::from).collect());
        let unknown = self.marked() || self.applied();
        if files.is_none() && !unknown {
            return None;
        }
        Some(Listed::new(
            self.id.as_str().to_owned(),
            files,
            unknown,
            self.held(),
            crate::environment::files::changed_ms(&self.folder).unwrap_or(0),
        ))
    }

    /// Remove the folder, called under its lock: it is moved to a discarded
    /// name first, so its own name is gone at once. Whether it is gone.
    pub(super) fn discard(&self) -> bool {
        let discarded = self
            .folder
            .with_file_name(format!("{DISCARDED}{}", files::token()));
        if fs::rename(&self.folder, &discarded).is_ok() {
            let _ = fs::remove_dir_all(discarded);
        } else {
            let _ = fs::remove_dir_all(&self.folder);
        }
        fs::symlink_metadata(&self.folder)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    }
}

/// Remove what a discard cut short left under a discarded name in `records`:
/// Atlas's own real folders, which no command uses once renamed.
pub(super) fn clear_discarded(records: &Path) {
    let Ok(entries) = fs::read_dir(root(records)) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(DISCARDED)
            && files::own_folder(&entry.path())
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// The plans' folder in `records`.
fn root(records: &Path) -> PathBuf {
    records.join(FOLDER)
}

/// Payload `index`'s name, in the payloads folder here and in the guest's
/// copy of it alike.
pub(super) fn payload(index: usize) -> String {
    format!("payload-{index}")
}

fn marker(change: &Token) -> String {
    format!("{MARKER}{}", change.as_str())
}

/// Remove the file at `path`; one already gone is removed.
fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::writing(path, error))
        }
        Ok(()) | Err(_) => Ok(()),
    }
}
