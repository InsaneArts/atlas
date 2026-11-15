use super::{write::Contents, *};
use crate::machine::{changes::State, commitment::create, vault::Memory};
use atlas_core::stage::Snapshot;
use std::{
    ffi::CStr,
    fs,
    io::Read,
    os::unix::fs::{PermissionsExt, symlink},
    sync::Arc,
};

/// A source tree with a script, a read-only folder, a link pointing out
/// of it and a quarantine flag, and the guest's root of copies.
struct Staging {
    _folders: (tempfile::TempDir, tempfile::TempDir),
    source: PathBuf,
    root: Root,
}

fn staging() -> Staging {
    let source = tempfile::tempdir().unwrap();
    let guest = tempfile::tempdir().unwrap();
    let tree = source.path().join("Tool.app");
    fs::create_dir_all(tree.join("Contents/MacOS")).unwrap();
    fs::create_dir_all(tree.join("Contents/Sealed")).unwrap();
    fs::write(tree.join("Contents/Sealed/data"), b"sealed").unwrap();
    fs::set_permissions(
        tree.join("Contents/Sealed"),
        fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    let script = tree.join("Contents/MacOS/tool");
    fs::write(&script, b"#!/bin/sh\necho staged\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    symlink("/etc/hosts", tree.join("Contents/hosts")).unwrap();
    let marked = atlas_core::spawn::status(
        std::process::Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0081;5f;Atlas;"])
            .arg(&tree),
    )
    .unwrap();
    assert!(marked.success());
    let root = Root::open(&guest.path().join("open")).unwrap();
    Staging {
        _folders: (source, guest),
        source: tree,
        root,
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // The read-only folders would keep the temporary folders from going.
        let _ = fs::set_permissions(
            self.source.join("Contents/Sealed"),
            fs::Permissions::from_mode(0o755),
        );
        for entry in fs::read_dir(&self.root.path)
            .into_iter()
            .flatten()
            .flatten()
        {
            let _ = write::discard(
                &self.root.folder,
                &CString::new(entry.file_name().into_encoded_bytes()).unwrap(),
            );
        }
    }
}

/// The contents of every file of a snapshot read from its source, one
/// chunk per file, as the station sends them.
struct Sent(std::vec::IntoIter<Vec<u8>>);

impl Sent {
    fn of(source: &Path, snapshot: &Snapshot) -> Self {
        let chunks: Vec<_> = snapshot
            .files()
            .map(|(entry, _, _)| {
                let mut read = Vec::new();
                atlas_core::stage::open_file(source, &entry.path)
                    .unwrap()
                    .read_to_end(&mut read)
                    .unwrap();
                read
            })
            .collect();
        Self(chunks.into_iter())
    }
}

impl Contents for Sent {
    fn next_chunk(&mut self) -> Option<Vec<u8>> {
        self.0.next()
    }
}

/// What the station asks with to stage `snapshot`.
fn request_for(snapshot: &Snapshot) -> Value {
    json!({"op": "stage", "name": snapshot.name, "digest": snapshot.digest})
}

fn written(staging: &Staging, change: &Token) -> Result<Snapshot> {
    let snapshot = Snapshot::of(&staging.source).unwrap();
    let mut sent = Sent::of(&staging.source, &snapshot);
    write::write(
        &staging.root,
        change,
        &snapshot,
        &request_for(&snapshot),
        &mut sent,
        &|| true,
    )?;
    Ok(snapshot)
}

#[test]
fn a_tree_written_and_placed_is_the_tree_it_was_taken_from() {
    let staging = staging();
    let change = Token::new();
    let snapshot = written(&staging, &change).unwrap();

    assert!(matches!(place(&staging.root, &change), Placed::Done));

    let copy = staging.root.placed(&change, "Tool.app");
    let again = Snapshot::of(&copy).unwrap();
    assert_eq!(again.entries, snapshot.entries);
    assert_eq!(
        fs::read_link(copy.join("Contents/hosts")).unwrap(),
        Path::new("/etc/hosts")
    );
    assert_eq!(
        evidence(&staging.root, &change),
        Some(Evidence {
            change: change.clone(),
            request: request_for(&snapshot),
        })
    );
}

#[test]
fn a_script_signed_in_its_attributes_still_verifies_once_placed() {
    let staging = staging();
    let script = staging.source.join("Contents/MacOS/tool");
    let signed = atlas_core::spawn::output(
        std::process::Command::new("/usr/bin/codesign")
            .args(["-s", "-", "-f"])
            .arg(&script),
    )
    .unwrap();
    assert!(signed.status.success(), "{signed:?}");
    let change = Token::new();
    written(&staging, &change).unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));

    let copy = staging
        .root
        .placed(&change, "Tool.app")
        .join("Contents/MacOS/tool");
    let verified = atlas_core::spawn::output(
        std::process::Command::new("/usr/bin/codesign")
            .arg("-v")
            .arg(&copy),
    )
    .unwrap();

    assert!(verified.status.success(), "{verified:?}");
}

