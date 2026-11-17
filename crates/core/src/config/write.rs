//! The one writer of the settings file. Every change, whoever makes it, is
//! made here under one lock, the settings file's own: the file is read again
//! under it, one change is applied to what it holds then, the result is
//! checked to load, and the file is replaced in one rename through its
//! folder's own descriptor, only while it is still the file read. A change
//! keeps every byte it does not touch where the text form allows. With no
//! file, one is made only where none appeared meanwhile.
//!
//! The settings lock is held for one write alone, and nothing is waited for
//! under it, so a caller may make a write while holding a later lock in the
//! station's order.
use super::{LIMIT, NOT_TOML, Settings, desktops::SECOND_LOGIN, tilde, unparsed};
use crate::{
    desktop::DesktopId,
    error::{Code, Error, Result},
};
use std::{
    ffi::OsString,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How long a settings write waits for another to finish.
const WRITING: Duration = Duration::from_secs(5);

/// What a settings file made by Atlas's own writes is: only this account's.
const MODE: u32 = 0o600;

/// The name a converted file's old form is kept under, beside it.
const BEFORE_VM: &str = "before-vm";

/// What a change makes of the settings text: the whole new text, and
/// whether the file it replaces is to be kept beside it first, as an old
/// single-helper file is when it becomes a list.
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    text: String,
    keep_before: bool,
    /// The mode the file gets; none keeps the one it has.
    mode: Option<u32>,
}

impl Edit {
    /// The settings as `text`.
    #[must_use]
    pub fn text(text: String) -> Self {
        Self {
            text,
            keep_before: false,
            mode: None,
        }
    }

    /// The same, the file made readable by this account alone whatever
    /// its mode was.
    #[must_use]
    pub fn private(self) -> Self {
        Self {
            mode: Some(MODE),
            ..self
        }
    }
}

/// Whether a write changed the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written {
    /// It holds what it held.
    Unchanged,
    /// It was made or replaced.
    Changed,
}

/// Make one change to the settings file at `path`, as the one writer does:
/// `change` is given the text the file holds now, read under the settings
/// lock, or none when there is no file, and answers the whole new text, or
/// none to leave it as it is. It may be asked again when the file changed
/// or appeared meanwhile, and is never asked while another write is made.
///
/// # Errors
///
/// Returns `change`'s refusal, a result that does not load or is too large,
/// the failure to read, lock or write the file, and a refusal when it
/// changed while it was being updated.
pub fn update(
    path: &Path,
    mut change: impl FnMut(Option<&str>) -> Result<Option<Edit>>,
) -> Result<Written> {
    let deadline = Instant::now() + WRITING;
    loop {
        if absent(path)? {
            let Some(edit) = change(None)? else {
                return Ok(Written::Unchanged);
            };
            if made(path, &edit)? {
                return Ok(Written::Changed);
            }
            // Another writer made it meanwhile: its file is changed instead.
            continue;
        }
        let held = locked(path, deadline)?;
        let text = std::str::from_utf8(&held.before)
            .map_err(|_| invalid(&held.resolved, &"it is not UTF-8 text"))?;
        let Some(edit) = change(Some(text))? else {
            return Ok(Written::Unchanged);
        };
        if edit.text.as_bytes() == held.before {
            return Ok(Written::Unchanged);
        }
        loads(&edit.text, &held.resolved)?;
        if edit.keep_before {
            held.keep_before()?;
        }
        // Still the very file locked and read, unchanged: then it is
        // replaced through its folder's own descriptor.
        if held.folder.entry_identity(&held.name)? != Some(held.identity)
            || held_bytes(&held.file)? != held.before
        {
            return Err(Error::new(
                Code::Config,
                format!(
                    "{} changed while Atlas was updating it; try again",
                    tilde(&held.resolved)
                ),
            ));
        }
        held.folder.replace(
            &held.name,
            edit.text.as_bytes(),
            edit.mode.unwrap_or(held.mode),
        )?;
        return Ok(Written::Changed);
    }
}

