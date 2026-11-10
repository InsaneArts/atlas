use super::*;
use crate::{
    client::AppList,
    test_support::{Requester, WATCHDOG},
};

#[test]
fn only_a_real_private_folder_of_this_account_counts() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("own");
    fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
    assert!(private_folder(&folder));

    fs::set_permissions(&folder, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(!private_folder(&folder), "writable by others");

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&folder, &link).unwrap();
    assert!(!private_folder(&link), "a link is never followed");
    assert!(!private_folder(&dir.path().join("missing")));
}

#[test]
fn a_symlinked_parent_keeps_personal_files_out_of_cleanup() {
    let shared = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let open = projects.path().join("Open");
    fs::DirBuilder::new().mode(0o700).create(&open).unwrap();
    let copy = copy_folder(&open, "a", "note.txt");
    age(&copy, 8);
    let atlas = shared.path().join("Atlas");
    std::os::unix::fs::symlink(projects.path(), &atlas).unwrap();
    let root = atlas.join("Open");
    assert!(private_folder(&root), "Open alone passes the checks");

    assert_eq!(
        remove_copies_in(&root, "note", &[], None),
        Removed::default()
    );
    prune_in(&root, &[]);
    let owned = root_if_owned_in(&root);
    if let Some(root) = &owned {
        fs::remove_dir_all(root).unwrap();
    }
    assert!(owned.is_none(), "uninstall must refuse the root");
    assert_eq!(fs::read_to_string(copy.join("note.txt")).unwrap(), "hello");
}

#[test]
fn a_real_private_parent_allows_cleanup() {
    let shared = tempfile::tempdir().unwrap();
    let atlas = shared.path().join("Atlas");
    fs::DirBuilder::new().mode(0o700).create(&atlas).unwrap();
    let root = atlas.join("Open");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let closed = copy_folder(&root, "a", "note.txt");
    let old = copy_folder(&root, "b", "old.txt");
    age(&old, 8);

    assert_eq!(root_if_owned_in(&root), Some(root.clone()));
    assert_eq!(
        remove_copies_in(&root, "note", &[], None).folders,
        [closed.to_string_lossy().into_owned()]
    );
    prune_in(&root, &[]);
    assert!(!closed.exists());
    assert!(!old.exists());
    fs::remove_dir_all(root_if_owned_in(&root).unwrap()).unwrap();
    assert!(!root.exists());
}

#[test]
fn a_copy_gets_its_own_folder_and_nobody_else_may_write_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let source = dir.path().join("note.txt");
    fs::write(&source, "hello").unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o666)).unwrap();

    let first = copy(&source, &root, &Token::new()).unwrap();
    let second = copy(&source, &root, &Token::new()).unwrap();
    assert_ne!(first.parent(), second.parent(), "never replaced");
    assert_eq!(fs::read_to_string(&first).unwrap(), "hello");
    let mode = fs::metadata(&first).unwrap().mode() & 0o777;
    assert_eq!(mode & 0o022, 0, "group and others cannot write: {mode:o}");
}

#[test]
fn the_grant_is_recognised_once_set() {
    let dir = tempfile::tempdir().unwrap();
    let me = nix::unistd::User::from_uid(nix::unistd::getuid())
        .unwrap()
        .unwrap()
        .name;
    assert!(!granted(dir.path(), &me));

    let grant = format!(
        "{me} allow list,search,readattr,readextattr,readsecurity,read,execute,file_inherit,directory_inherit"
    );
    let grant_with_write = format!("{grant},write,add_file");

    quiet(
        "/bin/chmod",
        &["+a", &grant],
        dir.path(),
        atlas_core::bounded::FILE,
    )
    .unwrap();
    assert!(granted(dir.path(), &me));
    assert!(!granted(dir.path(), "someone-else"));

    // A grant that also lets the helper write is not the one wanted.
    let other = tempfile::tempdir().unwrap();
    quiet(
        "/bin/chmod",
        &["+a", &grant_with_write],
        other.path(),
        atlas_core::bounded::FILE,
    )
    .unwrap();
    assert!(!granted(other.path(), &me));
}

/// An app bundle at `path` that calls itself Runner (runner on disk).
fn runner(path: &Path) {
    fs::create_dir_all(path.join("Contents")).unwrap();
    fs::write(
        path.join("Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>runner</string><key>CFBundleName</key><string>Runner</string></dict></plist>"#,
    )
    .unwrap();
}

/// A copy folder under `root` holding `name` (an app or a file).
fn copy_folder(root: &Path, folder: &str, name: &str) -> PathBuf {
    let folder = root.join(folder);
    fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();

    if name.ends_with(".app") {
        runner(&folder.join(name));
    } else {
        fs::write(folder.join(name), "hello").unwrap();
    }

    folder
}

/// Make `folder` look `days` old.
fn age(folder: &Path, days: u64) {
    let then = std::time::SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60);
    fs::File::open(folder).unwrap().set_modified(then).unwrap();
}

