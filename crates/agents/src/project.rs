//! Project instruction sections and the private registry of taught folders.
use super::{problem, same_place};
use atlas_core::error::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The section `atlas agents project` keeps in a project's AGENTS.md: when
/// to test in the helper, and how. The README shows the same text.
const PROJECT_SECTION: &str = include_str!("agent_project.md");
const SECTION_START: &str = "<!-- atlas:start -->";
const SECTION_END: &str = "<!-- atlas:end -->";

/// Add the Atlas section to a project's AGENTS.md, or bring it up to date,
/// and to its CLAUDE.md when that is a separate file that does not already
/// include AGENTS.md. Nothing else in either file changes, and a file is
/// replaced whole or not at all.
///
/// # Errors
///
/// Returns project-discovery, installation or file-write failures. Existing unrelated
/// project configuration is preserved by the host-specific integration.
pub fn project(dir: &Path) -> Result<Value> {
    if !dir.is_dir() {
        return Err(problem(format!("{} is not a folder", dir.display())));
    }

    let agents = dir.join("AGENTS.md");
    let claude = dir.join("CLAUDE.md");
    let mut plans = vec![(agents.clone(), read_instructions(&agents)?)];

    if let Some(read) = read_instructions(&claude)?
        && !same_place(&claude, &agents)
        && !read.text.contains("@AGENTS.md")
    {
        plans.push((claude, Some(read)));
    }

    // Check every file before changing any.
    let mut changes = Vec::new();

    for (path, read) in plans {
        let before = read
            .as_ref()
            .map(|read| read.text.clone())
            .unwrap_or_default();
        let after = with_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END}, then run this again",
                path.display()
            ))
        })?;
        changes.push((path, read, before, after));
    }

    let mut files = Vec::new();

    for (path, read, before, after) in changes {
        let change = if before == after {
            "unchanged"
        } else if before.contains(SECTION_START) {
            "updated"
        } else {
            "added"
        };

        if before != after {
            replace_file(&path, read.as_ref(), after.as_bytes())?;
        }

        files.push(json!({"path": path, "change": change}));
    }

    // Kept so Atlas's Settings can list the project and forget it later;
    // the files are what agents read, so a list that can't be saved never
    // undoes them. Tests never write the person's own list.
    #[cfg(not(any(test, feature = "test-support")))]
    if let Ok(record) = taught_record() {
        let _ = remember(&record, dir, true);
    }

    Ok(json!({"ok": true, "files": files}))
}

/// Take the Atlas section out of a project's AGENTS.md and CLAUDE.md, and
/// forget the project. Nothing else in either file changes; a file left
/// with nothing in it was only ever the section, and goes.
///
/// # Errors
///
/// Returns project-discovery or file-update failures while removing Atlas-owned integration
/// settings.
pub fn forget(dir: &Path) -> Result<Value> {
    forget_in(dir, &taught_record()?)
}

/// [`forget`], with the list of taught projects at `record`.
fn forget_in(dir: &Path, record: &Path) -> Result<Value> {
    if !dir.is_dir() {
        return Err(problem(format!("{} is not a folder", dir.display())));
    }

    let agents = dir.join("AGENTS.md");
    let claude = dir.join("CLAUDE.md");
    // A file another name here links to stays, even when only the section
    // was in it, so that link keeps working.
    let aliased = same_place(&claude, &agents);
    let mut changes = Vec::new();
    for path in [agents.clone(), claude.clone()] {
        // CLAUDE.md that is AGENTS.md under another name is changed once.
        if path == claude && same_place(&claude, &agents) {
            continue;
        }
        let Some(read) = read_instructions(&path)? else {
            continue;
        };
        let before = read.text.clone();
        let after = without_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END} yourself",
                path.display()
            ))
        })?;
        changes.push((path, read, before, after));
    }

    let mut files = Vec::new();
    for (path, read, before, after) in changes {
        // A link stays: its target is cleared as teaching wrote through it.
        let linked = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink());
        let change = if before == after {
            "absent"
        } else if after.trim().is_empty() && !linked && !aliased {
            remove_instructions(&path, &read)?;
            "removed"
        } else {
            replace_file(&path, Some(&read), after.as_bytes())?;
            "removed"
        };
        files.push(json!({"path": path, "change": change}));
    }

    // The files are what agents read: once they're changed, a list that
    // can't be saved is said, never reported as the forgetting failing.
    let mut report = json!({"ok": true, "files": files});
    if let Err(error) = remember(record, dir, false) {
        report["warnings"] = json!([format!(
            "Atlas's list of projects wasn't updated: {}",
            error.error
        )]);
    }
    Ok(report)
}