/// A change to the desktops the settings list, as the station makes it for
/// a VM it adds or removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// List the VM `id` as `name`. With no settings file, they list it
    /// alone, as the default; settings that name one helper become a list
    /// of their Second Login, kept as it was, the default still, and the VM.
    AddVm { name: String, id: DesktopId },
    /// Take the VM `id` off the list, found by its identifier whatever it
    /// is named now, as `disposition` says. A VM the list no longer holds
    /// is already off it. The VMs `going`, whose own removals are under
    /// way, are never handed the default and never count as left.
    RemoveVm {
        id: DesktopId,
        disposition: Disposition,
        going: Vec<DesktopId>,
    },
}

/// What a change made of the settings file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub written: Written,
    /// The desktop it made the default, when it moved the default.
    pub default: Option<String>,
}

/// What becomes of the settings when a VM is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    /// A desktop removed on its own: its entry goes, the default moves to
    /// Second Login when listed, else to the first desktop left, and the
    /// last desktop is never removed this way.
    Desktop,
    /// A desktop removed by uninstall: the settings stay exactly as they
    /// are, whatever they list, until uninstall's shared cleanup decides
    /// what becomes of the whole file.
    Uninstall,
}

impl Change {
    /// The settings the file at `path` would hold after this change, as
    /// they are now: what is checked before anything else is touched.
    /// Nothing is written.
    ///
    /// # Errors
    ///
    /// As [`Self::apply`], short of writing.
    pub fn check(&self, path: &Path) -> Result<Settings> {
        let current = if absent(path)? {
            None
        } else {
            Some(read_text(path)?)
        };
        let text = match self.edit(current.as_deref(), path)? {
            Some((edit, _)) => edit.text,
            None => current.ok_or_else(|| super::missing(path))?,
        };
        loads(&text, path)
    }

    /// Make this change to the settings file at `path`, through
    /// [`update`].
    ///
    /// # Errors
    ///
    /// Refuses a name or a VM the settings already list, removing the last
    /// desktop, and an old form's kept copy that holds something else; and
    /// fails as [`update`] does.
    pub fn apply(&self, path: &Path) -> Result<Written> {
        self.applied(path).map(|applied| applied.written)
    }

    /// [`Self::apply`], with the default it committed when it moved it.
    ///
    /// # Errors
    ///
    /// As [`Self::apply`].
    pub fn applied(&self, path: &Path) -> Result<Applied> {
        let mut default = None;
        let written = update(path, |current| {
            let edit = self.edit(current, path)?;
            default = edit.as_ref().and_then(|(_, moved)| moved.clone());
            Ok(edit.map(|(edit, _)| edit))
        })?;
        Ok(Applied {
            written,
            default: default.filter(|_| written == Written::Changed),
        })
    }

    /// The text `current`, the file at `path`'s, with this change made, and
    /// the desktop it makes the default when it moves the default.
    fn edit(&self, current: Option<&str>, path: &Path) -> Result<Option<(Edit, Option<String>)>> {
        match self {
            Self::AddVm { name, id } => {
                added(current, path, name, id).map(|edit| Some((edit, None)))
            }
            Self::RemoveVm {
                id,
                disposition,
                going,
            } => match (current, disposition) {
                (_, Disposition::Uninstall) | (None, Disposition::Desktop) => Ok(None),
                (Some(text), Disposition::Desktop) => removed(text, path, id, going),
            },
        }
    }
}

