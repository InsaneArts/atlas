use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("agents");
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
        let record = payload(&original).unwrap();
        let target = source.join("atlas");
        fs::create_dir_all(&target).unwrap();
        for (name, entry) in &record.entries {
            let path = target.join(name);
            if entry.digest.is_none() {
                fs::create_dir(&path).unwrap();
            } else {
                fs::copy(original.join("atlas").join(name), &path).unwrap();
            }
            fs::set_permissions(path, fs::Permissions::from_mode(entry.mode)).unwrap();
        }
        Self {
            root: directory.path().join(".cursor"),
            source,
            _directory: directory,
        }
    }

    fn store(&self) -> Store {
        Store::new(&self.root)
    }

    fn install(&self) -> Result<Removal> {
        change(&self.root, Some(&self.source), || Ok(()))
    }
    fn remove(&self) -> Result<Removal> {
        change(&self.root, None, || Ok(()))
    }

    fn update_source(&self) {
        let skill = self.source.join("atlas/skills/atlas/SKILL.md");
        let mut text = fs::read_to_string(&skill).unwrap();
        text.push_str("\nA new version of the shared skill.\n");
        fs::write(skill, text).unwrap();
    }

    fn fail_at(&self, point: &str, remove: bool) -> Result<Removal> {
        change_at(
            &self.root,
            if remove { None } else { Some(&self.source) },
            &mut |at| {
                if at == point {
                    Err(problem("simulated interruption"))
                } else {
                    Ok(())
                }
            },
        )
    }

    fn clean(&self, installed: bool) {
        let names = |path: &Path| {
            let mut names: Vec<_> = fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        assert_eq!(
            names(&self.store().state),
            if installed {
                vec!["record.json"]
            } else {
                vec![]
            }
        );
        assert_eq!(
            names(&self.store().local),
            if installed { vec!["atlas"] } else { vec![] }
        );
    }
}

#[test]
fn cursor_installs_only_its_curated_payload_and_repeats_without_changes() {
    let fixture = Fixture::new();
    fs::write(
        fixture.source.join("atlas/cursor/personal.txt"),
        "keep in source",
    )
    .unwrap();
    assert_eq!(fixture.install().unwrap(), Removal::Changed);
    assert!(connected(&fixture.root).unwrap());
    assert!(!fixture.store().target.join("cursor/personal.txt").exists());
    assert!(!fixture.store().target.join(".claude-plugin").exists());
    assert!(!fixture.store().target.join("hooks/register.js").exists());
    for (file, _) in crate::agent::REFERENCES {
        let installed = fixture
            .store()
            .target
            .join("skills/atlas/references")
            .join(file);
        assert!(installed.is_file(), "{file}");
    }
    assert_eq!(fixture.install().unwrap(), Removal::Nothing);
    fixture.clean(true);
    assert_eq!(fixture.remove().unwrap(), Removal::Changed);
    assert_eq!(fixture.remove().unwrap(), Removal::Nothing);
    fixture.clean(false);
}

#[test]
fn cursor_refreshes_an_owned_old_copy_and_removes_without_the_source() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    fixture.update_source();
    assert!(recorded(&fixture.root));
    assert!(
        connected(&fixture.root).unwrap(),
        "registration remains coherent while its source is newer"
    );
    assert_eq!(fixture.install().unwrap(), Removal::Changed);
    assert!(
        fs::read_to_string(fixture.store().target.join("skills/atlas/SKILL.md"))
            .unwrap()
            .contains("new version")
    );
    fs::rename(&fixture.source, fixture.source.with_extension("moved")).unwrap();
    assert_eq!(fixture.remove().unwrap(), Removal::Changed);
    fixture.clean(false);
}

#[test]
fn cursors_payload_carries_every_reference_the_skill_lists() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
    let record = payload(&source).unwrap();
    for (file, _) in crate::agent::REFERENCES {
        let path = format!("skills/atlas/references/{file}");
        let entry = record.entries.get(&path);
        assert!(entry.is_some_and(|entry| entry.digest.is_some()), "{path}");
    }
}

#[test]
fn cursor_refreshes_an_owned_copy_that_had_fewer_references() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    let store = fixture.store();
    let mut record = store.record().unwrap().unwrap();
    for file in ["testing.md", "environment.md", "limits.md"] {
        let path = format!("skills/atlas/references/{file}");
        fs::remove_file(store.target.join(&path)).unwrap();
        record.entries.remove(&path).unwrap();
    }
    store.write_record(Some(&record)).unwrap();

    assert_eq!(fixture.install().unwrap(), Removal::Changed);
    let expected = payload(&fixture.source).unwrap();
    assert_eq!(snapshot(&store.target).unwrap(), expected.entries);
    assert!(
        store
            .target
            .join("skills/atlas/references/limits.md")
            .is_file()
    );
    assert_eq!(fixture.install().unwrap(), Removal::Nothing);
}

