use super::*;
use std::os::unix::fs::PermissionsExt;

fn change_in(home: &Path, atlas: Option<&Path>) -> Result<bool> {
    super::change_in(home, atlas, home)
}

fn written(home: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(settings_in(home)).unwrap()).unwrap()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn settings_edited_after_reading_are_not_replaced() {
    for edited in [
        r#"{"theme":"light"}"#,
        r#"{"mcpServers":{"atlas":{"command":"someone-else"}}}"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let named = settings_in(dir.path());
        fs::create_dir_all(named.parent().unwrap()).unwrap();
        fs::write(&named, "{}").unwrap();
        let (path, before) = read_settings(&named).unwrap();
        let mut settings = parse(before.as_deref()).unwrap();
        changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

        fs::write(&named, edited).unwrap();
        let result = write_settings(dir.path(), &path, before.as_deref(), settings);

        assert!(result.unwrap_err().error.contains("settings changed"));
        assert_eq!(fs::read_to_string(&named).unwrap(), edited);
    }
}

#[test]
fn a_retargeted_settings_link_keeps_both_files_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let named = settings_in(dir.path());
    fs::create_dir_all(named.parent().unwrap()).unwrap();
    let original = dir.path().join("original.json");
    let replacement = dir.path().join("replacement.json");
    for path in [&original, &replacement] {
        fs::write(path, "{}").unwrap();
    }
    std::os::unix::fs::symlink(&original, &named).unwrap();
    let (path, before) = read_settings(&named).unwrap();
    let mut settings = parse(before.as_deref()).unwrap();
    changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

    fs::remove_file(&named).unwrap();
    std::os::unix::fs::symlink(&replacement, &named).unwrap();
    let result = write_settings(dir.path(), &path, before.as_deref(), settings);

    assert!(result.unwrap_err().error.contains("settings changed"));
    assert_eq!(fs::read_link(&named).unwrap(), replacement);
    for path in [&original, &replacement] {
        assert_eq!(fs::read_to_string(path).unwrap(), "{}");
    }
}

#[test]
fn settings_created_after_reading_are_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let named = settings_in(dir.path());
    let (path, before) = read_settings(&named).unwrap();
    let mut settings = parse(before.as_deref()).unwrap();
    changed(&mut settings, Some(Path::new("/x/atlas"))).unwrap();

    fs::create_dir_all(named.parent().unwrap()).unwrap();
    let edited = r#"{"theme":"light"}"#;
    fs::write(&named, edited).unwrap();
    let result = write_settings(dir.path(), &path, before.as_deref(), settings);

    assert!(result.unwrap_err().error.contains("settings changed"));
    assert_eq!(fs::read_to_string(&named).unwrap(), edited);
}

#[test]
fn another_atlas_registration_keeps_settings_unchanged_until_it_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let named = settings_in(dir.path());
    fs::create_dir_all(named.parent().unwrap()).unwrap();
    fs::write(&named, "{}").unwrap();
    let atlas = Path::new("/x/atlas");
    let held = crate::agents::connection_lock_in(dir.path()).unwrap();

    let refused = std::thread::scope(|scope| {
        scope
            .spawn(|| change_in(dir.path(), Some(atlas)))
            .join()
            .unwrap()
    });
    assert!(
        refused
            .unwrap_err()
            .error
            .contains("Another agent connection")
    );
    assert_eq!(fs::read_to_string(&named).unwrap(), "{}");
    assert!(!named.with_extension("json.before-atlas").exists());

    drop(held);
    assert!(change_in(dir.path(), Some(atlas)).unwrap());
    assert!(connected_in(dir.path()));
}

#[test]
fn connecting_adds_atlas_and_keeps_every_other_setting_and_the_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let before = r#"{"mcpServers": {"other": {"command": "x"}}, "theme": "dark"}"#;
    fs::write(&path, before).unwrap();
    let atlas = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");

    assert!(change_in(home, Some(atlas)).unwrap());
    let settings = written(home);
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["mcpServers"]["other"]["command"], "x");
    assert_eq!(settings["mcpServers"]["atlas"], entry(atlas));
    assert!(connected_in(home));
    let kept = path.with_extension("json.before-atlas");
    assert_eq!(fs::read_to_string(&kept).unwrap(), before);

    assert!(
        !change_in(home, Some(atlas)).unwrap(),
        "nothing to change twice"
    );
    assert!(change_in(home, None).unwrap());
    assert!(!connected_in(home));
    assert_eq!(written(home)["mcpServers"]["other"]["command"], "x");
    assert!(!change_in(home, None).unwrap());
    assert_eq!(
        fs::read_to_string(&kept).unwrap(),
        before,
        "the copy is never replaced"
    );
}