/// A copy is found by every name its app may run as, in any case, and
/// is never removed while an app of one of those names still runs.
#[test]
fn copies_in_use_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let app = copy_folder(&root, "a", "Long Runner.app");
    let opened = copy_folder(&root, "b", "note.txt");
    fs::write(opened.join(OPENED_WITH), "TextEdit").unwrap();
    let other = copy_folder(&root, "c", "other.txt");

    for name in ["runner", "long runner"] {
        assert_eq!(copies_of(&root, name), std::slice::from_ref(&app), "{name}");
    }

    assert_eq!(copies_of(&root, "TEXTEDIT"), std::slice::from_ref(&opened));
    assert!(copies_of(&root, "Preview").is_empty());
    assert!(in_use(&app, apps(&["RUNNER"]).rows()));
    assert!(
        in_use(&opened, apps(&["TextEdit"]).rows()),
        "opened with a running app"
    );
    assert!(!in_use(&other, apps(&["TextEdit", "Runner"]).rows()));

    // `close --app runner` while Runner still runs keeps its copy.
    assert_eq!(
        remove_copies_in(&root, "runner", apps(&["Finder", "Runner"]).rows(), None),
        Removed::default()
    );
    assert!(app.exists());
    let removed = remove_copies_in(&root, "runner", apps(&["Finder"]).rows(), None);
    assert_eq!(removed.folders, [app.to_string_lossy().into_owned()]);
    assert!(!app.exists());

    // Old copies go unless in use; new ones stay.
    age(&opened, 8);
    age(&other, 8);
    let new = copy_folder(&root, "d", "new.txt");
    prune_in(&root, apps(&["textedit"]).rows());
    assert!(opened.exists(), "in use");
    assert!(!other.exists(), "old and unused");
    assert!(new.exists(), "recent");
    prune_in(&root, &[]);
    assert!(!opened.exists());
}

fn key(id: &str) -> Key {
    Key::of(crate::host_session::Host::Codex, id)
}

/// Apps of these names running from somewhere outside any copy.
fn apps(names: &[&str]) -> AppList {
    let apps: Vec<_> = names
        .iter()
        .zip(1..)
        .map(|(name, pid)| {
            serde_json::json!({"pid": pid, "name": name,
                "executable": format!("/Applications/{name}.app/Contents/MacOS/{name}")})
        })
        .collect();
    AppList::decode(serde_json::json!({"ok": true, "apps": apps})).unwrap()
}

/// A copy root under `dir` holding one copy opened with TextEdit.
fn one_copy(dir: &Path) -> (PathBuf, PathBuf) {
    let root = dir.join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let copy = copy_folder(&root, "copy", "notes.rtf");
    fs::write(copy.join(OPENED_WITH), "TextEdit").unwrap();
    (root, copy)
}

#[test]
fn an_unreadable_label_keeps_the_copy_for_a_session() {
    type Make = fn(&Path);
    let unreadable: [(&str, Make); 5] = [
        ("a folder", |label| fs::create_dir(label).unwrap()),
        ("a link", |label| {
            std::os::unix::fs::symlink("/etc/hosts", label).unwrap();
        }),
        ("empty", |label| fs::write(label, "").unwrap()),
        ("two lines", |label| {
            fs::write(label, format!("{}\n{}\n", key("left"), key("left"))).unwrap();
        }),
        ("not a key", |label| fs::write(label, "../x\n").unwrap()),
    ];

    for (what, make) in unreadable {
        let dir = tempfile::tempdir().unwrap();
        let (root, copy) = one_copy(dir.path());
        make(&copy.join(SESSION));
        assert_eq!(labelled(&copy), Label::Unreadable, "{what}");

        let removed = remove_copies_in(&root, "TextEdit", &[], Some(&key("left")));
        assert_eq!(
            (removed.folders.len(), removed.kept_others),
            (0, 1),
            "{what}"
        );
        assert!(copy.exists(), "{what}");

        let removed = remove_copies_in(&root, "TextEdit", &[], None);
        assert_eq!(removed.folders.len(), 1, "without a session: {what}");
    }
}

