//! Project instruction sections and the private registry of taught folders.
use super::{problem, same_place};
use crate::error::Result;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The section `atlas agents project` keeps in a project's AGENTS.md: when
/// to test in the helper, and how. The README shows the same text.
pub const PROJECT_SECTION: &str = include_str!("../agent_project.md");
const SECTION_START: &str = "<!-- atlas:start -->";
const SECTION_END: &str = "<!-- atlas:end -->";

/// Add the Atlas section to a project's AGENTS.md, or bring it up to date,
/// and to its CLAUDE.md when that is a separate file that does not already
/// include AGENTS.md. Nothing else in either file changes, and a file is
/// replaced whole or not at all.
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
            replace_file(&path, after.as_bytes())
                .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?;
        }

        files.push(json!({"path": path, "change": change}));
    }

    // Kept so Atlas's Settings can list the project and forget it later;
    // the files are what agents read, so a list that can't be saved never
    // undoes them. Tests never write the person's own list.
    #[cfg(not(test))]
    if let Ok(record) = taught_record() {
        let _ = remember(&record, dir, true);
    }

    Ok(json!({"ok": true, "files": files}))
}

/// Take the Atlas section out of a project's AGENTS.md and CLAUDE.md, and
/// forget the project. Nothing else in either file changes; a file left
/// with nothing in it was only ever the section, and goes.
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
            replace_file(&path, after.as_bytes())
                .map_err(|error| problem(format!("Could not write {}: {error}", path.display())))?;
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
pub fn taught() -> Vec<Value> {
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

    let Some(file) = crate::helper_info::open_plain(path) else {
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
    Ok(crate::config::home()?.join("Library/Application Support/Atlas/projects.json"))
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
    use std::os::unix::fs::OpenOptionsExt;

    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Some(parent) = record.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| problem(format!("Could not write {}: {error}", parent.display())))?;
    }
    let lock_path = record.with_extension("lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lock_path)
        .map_err(|error| problem(format!("Could not write {}: {error}", lock_path.display())))?;
    lock.lock_exclusive()
        .map_err(|error| problem(format!("Could not lock {}: {error}", lock_path.display())))?;

    let mut listed = taught_in(record);
    listed.retain(|known| known != &dir);
    if keep {
        listed.push(dir);
    }
    replace_file(record, &serde_json::to_vec_pretty(&listed)?)
        .map_err(|error| problem(format!("Could not write {}: {error}", record.display())))
}

/// `text` without the Atlas section and the blank line put before it;
/// unchanged when there is none, and None when the markers are not exactly
/// one pair in order.
fn without_section(text: &str) -> Option<String> {
    match (
        text.matches(SECTION_START).count(),
        text.matches(SECTION_END).count(),
    ) {
        (0, 0) => Some(text.to_owned()),
        (1, 1) => {
            let start = text.find(SECTION_START)?;
            let end = text.find(SECTION_END).filter(|&end| end > start)?;
            // Only what adding put there goes: the line ending after the
            // section, and the blank line before it.
            let mut before = &text[..start];
            if before.ends_with("\n\n") {
                before = &before[..before.len() - 1];
            }
            let rest = &text[end + SECTION_END.len()..];
            let after = rest.strip_prefix('\n').unwrap_or(rest);
            Some(format!("{before}{after}"))
        }
        _ => None,
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

    match (
        text.matches(SECTION_START).count(),
        text.matches(SECTION_END).count(),
    ) {
        (0, 0) => {}
        (1, 1) => {
            let start = text.find(SECTION_START)?;
            let end = text.find(SECTION_END).filter(|&end| end > start)?;
            let rest = &text[end + SECTION_END.len()..];
            return Some(format!(
                "{}{block}{}",
                &text[..start],
                rest.strip_prefix('\n').unwrap_or(rest)
            ));
        }
        _ => return None,
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
/// the old text whole. A link is followed and its target replaced; the
/// file keeps its permissions.
fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, os::unix::fs::PermissionsExt};

    let target = file_target(path)?;
    let mode = std::fs::metadata(&target).map_or(0o644, |m| m.permissions().mode() & 0o777);
    let temporary = target.with_file_name(format!(
        ".{}.atlas-{}",
        target
            .file_name()
            .map_or("instructions".into(), |n| n.to_string_lossy()),
        crate::control::token()
    ));

    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)?;
        // The mode given at creation is reduced by the umask.
        file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        file.write_all(bytes)?;
        file.sync_all()?;

        std::fs::rename(&temporary, &target)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }

    result
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
mod tests {
    use super::*;