#[test]
fn contents_that_are_not_the_ones_named_are_refused_and_nothing_is_placed() {
    let staging = staging();
    let change = Token::new();
    let snapshot = Snapshot::of(&staging.source).unwrap();
    let mut chunks: Vec<Vec<u8>> = Sent::of(&staging.source, &snapshot).0.collect();
    chunks[0][0] ^= 1;
    let mut sent = Sent(chunks.into_iter());

    let refused = write::write(
        &staging.root,
        &change,
        &snapshot,
        &request_for(&snapshot),
        &mut sent,
        &|| true,
    );

    assert!(refused.is_err());
    write::discard(&staging.root.folder, &super::staging(&change)).unwrap();
    assert!(!there(&staging.root, &placed(&change)));
    assert!(!there(&staging.root, &super::staging(&change)));
}

#[test]
fn bytes_past_the_last_file_are_refused() {
    let staging = staging();
    let change = Token::new();
    let snapshot = Snapshot::of(&staging.source).unwrap();
    let mut chunks: Vec<Vec<u8>> = Sent::of(&staging.source, &snapshot).0.collect();
    chunks.push(b"!".to_vec());
    let mut sent = Sent(chunks.into_iter());

    let refused = write::write(
        &staging.root,
        &change,
        &snapshot,
        &request_for(&snapshot),
        &mut sent,
        &|| true,
    );

    assert!(refused.is_err());
}

#[test]
fn a_stage_ended_partway_writes_nothing_more() {
    let staging = staging();
    let change = Token::new();
    let snapshot = Snapshot::of(&staging.source).unwrap();
    let mut sent = Sent::of(&staging.source, &snapshot);
    let checks = std::sync::atomic::AtomicUsize::new(0);

    let ended = write::write(
        &staging.root,
        &change,
        &snapshot,
        &request_for(&snapshot),
        &mut sent,
        &|| checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2,
    );

    assert_eq!(ended.unwrap_err().code, Code::Cancelled);
}

#[test]
fn a_placed_stage_never_replaces_one_there_before() {
    let staging = staging();
    let change = Token::new();
    written(&staging, &change).unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));
    // Written again under the same change while the first is in place.
    let again = staging
        .root
        .path
        .join(super::staging(&change).to_str().unwrap());
    fs::create_dir(again).unwrap();

    assert!(matches!(place(&staging.root, &change), Placed::NotMoved(_)));
}

#[test]
fn discarding_a_tree_removes_a_link_in_it_never_what_it_points_at() {
    let staging = staging();
    let outside = tempfile::tempdir().unwrap();
    let kept = outside.path().join("kept");
    fs::write(&kept, b"keep me").unwrap();
    symlink(outside.path(), staging.source.join("Contents/out")).unwrap();
    let change = Token::new();
    written(&staging, &change).unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));

    write::discard(&staging.root.folder, &placed(&change)).unwrap();

    assert!(!there(&staging.root, &placed(&change)));
    assert_eq!(fs::read(&kept).unwrap(), b"keep me");
}