#[test]
fn a_copy_an_app_runs_from_is_in_use_even_without_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let app = copy_folder(&root, "app", "Long Runner.app");
    let beside = copy_folder(&root, "app-beside", "note.txt");
    let listed = AppList::decode(serde_json::json!({"ok": true, "apps": [{"pid": 1,
        "executable": app.join("Long Runner.app/Contents/MacOS/runner")}]}))
    .unwrap();
    let running = listed.rows();

    assert!(in_use(&app, running));
    assert!(!in_use(&beside, running), "a folder whose name it begins");
    assert_eq!(
        remove_copies_in(&root, "runner", running, None),
        Removed::default()
    );
    age(&app, 8);
    prune_in(&root, running);
    assert!(app.exists());
}

#[test]
fn a_copy_that_cannot_be_removed_is_reported_not_dropped() {
    // Permissions do not hold root back, so nothing would fail.
    if nix::unistd::geteuid().is_root() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (root, copy) = one_copy(dir.path());
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o500)).unwrap();

    let removed = remove_copies_in(&root, "TextEdit", &[], None);
    fs::set_permissions(&copy, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(removed.folders.is_empty());
    assert_eq!(removed.failed.len(), 1);
    assert_eq!(removed.failed[0].path, copy.to_string_lossy());
    assert!(removed.failed[0].error.contains("ermission"));
    assert!(copy.exists());
}

#[test]
fn a_copy_folder_records_its_session_out_of_the_helpers_sight() {
    let dir = tempfile::tempdir().unwrap();
    let folder = copy_folder(dir.path(), "a", "note.txt");
    fs::write(folder.join(OPENED_WITH), "TextEdit").unwrap();
    label(&folder, &key("right")).unwrap();

    let session = folder.join(SESSION);
    assert_eq!(
        fs::read_to_string(&session).unwrap(),
        format!("{}\n", key("right"))
    );
    assert_eq!(fs::metadata(&session).unwrap().mode() & 0o777, 0o600);
    assert_eq!(labelled(&folder), Label::Session(key("right")));
    assert_eq!(entries(&folder), [folder.join("note.txt")]);
    assert_eq!(folder_names(&folder), ["TextEdit", "note"]);
    assert!(label(&folder, &key("other")).is_err(), "never replaced");
}

#[test]
fn close_keeps_another_sessions_copies_and_counts_them() {
    let made = |dir: &Path| {
        let root = dir.join("root");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let own = copy_folder(&root, "own", "own.txt");
        let other = copy_folder(&root, "other", "other.txt");
        let nobody = copy_folder(&root, "nobody", "nobody.txt");
        let busy = copy_folder(&root, "busy", "busy.txt");
        for folder in [&own, &other, &nobody, &busy] {
            fs::write(folder.join(OPENED_WITH), "TextEdit").unwrap();
        }
        label(&own, &key("left")).unwrap();
        label(&other, &key("right")).unwrap();
        fs::write(busy.join(OPENED_WITH), "TextEdit\nBusy").unwrap();
        (root, [own, other, nobody, busy])
    };

    let dir = tempfile::tempdir().unwrap();
    let (root, [own, other, nobody, busy]) = made(dir.path());
    let removed = remove_copies_in(
        &root,
        "TextEdit",
        apps(&["Busy"]).rows(),
        Some(&key("left")),
    );
    assert_eq!(removed.kept_others, 1);
    let mut folders = removed.folders.clone();
    folders.sort();
    let mut expected = [own, nobody].map(|folder| folder.to_string_lossy().into_owned());
    expected.sort();
    assert_eq!(folders, expected);
    assert!(other.exists() && busy.exists());

    let dir = tempfile::tempdir().unwrap();
    let (root, [.., busy]) = made(dir.path());
    let removed = remove_copies_in(&root, "TextEdit", apps(&["Busy"]).rows(), None);
    assert_eq!(
        (removed.folders.len(), removed.kept_others),
        (3, 0),
        "without a session every copy not in use goes"
    );
    assert!(busy.exists(), "one in use always stays");
}

#[test]
fn prune_ignores_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let other = copy_folder(&root, "other", "other.txt");
    label(&other, &key("right")).unwrap();
    age(&other, 8);

    prune_in(&root, &[]);
    assert!(!other.exists());
}

