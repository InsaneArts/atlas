use super::*;

#[test]
fn only_a_plain_file_name_is_one() {
    for plain in ["Probe.app", ".atlas-removing-x", "a b"] {
        assert!(FileName::new(plain).is_some(), "{plain}");
    }
    for not in ["", ".", "..", "a/b", "/", "a\0b"] {
        assert!(FileName::new(not).is_none(), "{not:?}");
    }
}

#[test]
fn a_move_never_replaces_what_is_there() {
    let directory = tempfile::tempdir().unwrap();
    let (from, to) = (directory.path().join("from"), directory.path().join("to"));
    fs::create_dir(&from).unwrap();
    fs::create_dir(&to).unwrap();
    fs::write(from.join("item"), "moved").unwrap();
    fs::write(to.join("item"), "kept").unwrap();
    let (from_dir, to_dir) = (fs::File::open(&from).unwrap(), fs::File::open(&to).unwrap());
    let name = FileName::new("item").unwrap();

    let error = move_between(&from_dir, &name, &to_dir, &name).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(to.join("item")).unwrap(), "kept");

    let other = FileName::new("other").unwrap();
    assert!(matches!(
        move_between(&from_dir, &name, &to_dir, &other).unwrap(),
        Moved::Synced
    ));
    assert_eq!(fs::read_to_string(to.join("other")).unwrap(), "moved");
    assert!(!from.join("item").exists());
}
