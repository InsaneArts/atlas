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

    if let Some(text) = read_instructions(&claude)?
        && !same_place(&claude, &agents)
        && !text.contains("@AGENTS.md")
    {
        plans.push((claude, Some(text)));
    }

    // Check every file before changing any.
    let mut changes = Vec::new();

    for (path, before) in plans {
        let before = before.unwrap_or_default();
        let after = with_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END}, then run this again",
                path.display()
            ))
        })?;
        changes.push((path, before, after));
    }

    let mut files = Vec::new();

    for (path, before, after) in changes {
        let change = if before == after {
            "unchanged"
        } else if before.contains(SECTION_START) {
            "updated"
        } else {
            "added"
        };

        if before != after {
            replace_file(&path, after.as_bytes())?;
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
        let Some(before) = read_instructions(&path)? else {
            continue;
        };
        let after = without_section(&before).ok_or_else(|| {
            problem(format!(
                "{} has an Atlas marker without its pair. Remove the lines \
                 {SECTION_START} and {SECTION_END} yourself",
                path.display()
            ))
        })?;
        changes.push((path, before, after));
    }

    let mut files = Vec::new();
    for (path, before, after) in changes {
        // A link stays: its target is cleared as teaching wrote through it.
        let linked = std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink());
        let change = if before == after {
            "absent"
        } else if after.trim().is_empty() && !linked && !aliased {
            std::fs::remove_file(&path).map_err(|error| {
                problem(format!("Could not remove {}: {error}", path.display()))
            })?;
            "removed"
        } else {
            replace_file(&path, after.as_bytes())?;
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
    replace_file(record, &serde_json::to_vec_pretty(&listed)?)
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

/// An instruction file's text, or None when there is none.
fn read_instructions(path: &Path) -> Result<Option<String>> {
    let target = file_target(path)
        .map_err(|error| problem(format!("Could not read {}: {error}", path.display())))?;
    match std::fs::read_to_string(target) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(problem(format!(
            "Could not read {}: {error}",
            path.display()
        ))),
    }
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

/// Replace a file through a temporary file beside it, so a failure leaves
/// the old text whole, saying which one could not be written. A link is
/// followed and its target replaced; the file keeps its permissions.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let replaced = || -> Result<()> {
        let target = file_target(path)?;
        let mode = std::fs::metadata(&target).map_or(0o644, |m| m.permissions().mode() & 0o777);
        atlas_core::files::write_atomic(&target, bytes, mode)
    };

    replaced().map_err(|error| problem(format!("Could not write {}: {error}", path.display())))
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
