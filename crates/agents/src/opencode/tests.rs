use super::*;
use std::os::unix::fs::symlink;

#[test]
fn opencode_preserves_regular_files_and_unrelated_links() {
    let temp = tempfile::tempdir().unwrap();
    let link = temp.path().join("atlas.js");
    fs::write(&link, "personal").unwrap();
    assert!(change_at(&link, Some(temp.path())).is_err());
    assert!(matches!(change_at(&link, None).unwrap(), Removal::Kept(_)));
    assert_eq!(fs::read_to_string(&link).unwrap(), "personal");
    fs::remove_file(&link).unwrap();
    symlink("elsewhere.js", &link).unwrap();
    assert!(change_at(&link, Some(temp.path())).is_err());
    assert!(matches!(change_at(&link, None).unwrap(), Removal::Kept(_)));
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("elsewhere.js"));
}

/// An Atlas marketplace at `root`, its plugin's manifest naming Atlas's
/// homepage.
fn atlas_marketplace(root: &Path) {
    fs::create_dir_all(root.join("atlas/.codex-plugin")).unwrap();
    fs::write(
        root.join("atlas/.codex-plugin/plugin.json"),
        serde_json::json!({"name":"atlas", "homepage":super::super::HOMEPAGE}).to_string(),
    )
    .unwrap();
}

#[test]
fn opencode_resolves_relative_links_and_removes_only_atlas_own() {
    let temp = tempfile::tempdir().unwrap();
    let link = temp.path().join("atlas.js");
    let old = "old/Atlas.app/Contents/Resources/agents/atlas/opencode/atlas.js";
    symlink(old, &link).unwrap();
    let agents = temp.path().join("old/Atlas.app/Contents/Resources/agents");
    assert_eq!(source_at(&link).unwrap(), Source::Folder(agents.clone()));
    assert!(
        matches!(change_at(&link, None).unwrap(), Removal::Kept(_)),
        "a removed app's link has no manifest to show it is Atlas's"
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new(old));

    atlas_marketplace(&agents);
    assert_eq!(change_at(&link, None).unwrap(), Removal::Changed);
    assert_eq!(change_at(&link, None).unwrap(), Removal::Nothing);
}

#[test]
fn opencode_installs_atomically_and_replaces_only_its_own_old_link() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("agents");
    let plugin = root.join("atlas/opencode/atlas.js");
    fs::create_dir_all(plugin.parent().unwrap()).unwrap();
    fs::write(&plugin, "export default {}").unwrap();
    atlas_marketplace(&root);
    let link = temp.path().join("config/plugins/atlas.js");
    assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Changed);
    assert!(same_place(&fs::read_link(&link).unwrap(), &plugin));
    assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Nothing);
    fs::remove_file(&link).unwrap();
    let old = temp.path().join("Old/Atlas.app/Contents/Resources/agents");
    atlas_marketplace(&old);
    symlink(old.join("atlas/opencode/atlas.js"), &link).unwrap();
    assert_eq!(change_at(&link, Some(&root)).unwrap(), Removal::Changed);
    assert!(same_place(&fs::read_link(&link).unwrap(), &plugin));
    assert_eq!(fs::read_dir(link.parent().unwrap()).unwrap().count(), 1);

    fs::remove_file(&link).unwrap();
    symlink(
        "/missing/Atlas.app/Contents/Resources/agents/atlas/opencode/atlas.js",
        &link,
    )
    .unwrap();
    assert!(
        change_at(&link, Some(&root)).is_err(),
        "a removed app's link is not replaced"
    );
}