#[test]
fn a_missing_settings_file_is_made_privately() {
    let dir = tempfile::tempdir().unwrap();
    assert!(change_in(dir.path(), Some(Path::new("/usr/local/bin/atlas"))).unwrap());
    assert!(connected_in(dir.path()));
    let path = settings_in(dir.path());
    assert!(!path.with_extension("json.before-atlas").exists());
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn the_file_keeps_its_permissions_and_its_copy_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{"mcpServers": {"tokens": {"env": {"KEY": "secret"}}}}"#,
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

    assert!(change_in(home, Some(Path::new("/x/atlas"))).unwrap());
    assert_eq!(mode(&path), 0o640);
    assert_eq!(mode(&path.with_extension("json.before-atlas")), 0o600);
}

#[test]
fn a_linked_settings_file_is_changed_where_it_points() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let real = dir.path().join("dotfiles.json");
    fs::write(&real, "{}").unwrap();
    std::os::unix::fs::symlink(&real, &path).unwrap();

    assert!(change_in(home, Some(Path::new("/x/atlas"))).unwrap());
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(fs::read_to_string(&real).unwrap().contains("atlas"));
}

#[test]
fn an_unresolved_settings_link_is_refused_and_kept() {
    if !crate::test_support::isolated(
        "claude_app::tests::an_unresolved_settings_link_is_refused_and_kept",
    ) {
        return;
    }
    for target in ["missing.json", "claude_desktop_config.json"] {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let path = settings_in(home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, &path).unwrap();

        for atlas in [Some(Path::new("/x/atlas")), None] {
            let error = change_in(home, atlas).unwrap_err();
            assert!(
                error.error.contains(&path.display().to_string()),
                "{error:?}"
            );
            assert!(error.error.contains("Could not resolve"));
            assert_eq!(fs::read_link(&path).unwrap(), Path::new(target));
            assert!(!path.with_extension("json.before-atlas").exists());
            assert!(!path.with_extension("json.atlas-partial").exists());
        }
        assert!(!path.parent().unwrap().join("missing.json").exists());
    }
}

#[test]
fn removing_reports_invalid_settings_and_keeps_the_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    for before in [
        r#"{"mcpServers": {"atlas": {"command": "/x/atlas", "args": ["mcp"]}}"#,
        "[]",
        r#"{"mcpServers": null}"#,
        r#"{"mcpServers": []}"#,
        r#"{"mcpServers": "invalid"}"#,
    ] {
        fs::write(&path, before).unwrap();
        let error = change_in(home, None).unwrap_err();
        assert_eq!(error.code, "agents");
        assert!(error.error.contains(&path.display().to_string()));
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert!(!path.with_extension("json.before-atlas").exists());
    }
}

#[test]
fn removing_an_absent_entry_keeps_other_settings_and_creates_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    assert!(!change_in(home, None).unwrap());
    assert!(!path.exists());

    fs::create_dir_all(path.parent().unwrap()).unwrap();
    for before in [
        r#"{"theme": "dark"}"#,
        r#"{"mcpServers": {"other": {"command": "x"}}, "theme": "dark"}"#,
    ] {
        fs::write(&path, before).unwrap();
        assert!(!change_in(home, None).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert!(!path.with_extension("json.before-atlas").exists());
    }
}

#[test]
fn another_programs_atlas_entry_and_a_broken_file_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path();
    let path = settings_in(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let atlas = Path::new("/x/atlas");

    let theirs = r#"{"mcpServers": {"atlas": {"command": "npx", "args": ["atlas-maps"]}}}"#;
    fs::write(&path, theirs).unwrap();
    assert_eq!(change_in(home, Some(atlas)).unwrap_err().code, "agents");
    assert!(!change_in(home, None).unwrap(), "removing leaves it alone");
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    assert!(!connected_in(home));

    fs::write(&path, "{ not json").unwrap();
    assert_eq!(change_in(home, Some(atlas)).unwrap_err().code, "agents");
    assert_eq!(change_in(home, None).unwrap_err().code, "agents");
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
}