/// The settings `current` of the file at `path` with the VM `id` listed as
/// `name`.
fn added(current: Option<&str>, path: &Path, name: &str, id: &DesktopId) -> Result<Edit> {
    let Some(text) = current else {
        let runs = super::runs_folder()?;
        let mut document = toml_edit::DocumentMut::new();
        document.insert("default", toml_edit::value(name));
        document.insert(
            "output_dir",
            toml_edit::value(runs.to_string_lossy().as_ref()),
        );
        let mut desktops = toml_edit::ArrayOfTables::new();
        desktops.push(vm_entry(name, id));
        document.insert("desktops", toml_edit::Item::ArrayOfTables(desktops));
        return Ok(Edit::text(document.to_string()));
    };
    let settings = Settings::parse(text.as_bytes(), path)?;
    if settings.desktops().iter().any(|listed| listed.name == name) {
        return Err(Error::new(
            Code::Config,
            format!("The settings already list a desktop named \"{name}\""),
        ));
    }
    let mut document = parsed(text, path)?;
    let keep_before = !document.contains_key("desktops");
    if keep_before {
        listed_form(&mut document);
    }
    match document.get_mut("desktops") {
        Some(toml_edit::Item::ArrayOfTables(tables)) => tables.push(vm_entry(name, id)),
        Some(toml_edit::Item::Value(toml_edit::Value::Array(array))) => {
            array.push(vm_entry(name, id).into_inline_table());
        }
        _ => return Err(invalid(path, &"desktops is not a list")),
    }
    Ok(Edit {
        keep_before,
        ..Edit::text(document.to_string())
    })
}

/// The settings `text` of the file at `path` with the VM `id` off the list,
/// found by its identifier, and the desktop made the default when the VM
/// was it, never one of `going`; none when the list does not hold it.
fn removed(
    text: &str,
    path: &Path,
    id: &DesktopId,
    going: &[DesktopId],
) -> Result<Option<(Edit, Option<String>)>> {
    let settings = Settings::parse(text.as_bytes(), path)?;
    let is = |listed: &super::desktops::Listed, vm: &DesktopId| matches!(&listed.desktop, super::desktops::Chosen::Vm(config) if config.id == *vm);
    let Some(this) = settings.desktops().iter().find(|listed| is(listed, id)) else {
        return Ok(None);
    };
    let name = this.name.clone();
    let others: Vec<_> = settings
        .desktops()
        .iter()
        .filter(|listed| !is(listed, id))
        .collect();
    let left: Vec<_> = others
        .iter()
        .filter(|listed| !going.iter().any(|vm| is(listed, vm)))
        .collect();
    let Some(first) = left.first() else {
        let error = if others.is_empty() {
            format!(
                "{name} is the only desktop the settings list. To remove it with the rest of \
                 Atlas, run atlas uninstall."
            )
        } else {
            let names: Vec<&str> = others.iter().map(|listed| listed.name.as_str()).collect();
            format!(
                "{name} is the last desktop the settings list that is not being removed ({} \
                 being removed). To remove it with the rest of Atlas, run atlas uninstall.",
                names.join(", ")
            )
        };
        return Err(Error::new(Code::Config, error));
    };
    let default = (settings.default_desktop().name == name).then(|| {
        left.iter()
            .find(|listed| listed.desktop.user().is_some())
            .unwrap_or(first)
            .name
            .clone()
    });
    let mut document = parsed(text, path)?;
    let named = |entry: &dyn toml_edit::TableLike| {
        entry.get("kind").and_then(toml_edit::Item::as_str) == Some("vm")
            && entry.get("id").and_then(toml_edit::Item::as_str) == Some(id.as_str())
    };
    match document.get_mut("desktops") {
        Some(toml_edit::Item::ArrayOfTables(tables)) => {
            let at = tables.iter().position(|entry| named(entry));
            if let Some(at) = at {
                tables.remove(at);
            }
        }
        Some(toml_edit::Item::Value(toml_edit::Value::Array(array))) => {
            let at = array
                .iter()
                .position(|entry| entry.as_inline_table().is_some_and(|entry| named(entry)));
            if let Some(at) = at {
                array.remove(at);
            }
        }
        _ => return Err(invalid(path, &"desktops is not a list")),
    }
    if let Some(default) = &default {
        super::desktops::put(document.as_table_mut(), "default", default.as_str().into());
    }
    Ok(Some((Edit::text(document.to_string()), default)))
}

/// The list entry of the VM `id`, named `name`.
fn vm_entry(name: &str, id: &DesktopId) -> toml_edit::Table {
    let mut entry = toml_edit::Table::new();
    entry.insert("name", toml_edit::value(name));
    entry.insert("kind", toml_edit::value("vm"));
    entry.insert("id", toml_edit::value(id.as_str()));
    entry
}

