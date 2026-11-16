use super::*;

fn launched_from(path: &std::path::Path) -> Launched {
    let started = std::fs::metadata(path).unwrap();
    Launched::new(path.to_owned(), started.dev(), started.ino())
}

#[test]
fn a_program_is_replaced_once_its_launch_path_names_another_file_or_none() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas");
    std::fs::write(&path, b"A").unwrap();
    let launched = launched_from(&path);
    assert!(!launched.replaced(), "the file it was started from");

    // An update swaps the bundle: the same path names the new build's file.
    let staged = folder.path().join("atlas.new");
    std::fs::write(&staged, b"B").unwrap();
    std::fs::rename(&staged, &path).unwrap();
    assert!(launched.replaced(), "another file at its path");

    std::fs::remove_file(&path).unwrap();
    assert!(launched.replaced(), "no file at its path");
}

#[test]
fn a_program_whose_file_cannot_be_told_is_never_taken_for_one_not_replaced() {
    let unknown = Program::default();

    assert_eq!(unknown.replaced(), None);
    assert!(!unknown.signed());
    assert_eq!(unknown.said()["replaced"], Value::Null);
}

#[test]
fn this_keepers_own_program_is_read_whole_at_its_start() {
    let own = Program::own();

    assert!(own.signed());
    assert_eq!(own.replaced(), Some(false));
    assert_eq!(own.said()["code"].as_str().map(str::len), Some(40));
}