#[test]
fn a_copy_folder_is_named_by_its_token_and_its_app_never_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let folder = |name: &str| {
        let token = Token::new();
        copy_folder(dir.path(), token.as_str(), name);
        token
    };
    let token = folder("Quarterly Plan.txt");
    fs::write(
        dir.path().join(token.as_str()).join(OPENED_WITH),
        "TextEdit\nTextEdit Helper",
    )
    .unwrap();
    let app = folder("Long Runner.app");
    let bare = folder("notes.txt");

    assert_eq!(copy_app(dir.path(), &token).as_deref(), Some("TextEdit"));
    assert_eq!(copy_app(dir.path(), &app).as_deref(), Some("Long Runner"));
    assert_eq!(copy_app(dir.path(), &bare), None);
    assert!(copy_exists(dir.path(), &token));
    assert!(!copy_exists(dir.path(), &Token::new()));

    let path = format!("{ROOT}/{token}/Quarterly Plan.txt");
    assert_eq!(token_of(&path), Some(token.clone()));
    assert_eq!(token_of(&format!("{ROOT}/{token}")), Some(token));
    assert_eq!(token_of("/tmp/Quarterly Plan.txt"), None);
    assert_eq!(token_of(&format!("{ROOT}/not-a-token/x")), None);
    assert_eq!(token_of(&format!("{ROOT}/../{}", Token::new())), None);
}

/// A private copy root and an output folder under `dir`, and the claims
/// of a task of a fresh session kept there, with that task.
fn claiming(dir: &Path) -> (PathBuf, PathBuf, Claims, crate::task::Task) {
    let root = dir.join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let output = dir.join("output");
    fs::create_dir(&output).unwrap();
    let task = crate::task::Task::of(&key("claiming"), Some("probe")).unwrap();
    let store = crate::task::store::Store::at(&output);
    (root, output, Claims::in_store(store, task.clone()), task)
}

/// The copies of `sources`, as `hand_over` places them once checked.
fn to_copy(sources: &[&Path]) -> Vec<Placed> {
    sources
        .iter()
        .map(|path| Placed::Copy {
            path: path.to_path_buf(),
            token: Token::new(),
        })
        .collect()
}

/// The states `output`'s record of `task` holds, in claim order.
fn states(output: &Path, task: &crate::task::Task) -> Vec<State> {
    crate::task::store::Store::at(output)
        .load(task.key())
        .unwrap()
        .claims()
        .map(|(_, state)| state)
        .collect()
}

#[test]
fn pruning_names_each_copy_it_removed_by_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let (old, recent) = (Token::new(), Token::new());
    let gone = copy_folder(&root, old.as_str(), "old.txt");
    age(&gone, 8);
    copy_folder(&root, recent.as_str(), "new.txt");

    assert_eq!(prune_in(&root, &[]), [old]);
    assert!(!gone.exists());
}

/// When a pruning test prunes: a fixed time.
fn pruned_at() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

/// Make `folder` last changed `days` before [`pruned_at`].
fn changed_before(folder: &Path, days: u64) {
    let then = pruned_at() - Duration::from_secs(days * 24 * 60 * 60);
    fs::File::open(folder).unwrap().set_modified(then).unwrap();
}

/// A private copy root under `dir` holding one expired copy folder, and
/// that folder's token and path.
fn expired_copy(dir: &Path) -> (PathBuf, Token, PathBuf) {
    let root = dir.join("root");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let token = Token::new();
    let folder = copy_folder(&root, token.as_str(), "old.txt");
    changed_before(&folder, 8);
    (root, token, folder)
}

#[test]
fn pruning_keeps_a_copy_folder_replaced_after_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let (root, _, folder) = expired_copy(&base);
    let aside = base.join("aside");

    let removed = prune_at(&root, &[], pruned_at(), |judged| {
        assert_eq!(judged, folder);
        fs::rename(judged, &aside).unwrap();
        fs::DirBuilder::new().mode(0o700).create(judged).unwrap();
        fs::write(judged.join("sentinel"), "fresh").unwrap();
    });

    assert!(removed.is_empty(), "{removed:?}");
    assert_eq!(fs::read_to_string(aside.join("old.txt")).unwrap(), "hello");
    assert_eq!(
        fs::read_to_string(folder.join("sentinel")).unwrap(),
        "fresh"
    );
}

#[test]
fn pruning_keeps_a_copy_folder_replaced_by_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let (root, _, folder) = expired_copy(&base);
    let aside = base.join("aside");

    let removed = prune_at(&root, &[], pruned_at(), |judged| {
        fs::rename(judged, &aside).unwrap();
        std::os::unix::fs::symlink(&aside, judged).unwrap();
    });

    assert!(removed.is_empty(), "{removed:?}");
    assert_eq!(fs::read_to_string(aside.join("old.txt")).unwrap(), "hello");
    assert!(fs::symlink_metadata(&folder).unwrap().is_symlink());
}

