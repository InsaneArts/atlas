use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

fn set_attribute(path: &Path, name: &str, value: &[u8]) {
    let file = fs::File::open(path).unwrap();
    xattr::set(
        &file,
        &Attribute {
            name: name.to_owned(),
            value: value.to_vec(),
        },
    )
    .unwrap();
}

/// A small app-like tree: a folder, a script, a nested file and a link.
fn tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("Tool.app");
    fs::create_dir_all(root.join("Contents/MacOS")).unwrap();
    fs::write(root.join("Contents/Info.plist"), b"<plist/>").unwrap();
    let script = root.join("Contents/MacOS/tool");
    fs::write(&script, b"#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    symlink("MacOS/tool", root.join("Contents/run")).unwrap();
    set_attribute(&root, "com.apple.quarantine", b"0081;5f;Atlas;");
    (folder, root)
}

fn path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

#[test]
fn a_folder_is_taken_parents_first_with_its_modes_digests_attributes_and_links() {
    let (_folder, root) = tree();

    let snapshot = Snapshot::of(&root).unwrap();

    let paths: Vec<_> = snapshot.entries.iter().map(|entry| &entry.path).collect();
    assert_eq!(
        paths,
        [
            &path(&[]),
            &path(&["Contents"]),
            &path(&["Contents", "Info.plist"]),
            &path(&["Contents", "MacOS"]),
            &path(&["Contents", "MacOS", "tool"]),
            &path(&["Contents", "run"]),
        ]
    );
    assert_eq!(snapshot.name, "Tool.app");
    assert_eq!(
        snapshot.entries[0].attributes[0].name,
        "com.apple.quarantine"
    );
    assert_eq!(snapshot.entries[4].mode, 0o755);
    assert_eq!(
        snapshot.entries[4].kind,
        Kind::File {
            size: 18,
            sha256: crate::digest::bytes_sha256(b"#!/bin/sh\necho hi\n"),
        }
    );
    assert_eq!(
        snapshot.entries[5].kind,
        Kind::Symlink {
            target: "MacOS/tool".into()
        }
    );
    assert_eq!(snapshot.size(), 26);
    snapshot.check().unwrap();
}

#[test]
fn any_change_to_a_snapshot_breaks_its_digest() {
    let (_folder, root) = tree();
    let mut snapshot = Snapshot::of(&root).unwrap();

    snapshot.entries[4].mode = 0o700;

    assert_eq!(snapshot.check().unwrap_err().code, Code::BadRequest);
}

#[test]
fn a_hard_link_in_the_tree_is_refused() {
    let (_folder, root) = tree();
    fs::hard_link(
        root.join("Contents/Info.plist"),
        root.join("Contents/again.plist"),
    )
    .unwrap();

    let refused = Snapshot::of(&root).unwrap_err();

    assert!(refused.error.contains("hard link"), "{refused:?}");
}

#[test]
fn a_fifo_in_the_tree_is_refused_without_waiting_on_it() {
    let (_folder, root) = tree();
    nix::unistd::mkfifo(&root.join("Contents/pipe"), nix::sys::stat::Mode::S_IRWXU).unwrap();

    let refused = Snapshot::of(&root).unwrap_err();

    assert!(refused.error.contains("FIFO"), "{refused:?}");
}

#[test]
fn a_link_at_the_root_is_refused() {
    let (folder, root) = tree();
    let link = folder.path().join("Link.app");
    symlink(&root, &link).unwrap();

    let refused = Snapshot::of(&link).unwrap_err();

    assert!(refused.error.contains("is a link"), "{refused:?}");
}

#[test]
fn a_link_carrying_attributes_is_refused() {
    let (_folder, root) = tree();
    let written = crate::spawn::status(
        std::process::Command::new("/usr/bin/xattr")
            .args(["-s", "-w", "org.example.on-link", "1"])
            .arg(root.join("Contents/run")),
    )
    .unwrap();
    assert!(written.success());

    let refused = Snapshot::of(&root).unwrap_err();

    assert!(refused.error.contains("link carries"), "{refused:?}");
}

#[test]
fn a_setuid_file_is_refused() {
    let (_folder, root) = tree();
    let script = root.join("Contents/MacOS/tool");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o4755)).unwrap();

    let refused = Snapshot::of(&root).unwrap_err();

    assert!(refused.error.contains("setuid"), "{refused:?}");
}

#[test]
fn a_compressed_file_is_taken_by_its_contents() {
    // An app's Info.plist, compressed by its installer and carrying no
    // other flag: copying cannot make one, as ditto leaves small files be.
    let Some(compressed) = fs::read_dir("/Applications")
        .unwrap()
        .flatten()
        .map(|app| app.path().join("Contents/Info.plist"))
        .find(|plist| {
            nix::sys::stat::lstat(plist).is_ok_and(|stat| stat.st_flags == libc::UF_COMPRESSED)
        })
    else {
        eprintln!("no compressed file to read here");
        return;
    };

    let snapshot = Snapshot::of(&compressed).unwrap();

    let bytes = fs::read(&compressed).unwrap();
    assert_eq!(
        snapshot.entries[0].kind,
        Kind::File {
            size: bytes.len() as u64,
            sha256: crate::digest::bytes_sha256(&bytes),
        }
    );
}