/// Settings that name one helper, made a list of that helper as Second
/// Login, the default. Its fields move to its entry as they were written:
/// their keys, spacing, values and trailing comments, in the file's order,
/// each with the comment lines above it. The comment lines above the file's
/// first setting are the file's own and stay at the top, above what is
/// first there now; every other line stays where it was.
fn listed_form(document: &mut toml_edit::DocumentMut) {
    let root = document.as_table_mut();
    let fields = super::desktops::USER_FIELDS;
    let keys: Vec<String> = root
        .iter()
        .filter(|(_, item)| item.is_value())
        .map(|(key, _)| key.to_owned())
        .collect();
    let mut header = String::new();
    if let Some(first) = keys
        .first()
        .filter(|first| fields.contains(&first.as_str()))
        && let Some(mut key) = root.key_mut(first)
    {
        header = prefix_of(key.leaf_decor());
        key.leaf_decor_mut().set_prefix("");
    }
    let mut entry = toml_edit::Table::new();
    entry.insert("name", toml_edit::value(SECOND_LOGIN));
    entry.insert("kind", toml_edit::value("user"));
    for field in keys.iter().filter(|key| fields.contains(&key.as_str())) {
        if let Some((key, item)) = root.remove_entry(field) {
            entry.insert_formatted(&key, item);
        }
    }
    root.insert("default", toml_edit::value(SECOND_LOGIN));
    if !header.is_empty() {
        let first = root
            .iter()
            .find(|(_, item)| item.is_value())
            .map(|(key, _)| key.to_owned());
        if let Some(mut key) = first.as_deref().and_then(|first| root.key_mut(first)) {
            let own = prefix_of(key.leaf_decor());
            key.leaf_decor_mut().set_prefix(header + &own);
        }
    }
    let mut desktops = toml_edit::ArrayOfTables::new();
    desktops.push(entry);
    root.insert("desktops", toml_edit::Item::ArrayOfTables(desktops));
}

/// The text written before a key: its comment lines and blank lines.
fn prefix_of(decor: &toml_edit::Decor) -> String {
    decor
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// `text`, the settings of the file at `path`, as an editable document.
fn parsed(text: &str, path: &Path) -> Result<toml_edit::DocumentMut> {
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| unparsed(&tilde(path), text, error.span(), NOT_TOML))
}

/// The settings `text` of the file at `path`, when they load and fit.
fn loads(text: &str, path: &Path) -> Result<Settings> {
    if text.len() as u64 > LIMIT {
        return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
    }
    Settings::parse(text.as_bytes(), path)
}

fn invalid(path: &Path, error: &dyn std::fmt::Display) -> Error {
    Error::new(Code::Config, format!("{}: {error}", tilde(path)))
}

/// Whether nothing is at `path`, not even a link.
fn absent(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(Error::writing(path, error)),
    }
}

/// The settings text at `path`, read through the guarded reader.
fn read_text(path: &Path) -> Result<String> {
    let bytes = crate::checked::read(path)?.read_up_to(LIMIT + 1)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
    }
    String::from_utf8(bytes).map_err(|_| invalid(path, &"it is not UTF-8 text"))
}

/// Make the settings file at `path` as `edit` says, its folder made private
/// when missing: whether it was made, false when one appeared meanwhile.
fn made(path: &Path, edit: &Edit) -> Result<bool> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| Error::new(Code::Config, "Settings name no file"))?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|error| Error::writing(parent, error))?;
    let folder = crate::checked::folder(parent)?;
    loads(&edit.text, &folder.path().join(name))?;
    folder.create_new(name, edit.text.as_bytes(), MODE)
}

/// The settings file at `path` as one settings write holds it: its folder,
/// proven unchangeable by any other account; its name there; the file opened
/// through the guarded reader and locked for this writer alone; which file
/// that is; and its bytes, read through that descriptor.
struct Locked {
    folder: crate::checked::Folder,
    name: OsString,
    file: crate::files::LockFile,
    identity: crate::checked::Identity,
    before: Vec<u8>,
    mode: u32,
    resolved: PathBuf,
}

