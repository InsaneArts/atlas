//! Owned Cursor payloads and a small recoverable directory replacement.
//! Transaction scratch never lives in Cursor's plugin discovery directory.
use super::super::{Removal, problem};
use crate::error::Result;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

const MAX_FILES: usize = 512;
const MAX_FILE: u64 = 2 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    /// Directories have no digest. Their modes and empty presence still count.
    digest: Option<String>,
    mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    entries: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    before: Option<Record>,
    after: Option<Record>,
    copying: Option<Copying>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Copying {
    name: String,
    entry: String,
    size: u64,
    identity: Option<(u64, u64)>,
}

struct Store {
    root: PathBuf,
    local: PathBuf,
    target: PathBuf,
    state: PathBuf,
}

impl Store {
    fn new(root: &Path) -> Self {
        let plugins = root.join("plugins");
        let local = plugins.join("local");
        Self {
            root: root.to_owned(),
            target: local.join("atlas"),
            state: plugins.join(".atlas-state"),
            local,
        }
    }

    fn check_parents(&self, create: bool) -> Result<()> {
        for path in [&self.root, &self.root.join("plugins"), &self.local] {
            directory(path, create, false)?;
        }
        directory(&self.state, create, true)
    }

    fn record_path(&self) -> PathBuf {
        self.state.join("record.json")
    }
    fn transaction_path(&self) -> PathBuf {
        self.state.join("transaction.json")
    }
    fn stage(&self) -> PathBuf {
        self.state.join("stage")
    }
    fn backup(&self) -> PathBuf {
        self.state.join("backup")
    }

    fn record(&self) -> Result<Option<Record>> {
        let record: Option<Record> = read_json(&self.record_path())?;
        if let Some(record) = &record {
            validate(record)?;
        }
        Ok(record)
    }

    fn write_record(&self, record: Option<&Record>) -> Result<()> {
        match record {
            Some(record) => write_json(&self.record_path(), record),
            None => remove_file(&self.record_path()),
        }
    }

    fn finish(&self, transaction: &Transaction) -> Result<()> {
        self.write_record(transaction.after.as_ref())?;
        remove_tree(&self.backup(), transaction.before.as_ref(), true)?;
        remove_tree(&self.stage(), transaction.after.as_ref(), true)?;
        remove_file(&self.transaction_path())?;
        sync_directory(&self.state)
    }

    /// A published replacement is committed. Otherwise restore the previous
    /// payload before another operation begins. Changed scratch is preserved.
    fn recover(&self) -> Result<()> {
        let Some(transaction): Option<Transaction> = read_json(&self.transaction_path())? else {
            if exists(&self.stage())? || exists(&self.backup())? {
                return Err(problem(
                    "Cursor has unrecorded Atlas scratch; inspect its .atlas-state folder before reconnecting",
                ));
            }
            return Ok(());
        };
        for record in [&transaction.before, &transaction.after]
            .into_iter()
            .flatten()
        {
            validate(record)?;
        }
        self.clear_copy(&transaction)?;
        if let Some(after) = &transaction.after
            && matches_record(&self.target, after)?
        {
            return self.finish(&transaction);
        }
        if transaction.after.is_none() && !exists(&self.target)? && !exists(&self.backup())? {
            return self.finish(&transaction);
        }
        if exists(&self.backup())? {
            let before = transaction
                .before
                .as_ref()
                .ok_or_else(|| problem("Cursor's Atlas backup has no ownership record"))?;
            require_match(&self.backup(), before)?;
            if exists(&self.target)? {
                return Err(problem(
                    "Cursor's Atlas destination changed during recovery; no files were replaced",
                ));
            }
            rename_exclusive(&self.backup(), &self.target)?;
            sync_directory(&self.local)?;
        } else if exists(&self.target)? {
            match &transaction.before {
                Some(before) => require_match(&self.target, before)?,
                None => {
                    return Err(problem(
                        "Another Cursor Atlas plugin appeared during recovery; it was kept",
                    ));
                }
            }
        } else if transaction.before.is_some() {
            return Err(problem(
                "Cursor's previous Atlas plugin and backup are missing; inspect .atlas-state before reconnecting",
            ));
        }
        self.write_record(transaction.before.as_ref())?;
        remove_tree(&self.stage(), transaction.after.as_ref(), true)?;
        remove_file(&self.transaction_path())?;
        sync_directory(&self.state)
    }