#[test]
fn cursor_refresh_and_default_removal_include_an_owned_copy_without_the_host() {
    const CHILD: &str = "ATLAS_CURSOR_LIFECYCLE_TEST";
    if std::env::var_os(CHILD).is_some() {
        let root = super::super::home().unwrap();
        let expected = payload(&crate::agents::marketplace().unwrap()).unwrap();
        assert_ne!(
            snapshot(&Store::new(&root).target).unwrap(),
            expected.entries
        );
        let refreshed = crate::agents::refresh();
        let cursor = refreshed["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["agent"] == "cursor")
            .unwrap();
        assert_eq!(cursor["ok"], true, "{refreshed}");
        assert_eq!(
            snapshot(&Store::new(&root).target).unwrap(),
            expected.entries
        );
        let removed = crate::agents::change(&[], true).unwrap();
        let cursor = removed["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["agent"] == "cursor")
            .unwrap();
        assert_eq!(cursor["ok"], true, "{removed}");
        assert_eq!(cursor["changed"], true);
        assert_eq!(cursor["connected"], false);
        assert_eq!(cursor["restart"], true);
        assert!(!Store::new(&root).target.exists());
        return;
    }

    let fixture = Fixture::new();
    fixture.update_source();
    fixture.install().unwrap();
    let bin = fixture._directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for host in ["claude", "codex", "opencode", "pi", "cursor-agent"] {
        let path = bin.join(host);
        fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    // Keep HOME and process-wide discovery isolated without changing this
    // parallel test runner's environment or invoking any installed host.
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "agents::cursor::store::tests::cursor_refresh_and_default_removal_include_an_owned_copy_without_the_host",
        ])
        .env_clear()
        .env(CHILD, "1")
        .env("HOME", fixture._directory.path())
        .env("TMPDIR", fixture._directory.path())
        .env("PATH", &bin)
        .env("SHELL", "/usr/bin/false")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    fixture.clean(false);
}

#[test]
fn cursor_preserves_foreign_and_personally_modified_destinations() {
    for personal in ["foreign", "modified", "extra-file", "extra-directory"] {
        let fixture = Fixture::new();
        if personal == "foreign" {
            fs::create_dir_all(&fixture.store().target).unwrap();
            fs::write(fixture.store().target.join("mine"), "personal").unwrap();
        } else {
            fixture.install().unwrap();
            match personal {
                "modified" => fs::write(
                    fixture.store().target.join("skills/atlas/SKILL.md"),
                    "personal",
                )
                .unwrap(),
                "extra-file" => fs::write(fixture.store().target.join("mine"), "personal").unwrap(),
                "extra-directory" => fs::create_dir(fixture.store().target.join("mine")).unwrap(),
                _ => unreachable!(),
            }
        }
        let before = snapshot(&fixture.store().target).unwrap();
        assert!(fixture.install().is_err(), "{personal}");
        assert!(
            matches!(fixture.remove().unwrap(), Removal::Kept(_)),
            "{personal}"
        );
        assert_eq!(snapshot(&fixture.store().target).unwrap(), before);
    }
}

#[test]
fn cursor_never_follows_a_destination_or_child_symlink() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.store().local).unwrap();
    let personal = fixture._directory.path().join("personal");
    fs::create_dir(&personal).unwrap();
    fs::write(personal.join("mine"), "personal").unwrap();
    symlink(&personal, &fixture.store().target).unwrap();
    assert!(fixture.install().is_err());
    assert!(matches!(fixture.remove().unwrap(), Removal::Kept(_)));
    assert_eq!(
        fs::read_to_string(personal.join("mine")).unwrap(),
        "personal"
    );
    fs::remove_file(&fixture.store().target).unwrap();
    fixture.install().unwrap();
    let managed = fixture.store().target.join("skills/atlas/SKILL.md");
    fs::remove_file(&managed).unwrap();
    symlink(personal.join("mine"), &managed).unwrap();
    assert!(fixture.install().is_err());
    assert!(matches!(fixture.remove().unwrap(), Removal::Kept(_)));
    assert_eq!(
        fs::read_to_string(personal.join("mine")).unwrap(),
        "personal"
    );
}

#[test]
fn cursor_status_only_reads_even_when_a_transaction_is_incomplete() {
    let fixture = Fixture::new();
    assert!(!connected(&fixture.root).unwrap());
    assert!(!fixture.root.exists());
    fixture.install().unwrap();
    fixture.update_source();
    fixture.fail_at("backed-up", false).unwrap_err();
    let before = snapshot(&fixture.root).unwrap();
    assert!(connected(&fixture.root).is_err());
    assert_eq!(snapshot(&fixture.root).unwrap(), before);
    fixture.install().unwrap();
    fixture.clean(true);
}