impl Locked {
    /// Keep the file as it was beside it, as `<name>.before-vm`, made only
    /// where there is none: one already there that holds these very bytes
    /// is the same copy, and one that holds anything else is never
    /// overwritten.
    fn keep_before(&self) -> Result<()> {
        let mut name = self.name.clone();
        name.push(".");
        name.push(BEFORE_VM);
        if self.folder.create_new(&name, &self.before, self.mode)? {
            return Ok(());
        }
        let kept = self.folder.path().join(&name);
        let same = crate::checked::read(&kept)
            .and_then(|reading| reading.read_up_to(LIMIT + 1))
            .is_ok_and(|bytes| bytes == self.before);
        if same {
            return Ok(());
        }
        Err(Error::new(
            Code::Config,
            format!(
                "{} holds other settings, and Atlas keeps the old ones there before it lists a \
                 VM. Move it aside, then try again.",
                tilde(&kept)
            ),
        ))
    }
}

/// The settings file at `path`, locked for this writer alone, so two
/// settings writes never lose one another's change: the lock is on the file
/// opened through the guarded reader, taken again when another writer
/// replaced it meanwhile. Nothing is read or written by its name again.
fn locked(path: &Path, deadline: Instant) -> Result<Locked> {
    loop {
        let reading = crate::checked::read(path)?;
        let resolved = reading.path().to_path_buf();
        let identity = reading.identity()?;
        let file = crate::files::LockFile(reading.into_file());
        crate::files::lock_retrying(&file, crate::files::Lock::Exclusive, deadline, |deadline| {
            Instant::now() < deadline
        })
        .map_err(|_| {
            Error::new(
                Code::Config,
                format!("{} is being updated; try again", tilde(&resolved)),
            )
        })?;
        let parent = resolved.parent().unwrap_or_else(|| Path::new("/"));
        let name = resolved
            .file_name()
            .ok_or_else(|| Error::new(Code::Config, "Settings name no file"))?
            .to_os_string();
        let folder = crate::checked::folder(parent)?;
        if folder.entry_identity(&name)? != Some(identity) {
            continue;
        }
        let before = held_bytes(&file)?;
        let mode = file.0.metadata()?.permissions().mode() & 0o777;
        return Ok(Locked {
            folder,
            name,
            file,
            identity,
            before,
            mode,
            resolved,
        });
    }
}

/// The locked settings file's bytes, read through its descriptor from the
/// start, refused past the limit.
fn held_bytes(file: &crate::files::LockFile) -> Result<Vec<u8>> {
    use std::io::{Seek, SeekFrom};

    let mut reader = file.0.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let bytes = crate::files::read_up_to(reader, LIMIT + 1)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
    }
    Ok(bytes)
}

/// Set `key` of `[activity]` to `value`, or leave it out with none, in the
/// settings at `path`, changing nothing else in the file.
pub(super) fn activity(path: &Path, key: &str, value: Option<toml_edit::Value>) -> Result<()> {
    update(path, |current| {
        // The activity settings change a file that is there.
        let text = current.ok_or_else(|| super::missing(path))?;
        let mut document = parsed(text, path)?;
        let not_a_table = || invalid(path, &"activity is not a table");
        match value.clone() {
            Some(mut value) => {
                let activity = document
                    .entry("activity")
                    .or_insert_with(toml_edit::table)
                    .as_table_like_mut()
                    .ok_or_else(not_a_table)?;
                match activity
                    .get_mut(key)
                    .and_then(toml_edit::Item::as_value_mut)
                {
                    Some(written) => {
                        *value.decor_mut() = written.decor().clone();
                        *written = value;
                    }
                    None => {
                        activity.insert(key, toml_edit::Item::Value(value));
                    }
                }
            }
            None => {
                if let Some(activity) = document.get_mut("activity") {
                    let activity = activity.as_table_like_mut().ok_or_else(not_a_table)?;
                    activity.remove(key);
                    if activity.is_empty() {
                        document.remove("activity");
                    }
                }
            }
        }
        Ok(Some(Edit::text(document.to_string())))
    })
    .map(|_| ())
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