    /// The journal declares one unfinished copy before it is created. Only
    /// that regular private inode may contain incomplete bytes; completed
    /// stage files still require their full recorded digest.
    fn clear_copy(&self, transaction: &Transaction) -> Result<()> {
        let Some(copying) = &transaction.copying else {
            return Ok(());
        };
        if !copying
            .name
            .strip_prefix(".copy-")
            .is_some_and(|tail| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()))
            || copying.size > MAX_FILE
        {
            return Err(problem("Cursor's unfinished-copy record is invalid"));
        }
        let (entry, digest) = transaction
            .after
            .as_ref()
            .and_then(|after| after.entries.get(&copying.entry))
            .and_then(|entry| entry.digest.as_ref().map(|digest| (entry, digest)))
            .ok_or_else(|| problem("Cursor's unfinished copy is not part of its payload"))?;
        let path = self.state.join(&copying.name);
        if !exists(&path)? {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&path)?;
        let mode = metadata.mode() & 0o777;
        let identity = (metadata.dev(), metadata.ino());
        let private = mode == 0o600 || (metadata.len() == copying.size && mode == entry.mode);
        if !metadata.is_file()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || !private
            || metadata.len() > copying.size
            || copying
                .identity
                .is_some_and(|expected| expected != identity)
            || (copying.identity.is_none() && metadata.len() != 0)
        {
            return Err(problem(
                "Cursor's unfinished Atlas copy changed; it was kept",
            ));
        }
        if metadata.len() == copying.size
            && format!("{:x}", Sha256::digest(read_file(&path)?)) != *digest
        {
            return Err(problem(
                "Cursor's completed Atlas copy changed; it was kept",
            ));
        }
        fs::remove_file(path)?;
        sync_directory(&self.state)
    }
}

pub(super) fn recorded(root: &Path) -> bool {
    let store = Store::new(root);
    // Include an interrupted operation even when its record hasn't settled.
    exists(&store.record_path()).unwrap_or(true)
        || exists(&store.transaction_path()).unwrap_or(true)
}

pub(super) fn connected(root: &Path) -> Result<bool> {
    let store = Store::new(root);
    store.check_parents(false)?;
    if exists(&store.transaction_path())? {
        return Err(problem(
            "Cursor's Atlas connection was interrupted; run atlas agents install --cursor or atlas agents remove --cursor to recover",
        ));
    }
    match store.record()? {
        Some(record) if exists(&store.target)? => {
            require_match(&store.target, &record)?;
            Ok(true)
        }
        Some(_) => Ok(false),
        None if exists(&store.target)? => Err(problem(
            "Cursor has another local plugin named atlas; its files were kept",
        )),
        None => Ok(false),
    }
}

pub(super) fn change(
    root: &Path,
    source: Option<&Path>,
    mut active: impl FnMut() -> Result<()>,
) -> Result<Removal> {
    change_at(root, source, &mut |_| active())
}