/// The projects Atlas taught, each with whether its AGENTS.md still has
/// the section.
pub(super) fn taught() -> Vec<Value> {
    let Ok(record) = taught_record() else {
        return Vec::new();
    };
    taught_in(&record)
        .into_iter()
        .map(|dir| {
            let still = has_section(&dir.join("AGENTS.md"));
            json!({"path": dir, "taught": still})
        })
        .collect()
}

/// Whether an instruction file has the Atlas section: read only as a plain
/// file, never waiting on a pipe, and no more of it than an instruction file
/// takes.
fn has_section(path: &Path) -> bool {
    use std::io::Read;

    let Some(file) = atlas_core::files::open_plain(path) else {
        return false;
    };
    let mut text = String::new();
    file.take(INSTRUCTIONS_LIMIT)
        .read_to_string(&mut text)
        .is_ok_and(|_| text.contains(SECTION_START))
}

/// 1 MiB: more than any project's AGENTS.md, so reading one never runs on.
const INSTRUCTIONS_LIMIT: u64 = 1 << 20;

/// Where the taught projects are listed: one JSON list of folders.
fn taught_record() -> Result<PathBuf> {
    Ok(atlas_core::config::home()?.join("Library/Application Support/Atlas/projects.json"))
}

/// The folders `record` lists; none when it is missing or unreadable.
fn taught_in(record: &Path) -> Vec<PathBuf> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).ok())
        .unwrap_or_default()
}

/// Add `dir` to the list (`keep`) or take it off, by its full path. One
/// change at a time: a lock beside the list is held from reading it to
/// replacing it, so two at once never lose an entry.
fn remember(record: &Path, dir: &Path, keep: bool) -> Result<()> {
    use fs2::FileExt;

    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Some(parent) = record.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| problem(format!("Could not write {}: {error}", parent.display())))?;
    }
    let lock_path = record.with_extension("lock");
    let lock =
        atlas_core::files::open_lock_file(&lock_path, atlas_core::files::LockOpen::ReadWrite)
            .map_err(|error| {
                problem(format!("Could not write {}: {error}", lock_path.display()))
            })?;
    let lock = atlas_core::files::LockFile(lock);
    lock.0
        .lock_exclusive()
        .map_err(|error| problem(format!("Could not lock {}: {error}", lock_path.display())))?;

    let mut listed = taught_in(record);
    listed.retain(|known| known != &dir);
    if keep {
        listed.push(dir);
    }
    replace_record(record, &serde_json::to_vec_pretty(&listed)?)
}

/// What surrounds the Atlas section in an instruction file.
enum Section<'a> {
    Absent,
    /// The text before its first marker, and after its last marker's line
    /// ending.
    Between(&'a str, &'a str),
}

impl<'a> Section<'a> {
    /// The section in `text`; None when the markers are not exactly one pair
    /// in order.
    fn of(text: &'a str) -> Option<Self> {
        match (
            text.matches(SECTION_START).count(),
            text.matches(SECTION_END).count(),
        ) {
            (0, 0) => Some(Self::Absent),
            (1, 1) => {
                let start = text.find(SECTION_START)?;
                let end = text.find(SECTION_END).filter(|&end| end > start)?;
                let rest = &text[end + SECTION_END.len()..];
                Some(Self::Between(
                    &text[..start],
                    rest.strip_prefix('\n').unwrap_or(rest),
                ))
            }
            _ => None,
        }
    }
}

/// `text` without the Atlas section and the blank line put before it;
/// unchanged when there is none, and None when the markers are not exactly
/// one pair in order.
fn without_section(text: &str) -> Option<String> {
    match Section::of(text)? {
        Section::Absent => Some(text.to_owned()),
        Section::Between(before, after) => {
            // Only what adding put there goes: the line ending after the
            // section, and the blank line before it.
            let before = before
                .strip_suffix('\n')
                .filter(|kept| kept.ends_with('\n'))
                .unwrap_or(before);
            Some(format!("{before}{after}"))
        }
    }
}

/// An instruction file as it was read: its text, the real file it is, and
/// which file that was, so a later change acts only on that very file.
struct Instructions {
    text: String,
    target: PathBuf,
    identity: atlas_core::checked::Identity,
    mode: u32,
}