#[test]
fn cursor_recovers_every_install_and_update_boundary_idempotently() {
    for existing in [false, true] {
        for point in [
            "recorded",
            "before-stage",
            "before-entry",
            "copy-written",
            "before-copy-publication",
            "staged",
            "before-publication",
            "backed-up",
            "published",
            "settled",
        ] {
            if !existing && point == "backed-up" {
                continue;
            }
            let fixture = Fixture::new();
            if existing {
                fixture.install().unwrap();
                fixture.update_source();
            }
            fixture.fail_at(point, false).unwrap_err();
            assert!(connected(&fixture.root).is_err(), "{existing} {point}");
            fixture.install().unwrap();
            assert!(connected(&fixture.root).unwrap(), "{existing} {point}");
            assert_eq!(fixture.install().unwrap(), Removal::Nothing);
            fixture.clean(true);
            fixture.remove().unwrap();
            fixture.clean(false);
        }
    }
}

#[test]
fn cursor_cancellation_before_recording_keeps_an_existing_plugin_unchanged() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    fixture.update_source();
    let before = snapshot(&fixture.store().target).unwrap();
    fixture.fail_at("before-record", false).unwrap_err();
    assert_eq!(snapshot(&fixture.store().target).unwrap(), before);
    assert!(connected(&fixture.root).unwrap());
    fixture.clean(true);
}

#[test]
fn cursor_recovers_every_removal_boundary_without_a_source() {
    for point in ["recorded", "backed-up", "published", "settled"] {
        let fixture = Fixture::new();
        fixture.install().unwrap();
        fixture.fail_at(point, true).unwrap_err();
        fs::remove_dir_all(&fixture.source).unwrap();
        fixture.remove().unwrap();
        assert!(!connected(&fixture.root).unwrap(), "{point}");
        assert_eq!(fixture.remove().unwrap(), Removal::Nothing);
        fixture.clean(false);
    }
}

#[test]
fn cursor_finishes_a_cleanup_interrupted_after_deleting_a_backup_child() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    fixture.update_source();
    fixture.fail_at("published", false).unwrap_err();
    fs::remove_file(fixture.store().backup().join("cursor/mcp.json")).unwrap();
    fixture.install().unwrap();
    fixture.clean(true);
    fixture.remove().unwrap();
    fixture.clean(false);
}

#[test]
fn cursor_keeps_changed_backup_and_replaced_unfinished_copy_inodes() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    fixture.update_source();
    fixture.fail_at("published", false).unwrap_err();
    fs::write(fixture.store().backup().join("cursor/mcp.json"), "personal").unwrap();
    assert!(fixture.install().is_err());
    assert_eq!(
        fs::read_to_string(fixture.store().backup().join("cursor/mcp.json")).unwrap(),
        "personal"
    );

    let fixture = Fixture::new();
    fixture.fail_at("copy-written", false).unwrap_err();
    let transaction: Transaction = read_json(&fixture.store().transaction_path())
        .unwrap()
        .unwrap();
    let path = fixture
        .store()
        .state
        .join(transaction.copying.unwrap().name);
    let original = path.with_extension("original");
    fs::rename(&path, original).unwrap();
    fs::write(&path, "personal").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(fixture.install().is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "personal");
}

#[test]
fn cursor_publication_preserves_an_empty_directory_that_arrives_after_staging() {
    let fixture = Fixture::new();
    let target = fixture.store().target;
    let outcome = change_at(&fixture.root, Some(&fixture.source), &mut |point| {
        if point == "staged" {
            fs::create_dir(&target)?;
        }
        Ok(())
    });
    assert!(outcome.is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert!(
        fixture.remove().is_err(),
        "recovery does not replace the new directory either"
    );
    fs::rename(&target, fixture._directory.path().join("kept-personal")).unwrap();
    fixture.remove().unwrap();
    fixture.clean(false);
}

#[test]
fn cursor_rejects_corrupt_records_and_escaping_record_paths() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    let before = snapshot(&fixture.store().target).unwrap();
    let record = fixture.store().record_path();
    fs::write(&record, "broken").unwrap();
    assert!(connected(&fixture.root).is_err());
    assert!(fixture.remove().is_err());
    let mut invalid = payload(&fixture.source).unwrap();
    invalid.entries.insert(
        "../personal".into(),
        Entry {
            digest: None,
            mode: 0o700,
        },
    );
    write_json(&record, &invalid).unwrap();
    assert!(fixture.install().is_err());
    assert_eq!(snapshot(&fixture.store().target).unwrap(), before);
}

#[test]
fn cursor_can_remove_a_record_whose_payload_was_already_deleted() {
    let fixture = Fixture::new();
    fixture.install().unwrap();
    fs::remove_dir_all(&fixture.store().target).unwrap();
    assert!(!connected(&fixture.root).unwrap());
    assert_eq!(fixture.remove().unwrap(), Removal::Changed);
    fixture.clean(false);
}

#[test]
fn cursor_rejects_linked_sources_and_missing_payload_files() {
    let fixture = Fixture::new();
    let file = fixture.source.join("atlas/cursor/mcp.json");
    fs::remove_file(&file).unwrap();
    assert!(fixture.install().is_err());
    symlink(fixture.source.join("atlas/cursor/hooks.json"), file).unwrap();
    assert!(fixture.install().is_err());
    assert!(!fixture.store().target.exists());
}