fn change_at(
    root: &Path,
    source: Option<&Path>,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<Removal> {
    let store = Store::new(root);
    store.check_parents(false)?;
    if !exists(&store.state)? && source.is_none() {
        return Ok(if exists(&store.target)? {
            Removal::Kept(store.target.display().to_string())
        } else {
            Removal::Nothing
        });
    }
    checkpoint("starting")?;
    store.check_parents(true)?;
    store.recover()?;
    let recorded = store.record()?;
    let before = if exists(&store.target)? {
        let Some(record) = &recorded else {
            return conflict(&store.target, source.is_some());
        };
        if require_match(&store.target, record).is_err() {
            return conflict(&store.target, source.is_some());
        }
        Some(record.clone())
    } else {
        None
    };
    let after = source.map(payload).transpose()?;
    if before == after {
        if recorded.is_some() && before.is_none() {
            store.write_record(None)?;
            return Ok(Removal::Changed);
        }
        return Ok(Removal::Nothing);
    }
    let mut transaction = Transaction {
        before,
        after,
        copying: None,
    };
    checkpoint("before-record")?;
    write_json(&store.transaction_path(), &transaction)?;
    checkpoint("recorded")?;
    if let (Some(source), Some(after)) = (source, transaction.after.clone()) {
        materialize(
            &source.join("atlas"),
            &store,
            &after,
            &mut transaction,
            checkpoint,
        )?;
        checkpoint("staged")?;
    }
    if let Some(before) = &transaction.before {
        require_match(&store.target, before)?;
        checkpoint("before-backup")?;
        rename_exclusive(&store.target, &store.backup())?;
        sync_directory(&store.local)?;
        sync_directory(&store.state)?;
        checkpoint("backed-up")?;
    }
    if transaction.after.is_some() {
        checkpoint("before-publication")?;
        rename_exclusive(&store.stage(), &store.target)?;
        sync_directory(&store.local)?;
        sync_directory(&store.state)?;
    }
    checkpoint("published")?;
    checkpoint("before-settlement")?;
    store.write_record(transaction.after.as_ref())?;
    checkpoint("settled")?;
    store.finish(&transaction)?;
    Ok(Removal::Changed)
}

fn conflict(path: &Path, installing: bool) -> Result<Removal> {
    if installing {
        Err(problem(format!(
            "Cursor's Atlas plugin at {} has personal or unrecognized files; they were kept. Move it aside yourself before reconnecting",
            path.display()
        )))
    } else {
        Ok(Removal::Kept(path.display().to_string()))
    }
}

fn payload(marketplace: &Path) -> Result<Record> {
    let root = marketplace.join("atlas");
    let mut entries = BTreeMap::new();
    for name in [
        ".cursor-plugin",
        "cursor",
        "skills",
        "skills/atlas",
        "skills/atlas/references",
        "skills/setup",
    ] {
        let metadata = fs::symlink_metadata(root.join(name))?;
        if !metadata.is_dir() {
            return Err(problem(
                "Atlas's Cursor payload contains a linked or missing directory",
            ));
        }
        entries.insert(
            name.to_owned(),
            Entry {
                digest: None,
                mode: metadata.permissions().mode() & 0o777,
            },
        );
    }
    let references = crate::agent::REFERENCES
        .iter()
        .map(|(file, _)| format!("skills/atlas/references/{file}"));
    let files = [
        ".cursor-plugin/plugin.json",
        "cursor/mcp.json",
        "cursor/hooks.json",
        "cursor/launch",
        "skills/atlas/SKILL.md",
        "skills/setup/SKILL.md",
    ]
    .map(String::from)
    .into_iter()
    .chain(references);
    for path in files {
        collect(&root, &root.join(path), &mut entries, false)?;
    }
    let manifest: Value =
        serde_json::from_slice(&read_file(&root.join(".cursor-plugin/plugin.json"))?)
            .map_err(|e| problem(format!("Could not read Atlas's Cursor manifest: {e}")))?;
    if manifest["name"] != "atlas"
        || manifest["homepage"] != super::super::HOMEPAGE
        || manifest["version"] != crate::VERSION
    {
        return Err(problem(
            "This Atlas carries an incompatible Cursor plugin; reinstall it",
        ));
    }
    let record = Record { schema: 1, entries };
    validate(&record)?;
    Ok(record)
}

fn validate(record: &Record) -> Result<()> {
    if record.schema != 1 || record.entries.is_empty() || record.entries.len() > MAX_FILES {
        return Err(problem("Cursor's Atlas ownership record is invalid"));
    }
    for (name, entry) in &record.entries {
        if name.is_empty()
            || !Path::new(name)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            || entry.mode & !0o777 != 0
            || entry
                .digest
                .as_ref()
                .is_some_and(|digest| !crate::name::lower_hex(digest, 64))
        {
            return Err(problem(
                "Cursor's Atlas ownership record contains an invalid path or file",
            ));
        }
        if let Some(parent) = Path::new(name)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let parent = parent
                .to_str()
                .ok_or_else(|| problem("Cursor's Atlas path is not UTF-8"))?;
            if record
                .entries
                .get(parent)
                .is_none_or(|entry| entry.digest.is_some())
            {
                return Err(problem(
                    "Cursor's Atlas ownership record has a missing parent directory",
                ));
            }
        }
    }
    Ok(())
}