/// An instruction file, or None when there is none. A link is followed to
/// its real target once, then the target is read through the one guarded
/// reader: never Atlas's own folder, its folders proven unchangeable by any
/// other account, read through the descriptor it opened.
fn read_instructions(path: &Path) -> Result<Option<Instructions>> {
    use std::os::unix::fs::PermissionsExt;

    // Nothing there is no instructions; a link whose target cannot be
    // resolved is a project to repair, as before, not an empty file.
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(problem(format!(
                "Could not read {}: {error}",
                path.display()
            )));
        }
        Ok(_) => {}
    }
    let target = std::fs::canonicalize(path)
        .map_err(|error| problem(format!("Could not read {}: {error}", path.display())))?;
    let reading = atlas_core::checked::read(&target)?;
    let identity = reading.identity()?;
    let target = reading.path().to_path_buf();
    let (file, bytes) = reading.read_keeping(INSTRUCTIONS_LIMIT + 1)?;
    if bytes.len() as u64 > INSTRUCTIONS_LIMIT {
        return Err(problem(format!("{} is too large", path.display())));
    }
    let mode = file
        .metadata()
        .map_or(0o644, |metadata| metadata.permissions().mode() & 0o777);
    let text =
        String::from_utf8(bytes).map_err(|_| problem(format!("{} is not text", path.display())))?;
    Ok(Some(Instructions {
        text,
        target,
        identity,
        mode,
    }))
}

/// The folder an instruction file `target` is in, proven unchangeable by
/// any other account, and its name there.
fn instructions_folder<'a>(
    path: &Path,
    target: &'a Path,
) -> Result<(atlas_core::checked::Folder, &'a std::ffi::OsStr)> {
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            problem(format!(
                "Could not write {}: it has no folder",
                path.display()
            ))
        })?;
    let name = target.file_name().ok_or_else(|| {
        problem(format!(
            "Could not write {}: it has no name",
            path.display()
        ))
    })?;
    Ok((atlas_core::checked::folder(parent)?, name))
}

/// Refuse when the instruction file at `name` in `folder` is no longer the
/// one `read` was.
fn still_read(
    path: &Path,
    folder: &atlas_core::checked::Folder,
    name: &std::ffi::OsStr,
    read: &Instructions,
) -> Result<()> {
    if folder.entry_identity(name)? == Some(read.identity) {
        return Ok(());
    }
    Err(problem(format!(
        "{} changed while Atlas was updating it; run this again",
        path.display()
    )))
}

/// Remove the instruction file `read`, read at `path`, through its folder's
/// descriptor, only while it is still that very file.
fn remove_instructions(path: &Path, read: &Instructions) -> Result<()> {
    let (folder, name) = instructions_folder(path, &read.target)?;
    still_read(path, &folder, name, read)?;
    if !folder.remove_if_same(name, read.identity)? {
        return Err(problem(format!(
            "{} changed while Atlas was updating it; run this again",
            path.display()
        )));
    }
    Ok(())
}

/// `text` with the Atlas section in place of an earlier one, or appended;
/// None when the markers are not exactly one pair in order.
fn with_section(text: &str) -> Option<String> {
    let block = format!("{SECTION_START}\n{PROJECT_SECTION}{SECTION_END}\n");

    if let Section::Between(before, after) = Section::of(text)? {
        return Some(format!("{before}{block}{after}"));
    }

    let gap = match text {
        "" => "",
        _ if text.ends_with("\n\n") => "",
        _ if text.ends_with('\n') => "\n",
        _ => "\n\n",
    };

    Some(format!("{text}{gap}{block}"))
}

/// Replace a project's instruction file: the very file `read` was, or, with
/// none read, the file at `path` (a link followed to its real target once).
/// It is written through its folder's own descriptor, proven to be one no
/// other account can change and never Atlas's own, a temporary file placed
/// there with no link followed and renamed over it, keeping its permissions.
fn replace_file(path: &Path, read: Option<&Instructions>, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let target = match read {
        Some(read) => read.target.clone(),
        None => file_target(path)
            .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?,
    };
    let (folder, name) = instructions_folder(path, &target)?;
    let mode = match read {
        Some(read) => {
            still_read(path, &folder, name, read)?;
            read.mode
        }
        None => std::fs::metadata(&target).map_or(0o644, |meta| meta.permissions().mode() & 0o777),
    };
    folder.replace(name, bytes, mode)
}

/// Replace Atlas's own record of taught projects, which lives in its own
/// folder: the own-folder guard is not for it.
fn replace_record(path: &Path, bytes: &[u8]) -> Result<()> {
    let target = file_target(path)
        .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?;
    write_target(path, &target, bytes)
}

/// Write `bytes` to `target`, the resolved place of `path`, keeping its
/// permissions and replacing it whole.
fn write_target(path: &Path, target: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(target).map_or(0o644, |m| m.permissions().mode() & 0o777);
    atlas_core::files::write_atomic(target, bytes, mode)
        .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))
}

/// Resolve an existing file, refusing links whose targets cannot be resolved.
fn file_target(path: &Path) -> std::io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => path.canonicalize().map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Could not resolve {}: {error}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