#[test]
fn pruning_removes_only_an_unchanged_expired_copy_nothing_uses() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let (root, token, folder) = expired_copy(&base);
    let recent = copy_folder(&root, Token::new().as_str(), "new.txt");
    changed_before(&recent, 6);
    let used = copy_folder(&root, Token::new().as_str(), "used.txt");
    fs::write(used.join(OPENED_WITH), "TextEdit").unwrap();
    changed_before(&used, 8);
    let outside = copy_folder(&base, "outside", "kept.txt");
    changed_before(&outside, 8);
    let linked = root.join(Token::new().as_str());
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    let mut judged = Vec::new();

    let removed = prune_at(&root, apps(&["TextEdit"]).rows(), pruned_at(), |folder| {
        judged.push(folder.to_owned());
    });

    assert_eq!(removed, [token]);
    assert_eq!(judged, std::slice::from_ref(&folder));
    assert!(!folder.exists());
    assert!(recent.exists() && used.exists(), "recent and in use");
    assert!(fs::symlink_metadata(&linked).is_ok() && outside.join("kept.txt").exists());
}

#[test]
fn every_copy_is_claimed_in_one_write_before_any_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let owner = Owner {
        session: None,
        claims: &claims,
        opened_with: &[],
    };
    let sources = [dir.path().join("a.txt"), dir.path().join("b.txt")];
    for source in &sources {
        fs::write(source, "hello").unwrap();
    }
    let placed = to_copy(&[&sources[0], &sources[1]]);

    let mut seen = Vec::new();
    let handed = handed_in(
        &root,
        placed,
        &owner,
        |path, root, token| {
            seen.push(states(&output, &task));
            copy(path, root, token)
        },
        |folder| fs::symlink_metadata(folder),
    )
    .unwrap();

    assert_eq!(seen, [[State::Intent, State::Intent]; 2]);
    assert_eq!(handed.paths.len(), 2);
    assert_eq!(states(&output, &task), [State::Made, State::Made]);
}

#[test]
fn a_failed_copy_takes_the_ones_before_it_and_each_is_settled_once_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let session = key("claiming");
    let owner = Owner {
        session: Some(&session),
        claims: &claims,
        opened_with: &["TextEdit".to_owned()],
    };
    let made = dir.path().join("made.txt");
    fs::write(&made, "hello").unwrap();
    let missing = dir.path().join("missing.txt");
    let placed = to_copy(&[&made, &missing, &made]);

    let error = handed_in(&root, placed, &owner, copy, |folder| {
        fs::symlink_metadata(folder)
    })
    .unwrap_err();

    assert!(error.error.contains("Could not copy"), "{error}");
    assert!(error.details.get("leftover").is_none());
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        0,
        "the first copy went"
    );
    assert_eq!(states(&output, &task), [State::Gone; 3]);
}

#[test]
fn a_failed_copy_whose_folder_stays_is_left_claimed_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let owner = Owner {
        session: None,
        claims: &claims,
        opened_with: &[],
    };
    let source = dir.path().join("note.txt");
    fs::write(&source, "hello").unwrap();

    // The copy fails after making its folder, and cannot remove it.
    let error = handed_in(
        &root,
        to_copy(&[&source]),
        &owner,
        |_, root, token| {
            fs::create_dir(root.join(token.as_str())).unwrap();
            Err(problem("Could not copy"))
        },
        |folder| fs::symlink_metadata(folder),
    )
    .unwrap_err();

    let leftover = error.details["leftover"].as_array().unwrap();
    assert_eq!(leftover.len(), 1);
    assert!(Path::new(leftover[0].as_str().unwrap()).exists());
    assert_eq!(states(&output, &task), [State::Intent]);
}

/// Look at a path as `fs::symlink_metadata` does, except that a copy
/// folder under `root` is looked at with `fault` once it exists.
fn faulty(
    root: &Path,
    fault: std::io::ErrorKind,
) -> impl Fn(&Path) -> std::io::Result<fs::Metadata> + '_ {
    move |path| {
        let metadata = fs::symlink_metadata(path)?;
        if path.parent() == Some(root) {
            return Err(std::io::Error::from(fault));
        }
        Ok(metadata)
    }
}