#[test]
fn the_provenance_attribute_is_never_taken() {
    let (_folder, root) = tree();

    let snapshot = Snapshot::of(&root).unwrap();

    assert!(
        snapshot
            .entries
            .iter()
            .flat_map(|entry| &entry.attributes)
            .all(|attribute| attribute.name != PROVENANCE)
    );
}

#[test]
fn a_file_is_opened_again_to_send_only_while_it_is_still_a_file() {
    let (_folder, root) = tree();
    let script = path(&["Contents", "MacOS", "tool"]);
    let mut opened = open_file(&root, &script).unwrap();
    let mut read = String::new();
    std::io::Read::read_to_string(&mut opened, &mut read).unwrap();
    assert_eq!(read, "#!/bin/sh\necho hi\n");
    fs::remove_file(root.join("Contents/MacOS/tool")).unwrap();
    symlink("/etc/hosts", root.join("Contents/MacOS/tool")).unwrap();

    let refused = open_file(&root, &script);

    assert!(refused.is_err());
}

fn snapshot_of(entries: Vec<Entry>) -> Snapshot {
    Snapshot::new("Tool.app".into(), entries).unwrap()
}

fn folder(parts: &[&str]) -> Entry {
    Entry {
        path: path(parts),
        kind: Kind::Folder,
        mode: 0o755,
        attributes: Vec::new(),
    }
}

#[test]
fn a_name_that_climbs_out_is_refused() {
    let snapshot = snapshot_of(vec![folder(&[]), folder(&[".."])]);

    assert_eq!(snapshot.check().unwrap_err().code, Code::BadRequest);
}

#[test]
fn entries_out_of_order_or_outside_a_folder_are_refused() {
    let unordered = snapshot_of(vec![folder(&[]), folder(&["b"]), folder(&["a"])]);
    let orphaned = snapshot_of(vec![folder(&[]), folder(&["a", "b"])]);
    let doubled = snapshot_of(vec![folder(&[]), folder(&["a"]), folder(&["a"])]);

    assert!(unordered.check().is_err());
    assert!(orphaned.check().is_err());
    assert!(doubled.check().is_err());
}

#[test]
fn an_entry_deeper_than_the_bound_is_refused() {
    let mut entries = vec![folder(&[])];
    let mut deep = Vec::new();
    for _ in 0..=DEEPEST {
        deep.push("d");
        entries.push(folder(&deep));
    }

    assert!(snapshot_of(entries).check().is_err());
}

#[test]
fn the_provenance_attribute_in_a_manifest_is_refused() {
    let mut root = folder(&[]);
    root.attributes.push(Attribute {
        name: PROVENANCE.into(),
        value: vec![1],
    });

    assert!(snapshot_of(vec![root]).check().is_err());
}

#[test]
fn a_held_snapshot_reads_from_what_it_read_and_refuses_once_a_file_or_its_path_changes() {
    let folder = tempfile::tempdir().unwrap();
    let tree = folder.path().join("tree");
    std::fs::create_dir(&tree).unwrap();
    std::fs::write(tree.join("a.txt"), b"first").unwrap();
    std::fs::write(tree.join("b.txt"), b"second").unwrap();

    let held = Snapshot::held(&tree).unwrap();
    assert_eq!(held.files().count(), 2);
    held.still(&tree).unwrap();
    std::fs::rename(tree.join("a.txt"), folder.path().join("a.old")).unwrap();
    std::fs::write(tree.join("a.txt"), b"other").unwrap();

    let (_, size, first) = held.files().next().unwrap();
    assert_eq!(
        first.chunk(0, size).unwrap(),
        b"first",
        "the file read, never what the path names now"
    );
    assert!(first.unchanged().is_err(), "renamed away since");
    assert!(held.still(&tree).is_err());

    let again = Snapshot::held(&tree).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(tree.join("b.txt"))
        .unwrap();
    again.still(&tree).unwrap();
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(tree.join("b.txt"))
        .unwrap()
        .write_all(b"!")
        .unwrap();
    assert!(again.still(&tree).is_err(), "grown since");
}

#[test]
fn a_held_files_chunk_is_given_only_while_it_reads_as_it_did() {
    use std::os::unix::fs::FileExt;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("large.bin");
    fs::write(&file, vec![7_u8; CHUNK * 2 + 10]).unwrap();
    let held = Snapshot::held(&file).unwrap();
    let (_, size, held_file) = held.files().next().unwrap();

    assert_eq!(held_file.chunk(0, size).unwrap(), vec![7_u8; CHUNK]);
    assert_eq!(held_file.chunk(2, size).unwrap(), vec![7_u8; 10]);
    assert!(held_file.chunk(3, size).is_err(), "not one of its own");
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .write_at(b"changed", CHUNK as u64 + 5)
        .unwrap();
    assert!(held_file.chunk(1, size).is_err(), "overwritten in place");
    assert_eq!(held_file.chunk(0, size).unwrap(), vec![7_u8; CHUNK]);
}

#[test]
fn a_held_file_is_known_by_the_device_and_inode_it_was_read_as() {
    use std::os::unix::fs::MetadataExt;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("atlas");
    fs::write(&file, b"A").unwrap();
    let read = fs::metadata(&file).unwrap();
    let held = Snapshot::held(&file).unwrap();
    let (_, _, held_file) = held.files().next().unwrap();

    assert!(held_file.is(read.dev(), read.ino()));
    assert!(!held_file.is(read.dev(), read.ino() + 1));
    assert!(!held_file.is(read.dev() + 1, read.ino()));
}
