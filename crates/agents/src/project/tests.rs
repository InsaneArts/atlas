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
fn the_agent_docs_show_the_project_section() {
    let docs = include_str!("../../../../docs/agents.md");
    assert!(docs.contains(&format!("```text\n{}\n```", PROJECT_SECTION.trim_end())));
}

#[test]
fn the_project_section_lets_a_loaded_skill_stand_for_the_guide() {
    let section = PROJECT_SECTION
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let skill = section
        .find("If the atlas skill is loaded, follow it.")
        .unwrap();
    let guide = section.find("Otherwise run `atlas agent-guide`").unwrap();
    assert!(skill < guide, "{section}");
    assert!(
        section.contains("`atlas doctor && atlas windows`"),
        "{section}"
    );
    assert!(
        section.contains("only if that action was yours"),
        "{section}"
    );
}