#[test]
fn an_unreadable_copy_folder_keeps_its_task_claim() {
    use std::io::ErrorKind;
    for fault in [ErrorKind::PermissionDenied, ErrorKind::Other] {
        let dir = tempfile::tempdir().unwrap();
        let (root, output, claims, task) = claiming(dir.path());
        let owner = Owner {
            session: None,
            claims: &claims,
            opened_with: &[],
        };
        let source = dir.path().join("note.txt");
        fs::write(&source, "hello").unwrap();

        let error = handed_in(
            &root,
            to_copy(&[&source]),
            &owner,
            copy,
            faulty(&root, fault),
        )
        .unwrap_err();
        let leftover = error.details["leftover"].as_array().unwrap();
        assert_eq!(leftover.len(), 1, "{fault:?}");
        assert!(Path::new(leftover[0].as_str().unwrap()).exists());
        assert!(
            error.error.contains(leftover[0].as_str().unwrap()),
            "{error}"
        );
        assert_eq!(states(&output, &task), [State::Intent], "{fault:?}");

        let error = handed_in(
            &root,
            to_copy(&[&source]),
            &owner,
            |_, root, token| {
                fs::create_dir(root.join(token.as_str())).unwrap();
                Err(problem("Could not copy"))
            },
            faulty(&root, fault),
        )
        .unwrap_err();
        assert!(error.error.starts_with("Could not copy"), "{error}");
        let leftover = error.details["leftover"].as_array().unwrap();
        assert_eq!(leftover.len(), 1, "{fault:?}");
        assert!(Path::new(leftover[0].as_str().unwrap()).exists());
        assert_eq!(
            states(&output, &task),
            [State::Intent, State::Intent],
            "{fault:?}"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let owner = Owner {
        session: None,
        claims: &claims,
        opened_with: &[],
    };
    let error = handed_in(
        &root,
        to_copy(&[&dir.path().join("note.txt")]),
        &owner,
        |_, _, _| Err(problem("Could not copy")),
        faulty(&root, ErrorKind::PermissionDenied),
    )
    .unwrap_err();
    assert!(error.details.get("leftover").is_none(), "{error}");
    assert_eq!(states(&output, &task), [State::Gone], "a folder never made");
}

#[test]
fn the_record_keeps_no_file_names_or_paths() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let owner = Owner {
        session: None,
        claims: &claims,
        opened_with: &[],
    };
    let source = dir.path().join("Secret plan.rtf");
    fs::write(&source, "hello").unwrap();

    let handed = handed_in(&root, to_copy(&[&source]), &owner, copy, |folder| {
        fs::symlink_metadata(folder)
    })
    .unwrap();

    assert!(handed.paths[0].ends_with("Secret plan.rtf"));
    let record = fs::read_to_string(output.join(format!("tasks/{}.json", task.key()))).unwrap();
    assert!(!record.contains("Secret"), "{record}");
    assert!(
        !record.contains(&dir.path().to_string_lossy().into_owned()),
        "{record}"
    );
    assert!(record.contains(token_of_copy(&handed.paths[0]).as_str()));
}

#[test]
fn a_copy_is_said_by_where_it_came_from_and_went_and_knows_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (root, _output, claims, _task) = claiming(dir.path());
    let owner = Owner {
        session: None,
        claims: &claims,
        opened_with: &[],
    };
    let source = dir.path().join("plan.rtf");
    fs::write(&source, "hello").unwrap();

    let handed = handed_in(&root, to_copy(&[&source]), &owner, copy, |folder| {
        fs::symlink_metadata(folder)
    })
    .unwrap();

    let copied = &handed.copied[0];
    assert_eq!(copied.token, token_of_copy(&handed.paths[0]));
    assert_eq!(
        serde_json::to_string(&handed.copied).unwrap(),
        format!(
            r#"[{{"from":"{}","to":"{}"}}]"#,
            source.display(),
            handed.paths[0]
        )
    );
}

/// The token of the copy folder `copied` lies in, wherever the root is.
fn token_of_copy(copied: &str) -> Token {
    folder_token(Path::new(copied).parent().unwrap()).unwrap()
}

#[test]
fn names_come_from_the_file_and_the_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("Long Runner.app");
    runner(&app);
    let names = names(&app);

    for name in ["Long Runner", "Runner", "runner"] {
        assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
    }
}

/// A private copy root in `dir` holding a copy folder for `session`,
/// and that folder's token.
fn labelled_copy(dir: &Path, session: &Key) -> (PathBuf, Token) {
    let root = dir.join("copies");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let token = Token::new();
    let folder = root.join(token.as_str());
    fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
    fs::write(folder.join("notes.rtf"), "hello").unwrap();
    label(&folder, session).unwrap();
    (root, token)
}

#[test]
fn a_copy_this_session_may_take_is_removed_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let session = Key::of(crate::host_session::Host::Codex, "removing");
    let (root, token) = labelled_copy(&base, &session);

    assert_eq!(
        remove_copy(&root, &token, &session, &[]),
        CopyRemoval::Removed
    );
    assert!(
        fs::read_dir(&root).unwrap().next().is_none(),
        "nothing moved anywhere"
    );
    assert_eq!(
        remove_copy(&root, &token, &session, &[]),
        CopyRemoval::AlreadyGone
    );
}