    #[test]
    fn the_project_section_is_added_once_and_kept_current() {
        let block = format!("{SECTION_START}\n{PROJECT_SECTION}{SECTION_END}\n");
        assert_eq!(with_section("").unwrap(), block);
        assert_eq!(
            with_section("# Rules").unwrap(),
            format!("# Rules\n\n{block}")
        );
        assert_eq!(
            with_section("# Rules\n").unwrap(),
            format!("# Rules\n\n{block}")
        );

        let once = with_section("# Rules\n\nKeep this.\n").unwrap();
        assert_eq!(
            with_section(&once).unwrap(),
            once,
            "a second run changes nothing"
        );

        let old = format!("# Rules\n{SECTION_START}\nold words\n{SECTION_END}\nAfter.\n");
        assert_eq!(
            with_section(&old).unwrap(),
            format!("# Rules\n{block}After.\n")
        );

        // An unpaired or reversed marker is refused, never guessed at.
        for broken in [
            format!("{SECTION_START}\nKeep this.\n"),
            format!("Keep this.\n{SECTION_END}\n"),
            format!("{SECTION_END}\n{SECTION_START}\n"),
            format!("{once}{SECTION_START}\n"),
        ] {
            assert_eq!(with_section(&broken), None, "{broken}");
        }
    }

    #[test]
    fn a_broken_marker_leaves_every_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Rules\n").unwrap();
        let broken = format!("# Claude\n{SECTION_START}\nKeep this.\n");
        std::fs::write(dir.path().join("CLAUDE.md"), &broken).unwrap();
        assert!(project(dir.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
            "# Rules\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
            broken
        );
    }