fn records() -> (tempfile::TempDir, Changes<Arc<Memory>>) {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, crate::machine::changes::STORE).unwrap();
    let changes = Changes::new(folder.path(), vault);
    (folder, changes)
}

#[test]
fn recovery_settles_each_stage_left_intended_from_what_the_disk_holds() {
    let staging = staging();
    let (_records, changes) = records();
    let request = request_for(&Snapshot::of(&staging.source).unwrap());
    let (placed_one, cut_short, never_made) = (Token::new(), Token::new(), Token::new());
    for change in [&placed_one, &cut_short, &never_made] {
        changes.ask(change, OPERATION, &request).unwrap();
    }
    let snapshot = written(&staging, &placed_one).unwrap();
    assert!(matches!(place(&staging.root, &placed_one), Placed::Done));
    written(&staging, &cut_short).unwrap();

    recover(&staging.root, &changes).unwrap();

    let reply_of = |change: &Token| match changes.of(change).unwrap() {
        Some(State::Done { reply }) => reply,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        reply_of(&placed_one),
        reply(&staging.root, &placed_one, "Tool.app", &snapshot.digest)
    );
    assert_eq!(reply_of(&cut_short)["ok"], false);
    assert_eq!(reply_of(&never_made)["ok"], false);
    assert!(!there(&staging.root, &super::staging(&cut_short)));
}

#[test]
fn a_placed_stage_without_its_evidence_stays_intended() {
    let staging = staging();
    let (_records, changes) = records();
    let change = Token::new();
    changes
        .ask(&change, OPERATION, &json!({"op": "stage"}))
        .unwrap();
    written(&staging, &change).unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));
    fs::remove_file(staging.root.path.join(change.as_str()).join(EVIDENCE)).unwrap();

    recover(&staging.root, &changes).unwrap();

    assert_eq!(changes.of(&change).unwrap(), Some(State::Intended));
}

#[test]
fn evidence_of_another_request_leaves_its_stage_intended() {
    let staging = staging();
    let (_records, changes) = records();
    let change = Token::new();
    changes
        .ask(
            &change,
            OPERATION,
            &json!({"op": "stage", "name": "Other.app", "digest": "0"}),
        )
        .unwrap();
    written(&staging, &change).unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));

    recover(&staging.root, &changes).unwrap();

    assert_eq!(changes.of(&change).unwrap(), Some(State::Intended));
}

#[test]
fn a_folder_written_with_no_access_at_all_is_still_discarded() {
    let staging = staging();
    fs::create_dir(staging.source.join("Contents/Closed")).unwrap();
    let taken = Snapshot::of(&staging.source).unwrap();
    // No source can be read with no access: the station never takes one,
    // but a desktop must still clear away what it was asked to write.
    let mut entries = taken.entries.clone();
    for entry in &mut entries {
        if entry.path == ["Contents", "Closed"] {
            entry.mode = 0;
        }
    }
    let snapshot = Snapshot::new(taken.name.clone(), entries).unwrap();
    let mut sent = Sent::of(&staging.source, &snapshot);
    let change = Token::new();
    write::write(
        &staging.root,
        &change,
        &snapshot,
        &request_for(&snapshot),
        &mut sent,
        &|| true,
    )
    .unwrap();
    assert!(matches!(place(&staging.root, &change), Placed::Done));

    write::discard(&staging.root.folder, &placed(&change)).unwrap();

    assert!(!there(&staging.root, &placed(&change)));
}

#[test]
fn a_root_that_is_a_link_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let real = folder.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&real, folder.path().join("open")).unwrap();

    assert!(Root::open(&folder.path().join("open")).is_err());
}

/// Whether `name` is in `root`.
fn there(root: &Root, name: &CStr) -> bool {
    write::exists(&root.folder, name).unwrap()
}