#[test]
fn a_copy_labelled_for_another_session_or_unreadably_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let (own, other) = (
        Key::of(crate::host_session::Host::Codex, "own"),
        Key::of(crate::host_session::Host::Codex, "other"),
    );
    let (root, theirs) = labelled_copy(&base, &other);
    assert_eq!(
        remove_copy(&root, &theirs, &own, &[]),
        CopyRemoval::AnotherSession
    );
    let linked = Token::new();
    let folder = root.join(linked.as_str());
    fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", folder.join(SESSION)).unwrap();
    assert_eq!(
        remove_copy(&root, &linked, &own, &[]),
        CopyRemoval::Unreadable
    );
    assert!(root.join(theirs.as_str()).exists() && folder.exists());
}

#[test]
fn a_copy_folder_swapped_after_it_was_opened_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let session = Key::of(crate::host_session::Host::Codex, "swapped");
    let (root, token) = labelled_copy(&base, &session);
    let parent = open_folder(&root).unwrap();
    let copy = OpenCopy::open(&parent, token.as_str()).unwrap().unwrap();
    assert!(copy.may_take(Some(&session)).is_ok());

    // Another folder takes its place after it was judged.
    let path = root.join(token.as_str());
    fs::rename(&path, base.join("judged")).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    assert!(!copy.still_at(&path), "the path names another folder now");
    assert!(copy.still_at(&base.join("judged")));
}

#[test]
fn a_copy_quit_target_is_only_one_this_session_may_take() {
    let dir = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap();
    let (own, other) = (
        Key::of(crate::host_session::Host::Codex, "own"),
        Key::of(crate::host_session::Host::Codex, "other"),
    );
    let (root, token) = labelled_copy(&base, &own);
    let folder = root.join(token.as_str());
    let metadata = fs::symlink_metadata(&folder).unwrap();
    assert_eq!(
        copy_node(&root, &token, &own),
        Some(format!("{}:{}", metadata.dev(), metadata.ino()))
    );
    assert_eq!(copy_node(&root, &token, &other), None);
}

/// Two files under `dir` for the helper, which it cannot read.
fn unreadable_pair(dir: &Path) -> Vec<Target> {
    ["a.txt", "b.txt"]
        .into_iter()
        .map(|name| {
            let path = dir.join(name);
            fs::write(&path, "hello").unwrap();
            Target::Path(path)
        })
        .collect()
}

fn handing(targets: Vec<Target>, claims: Claims) -> Handing {
    Handing {
        answers: vec![false; targets.len()],
        targets,
        running: Vec::new(),
        session: None,
        claims,
    }
}

#[test]
fn a_blocked_handoff_copy_leaves_the_runtime_responsive_and_retains_controller_exclusion() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let controller = atlas_control::Controller::at(7001, &dir.path().join("state"));
    let ownership = Arc::new(controller.acquire(false).unwrap());
    let handing = handing(unreadable_pair(dir.path()), claims);
    let (entered, entry) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (seen, seen_task) = (output.clone(), task.clone());
    let mut first = true;
    let copying = move |path: &Path, root: &Path, token: &Token| {
        if std::mem::take(&mut first) {
            let _ = entered.send(states(&seen, &seen_task));
            let _ = released.recv();
        }
        copy(path, root, token)
    };
    let prepared = root.clone();
    let request = Requester::start(move || async move {
        handing
            .on_worker(&ownership, move || Ok(prepared), copying)
            .await
    });

    let intents = entry.recv_timeout(WATCHDOG).unwrap();
    let responsive = request.responsive();
    if !responsive {
        let _ = release.send(());
    }
    assert!(responsive, "the runtime waited for the copy");
    assert_eq!(intents, [State::Intent, State::Intent]);
    let busy = |controller: &atlas_control::Controller| {
        controller.acquire(false).err().map(|error| error.code) == Some(Code::Busy)
    };
    assert!(busy(&controller), "another controller got in");

    request.abandon();
    assert!(busy(&controller), "control ended with the request");
    assert_eq!(states(&output, &task), [State::Intent, State::Intent]);

    release.send(()).unwrap();
    let _next = controller.clone().waiting(WATCHDOG).acquire(false).unwrap();
    assert_eq!(states(&output, &task), [State::Made, State::Made]);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
}