fn snapshot(root: &Path) -> Result<BTreeMap<String, Entry>> {
    directory(root, false, false)?;
    let mut entries = BTreeMap::new();
    for child in fs::read_dir(root)? {
        collect(root, &child?.path(), &mut entries, true)?;
    }
    Ok(entries)
}

fn collect(
    root: &Path,
    path: &Path,
    entries: &mut BTreeMap<String, Entry>,
    owned: bool,
) -> Result<()> {
    if entries.len() >= MAX_FILES {
        return Err(problem("Cursor's Atlas plugin contains too many files"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if (owned && metadata.uid() != nix::unistd::getuid().as_raw())
        || (!metadata.is_file() && !metadata.is_dir())
    {
        return Err(problem(format!(
            "{} is not an owned regular file or directory",
            path.display()
        )));
    }
    let name = path
        .strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .ok_or_else(|| problem("Cursor's Atlas plugin path is invalid"))?
        .to_owned();
    let entry = Entry {
        digest: if metadata.is_file() {
            Some(format!("{:x}", Sha256::digest(read_file(path)?)))
        } else {
            None
        },
        mode: metadata.permissions().mode() & 0o777,
    };
    entries.insert(name, entry);
    if metadata.is_dir() {
        for child in fs::read_dir(path)? {
            collect(root, &child?.path(), entries, owned)?;
        }
    }
    Ok(())
}

fn matches_record(path: &Path, record: &Record) -> Result<bool> {
    if !exists(path)? {
        return Ok(false);
    }
    Ok(snapshot(path)? == record.entries)
}

fn require_match(path: &Path, record: &Record) -> Result<()> {
    if !matches_record(path, record)? {
        return Err(problem(format!(
            "Cursor's Atlas files at {} changed; they were kept",
            path.display()
        )));
    }
    Ok(())
}

fn materialize(
    source: &Path,
    store: &Store,
    record: &Record,
    transaction: &mut Transaction,
    checkpoint: &mut impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let target = store.stage();
    checkpoint("before-stage")?;
    fs::DirBuilder::new().mode(0o700).create(&target)?;
    for (name, entry) in &record.entries {
        checkpoint("before-entry")?;
        let path = target.join(name);
        if let Some(digest) = &entry.digest {
            let bytes = read_file(&source.join(name))?;
            if format!("{:x}", Sha256::digest(&bytes)) != *digest {
                return Err(problem(
                    "Atlas's Cursor plugin changed while it was being copied",
                ));
            }
            // An interrupted write stays outside the stage tree. Recovery
            // only sees complete files. The journal owns the unfinished inode.
            let scratch_name = format!(".copy-{}", rand::random::<u64>());
            let temporary = store.state.join(&scratch_name);
            transaction.copying = Some(Copying {
                name: scratch_name,
                entry: name.clone(),
                size: bytes.len() as u64,
                identity: None,
            });
            write_json(&store.transaction_path(), transaction)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            let metadata = file.metadata()?;
            #[allow(clippy::unwrap_used, reason = "The copy record was just inserted")]
            {
                transaction.copying.as_mut().unwrap().identity =
                    Some((metadata.dev(), metadata.ino()));
            }
            write_json(&store.transaction_path(), transaction)?;
            let middle = bytes.len() / 2;
            file.write_all(&bytes[..middle])?;
            file.sync_all()?;
            checkpoint("copy-written")?;
            file.write_all(&bytes[middle..])?;
            file.set_permissions(fs::Permissions::from_mode(entry.mode))?;
            file.sync_all()?;
            checkpoint("before-copy-publication")?;
            rename_exclusive(&temporary, &path)?;
            transaction.copying = None;
            write_json(&store.transaction_path(), transaction)?;
        } else {
            fs::DirBuilder::new().mode(entry.mode).create(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode))?;
        }
    }
    require_match(&target, record)?;
    sync_directory(&target)
}

fn remove_tree(path: &Path, record: Option<&Record>, partial: bool) -> Result<()> {
    if !exists(path)? {
        return Ok(());
    }
    let record = record.ok_or_else(|| problem("Cursor's Atlas scratch has no ownership record"))?;
    let actual = snapshot(path)?;
    if (!partial && actual != record.entries)
        || actual
            .iter()
            .any(|(name, entry)| record.entries.get(name) != Some(entry))
    {
        return Err(problem(format!(
            "Cursor's Atlas scratch at {} changed; it was kept",
            path.display()
        )));
    }
    // Every child was checked without following links, including empty dirs.
    // Remove listed entries individually; never recursively follow new paths.
    for (name, entry) in actual.iter().rev() {
        let child = path.join(name);
        if entry.digest.is_none() {
            fs::remove_dir(child)?;
        } else {
            fs::remove_file(child)?;
        }
    }
    fs::remove_dir(path)?;
    Ok(())
}

fn directory(path: &Path, create: bool, private: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if create {
                fs::DirBuilder::new().mode(0o700).create(path)?;
            }
            return Ok(());
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) => {
            if !metadata.is_dir()
                || metadata.uid() != nix::unistd::getuid().as_raw()
                || metadata.mode() & 0o022 != 0
                || (private && metadata.mode() & 0o077 != 0)
            {
                return Err(problem(format!(
                    "{} is not an owned{} directory",
                    path.display(),
                    if private { " private" } else { "" }
                )));
            }
        }
    }
    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    let file = crate::files::open_plain(path)
        .ok_or_else(|| problem(format!("Could not read regular file {}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(problem("Cursor's Atlas file exceeds 2 MiB"));
    }
    Ok(bytes)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !exists(path)? {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(problem(format!(
            "{} is not an owned private file",
            path.display()
        )));
    }
    serde_json::from_slice(&read_file(path)?)
        .map(Some)
        .map_err(|e| problem(format!("Could not read {}: {e}", path.display())))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| problem("Cursor's Atlas state has no parent"))?;
    let temporary = parent.join(format!(".write-{}", rand::random::<u64>()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let outcome = (|| {
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_directory(parent)
    })();
    let _ = fs::remove_file(temporary);
    outcome
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    Ok(File::open(path)?.sync_all()?)
}

/// Publish only into an absent name; another process's empty directory is
/// still its own and must not be replaced.
#[cfg(target_os = "macos")]
fn rename_exclusive(from: &Path, to: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from = CString::new(from.as_os_str().as_bytes()).map_err(|e| problem(e.to_string()))?;
    let to = CString::new(to.as_os_str().as_bytes()).map_err(|e| problem(e.to_string()))?;
    crate::fs_move::rename_path(&from, &to, crate::fs_move::Replace::Never)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn rename_exclusive(_from: &Path, _to: &Path) -> Result<()> {
    Err(problem("Cursor registration requires macOS"))
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