    #[test]
    fn an_unresolved_instruction_link_leaves_every_file_unchanged() {
        for (name, other) in [("AGENTS.md", "CLAUDE.md"), ("CLAUDE.md", "AGENTS.md")] {
            for target in ["missing.md", name] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join(name);
                let other = dir.path().join(other);
                let before = with_section("# Rules\n").unwrap();
                std::fs::write(&other, &before).unwrap();
                std::os::unix::fs::symlink(target, &path).unwrap();
                let record = dir.path().join("projects.json");

                let error = project(dir.path()).unwrap_err();
                assert!(error.error.contains(&path.display().to_string()));
                assert!(error.error.contains("Could not resolve"));
                assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(target));
                assert_eq!(std::fs::read_to_string(&other).unwrap(), before);

                let error = forget_in(dir.path(), &record).unwrap_err();
                assert!(error.error.contains(&path.display().to_string()));
                assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(target));
                assert_eq!(std::fs::read_to_string(&other).unwrap(), before);
                assert!(!record.exists());
                assert!(!dir.path().join("missing.md").exists());
            }
        }
    }

    #[test]
    fn forgetting_takes_back_only_what_teaching_added() {
        for text in ["A", "A\n", "A\n\n", "# Opie\n\nBuild with make.\n"] {
            let taught = with_section(text).unwrap();
            let forgotten = without_section(&taught).unwrap();
            assert!(
                forgotten == text || forgotten == format!("{}\n", text.trim_end_matches('\n')),
                "{text:?} came back as {forgotten:?}"
            );
        }
        let moved = format!("X\n{SECTION_START}\nbody\n{SECTION_END}\nY");
        assert_eq!(
            without_section(&moved).unwrap(),
            "X\nY",
            "no blank line added"
        );
    }

    #[test]
    fn forgetting_a_folder_that_isnt_there_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("opei");
        let error = forget_in(&missing, &dir.path().join("projects.json")).unwrap_err();
        assert!(error.error.contains("is not a folder"), "{}", error.error);
    }

    #[test]
    fn forgetting_keeps_a_linked_agents_file_and_clears_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("opie");
        std::fs::create_dir_all(&project).unwrap();
        let shared = dir.path().join("shared.md");
        std::fs::write(&shared, with_section("").unwrap()).unwrap();
        std::os::unix::fs::symlink(&shared, project.join("AGENTS.md")).unwrap();
        std::os::unix::fs::symlink("AGENTS.md", project.join("CLAUDE.md")).unwrap();

        let report = forget_in(&project, &dir.path().join("projects.json")).unwrap();

        assert!(project.join("AGENTS.md").is_symlink(), "the link stays");
        assert!(project.join("CLAUDE.md").is_symlink(), "the alias stays");
        assert_eq!(std::fs::read_to_string(&shared).unwrap(), "");
        assert_eq!(
            report["files"].as_array().unwrap().len(),
            1,
            "the alias once"
        );
    }

    #[test]
    fn teaching_and_forgetting_at_once_lose_no_listing() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("projects.json");
        let folders = (0..16)
            .map(|index| {
                let folder = dir.path().join(format!("p{index}"));
                std::fs::create_dir_all(&folder).unwrap();
                folder
            })
            .collect::<Vec<_>>();

        std::thread::scope(|scope| {
            for folder in &folders {
                let record = &record;
                scope.spawn(move || remember(record, folder, true).unwrap());
            }
        });

        assert_eq!(taught_in(&record).len(), folders.len());
    }

    #[test]
    fn an_unresolved_project_list_link_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("projects.json");
        let target = Path::new("missing.json");
        std::os::unix::fs::symlink(target, &record).unwrap();

        for keep in [true, false] {
            let error = remember(&record, dir.path(), keep).unwrap_err();
            assert!(error.error.contains(&record.display().to_string()));
            assert!(error.error.contains("Could not resolve"));
            assert_eq!(std::fs::read_link(&record).unwrap(), target);
            assert!(!dir.path().join(target).exists());
        }
    }

    #[test]
    fn forgetting_a_project_takes_out_only_the_section_and_its_listing() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("opie");
        let other = dir.path().join("landing");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let record = dir.path().join("projects.json");
        std::fs::write(project.join("AGENTS.md"), "# Opie\n\nBuild with make.\n").unwrap();
        std::fs::write(project.join("CLAUDE.md"), "Only ours.").unwrap();
        super::project(&project).unwrap();
        std::fs::write(project.join("CLAUDE.md"), with_section("").unwrap()).unwrap();
        remember(&record, &project, true).unwrap();
        remember(&record, &other, true).unwrap();

        let report = forget_in(&project, &record).unwrap();

        assert_eq!(
            std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
            "# Opie\n\nBuild with make.\n"
        );
        assert!(!project.join("CLAUDE.md").exists(), "only ever the section");
        assert_eq!(report["files"][0]["change"], "removed");
        assert_eq!(taught_in(&record), [other.canonicalize().unwrap()]);
        assert_eq!(
            forget_in(&project, &record).unwrap()["files"][0]["change"],
            "absent",
            "forgetting twice changes nothing"
        );
    }

    #[test]
    fn a_project_gets_the_section_in_agents_and_a_separate_claude_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        let result = project(dir.path()).unwrap();
        let changes: Vec<&str> = result["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["change"].as_str().unwrap())
            .collect();
        assert_eq!(changes, ["added", "added"]);

        for name in ["AGENTS.md", "CLAUDE.md"] {
            let text = std::fs::read_to_string(dir.path().join(name)).unwrap();
            assert!(text.contains(PROJECT_SECTION), "{name}");
        }

        assert_eq!(
            project(dir.path()).unwrap()["files"][0]["change"],
            "unchanged"
        );

        // A CLAUDE.md that already includes AGENTS.md is left alone.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "@AGENTS.md\n").unwrap();

        assert_eq!(
            project(dir.path()).unwrap()["files"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
            "@AGENTS.md\n"
        );
    }

    #[test]
    fn a_replaced_file_keeps_its_exact_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        replace_file(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o664);
    }

    #[test]
    fn the_readme_shows_the_project_section() {
        let readme = include_str!("../../../README.md");
        assert!(readme.contains(PROJECT_SECTION.trim_end()));
    }
}