#[test]
fn a_failed_handoff_cleans_earlier_copies_before_releasing_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let (root, output, claims, task) = claiming(dir.path());
    let controller = atlas_control::Controller::at(7001, &dir.path().join("state"));
    let ownership = Arc::new(controller.acquire(false).unwrap());
    let handing = handing(unreadable_pair(dir.path()), claims);
    let (entered, entry) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let mut tried = Vec::new();
    let copying = move |path: &Path, root: &Path, token: &Token| {
        tried.push(path.file_name().unwrap().to_owned());
        if tried.len() == 1 {
            return copy(path, root, token);
        }
        let _ = entered.send(tried.clone());
        let _ = released.recv();
        // The copy fails after making its folder, and cannot remove it.
        fs::create_dir(root.join(token.as_str())).unwrap();
        Err(problem("Could not copy"))
    };
    let prepared = root.clone();
    let request = Requester::start(move || async move {
        handing
            .on_worker(&ownership, move || Ok(prepared), copying)
            .await
    });

    let tried = entry.recv_timeout(WATCHDOG).unwrap();
    let responsive = request.responsive();
    if !responsive {
        let _ = release.send(());
    }
    assert!(responsive, "the runtime waited for the copy");
    assert_eq!(tried, ["a.txt", "b.txt"]);
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "the first copy is made"
    );

    request.abandon();
    release.send(()).unwrap();
    let _next = controller.clone().waiting(WATCHDOG).acquire(false).unwrap();
    let left: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(left.len(), 1, "the first copy went: {left:?}");
    assert!(!left[0].join("a.txt").exists());
    assert_eq!(states(&output, &task), [State::Gone, State::Intent]);
}

#[test]
fn a_file_url_names_its_path_on_this_mac_with_escapes_decoded() {
    assert_eq!(
        local_file("file:///tmp/a%20b/.env"),
        Some(PathBuf::from("/tmp/a b/.env"))
    );
    assert_eq!(
        local_file("FILE://localhost/tmp/x?y#z"),
        Some(PathBuf::from("/tmp/x"))
    );
    assert_eq!(local_file("file:/tmp/x"), Some(PathBuf::from("/tmp/x")));
    // macOS opens a file URL's path on this Mac whatever host it names.
    assert_eq!(
        local_file("file://other-mac/tmp/x"),
        Some(PathBuf::from("/tmp/x"))
    );
    assert_eq!(
        local_file("file://127.0.0.1/tmp/.env"),
        Some(PathBuf::from("/tmp/.env"))
    );
    assert_eq!(
        local_file("file://%6cocalhost/tmp/.env"),
        Some(PathBuf::from("/tmp/.env")),
        "an escaped host"
    );
    assert_eq!(local_file("https://example.com/.env"), None);
    assert_eq!(
        local_file("file:///tmp/100%/.env"),
        Some(PathBuf::from("/tmp/100%/.env")),
        "a lone % stays, as macOS reads it"
    );
    assert_eq!(local_file("file://host-without-path"), None);
}

#[test]
fn a_source_replaced_by_a_link_to_a_store_is_refused_as_it_is_copied_and_leaves_no_folder() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join(".env");
    fs::write(&store, b"TOKEN=1").unwrap();
    let source = directory.path().join("notes.txt");
    std::os::unix::fs::symlink(&store, &source).unwrap();
    let root = directory.path().join("copies");
    fs::create_dir(&root).unwrap();
    let token = Token::new();
    let error = copy(&source, &root, &token).unwrap_err();
    assert_eq!(error.code, Code::Credentials, "{error}");
    assert!(
        !root.join(token.as_str()).exists(),
        "the copy's folder was left"
    );
}

#[test]
fn a_file_url_naming_no_file_is_refused_rather_than_handed_on() {
    let error = refuse_targets(&[Target::Name("file://host-without-path".into())]).unwrap_err();
    assert_eq!(error.code, Code::Usage);
    assert!(refuse_targets(&[Target::Name("https://example.com".into())]).is_ok());
}

#[test]
fn a_file_url_with_an_escaped_slash_in_a_name_is_refused_rather_than_judged_as_another_file() {
    assert_eq!(
        local_file("file:///Users/person/.ssh/key%2F..%2Fconfig"),
        None
    );
    let error = refuse_targets(&[Target::Name("file:///tmp/a%2Fb".into())]).unwrap_err();
    assert_eq!(error.code, Code::Usage);
}

#[test]
fn only_a_scheme_at_the_start_makes_a_url() {
    assert!(has_scheme("https://example.com"));
    assert!(has_scheme("file:///tmp/x"));
    assert!(!has_scheme("/tmp/odd://missing/.env"));
    assert!(!has_scheme("dir/notes.txt"));
    assert!(!has_scheme("2x:thing"));
}
