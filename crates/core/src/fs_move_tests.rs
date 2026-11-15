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

#[test]
fn two_entries_in_two_folders_exchange_places_whole() {
    let root = tempfile::tempdir().unwrap();
    let (one, two) = (root.path().join("one"), root.path().join("two"));
    std::fs::create_dir(&one).unwrap();
    std::fs::create_dir(&two).unwrap();
    std::fs::write(one.join("app"), "new").unwrap();
    std::fs::write(two.join("held"), "old").unwrap();
    let (first, second) = (fs::File::open(&one).unwrap(), fs::File::open(&two).unwrap());

    exchange_between(
        &first,
        &FileName::new("app").unwrap(),
        &second,
        &FileName::new("held").unwrap(),
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(one.join("app")).unwrap(), "old");
    assert_eq!(std::fs::read_to_string(two.join("held")).unwrap(), "new");
    let missing = exchange_between(
        &first,
        &FileName::new("app").unwrap(),
        &second,
        &FileName::new("absent").unwrap(),
    );
    assert!(missing.is_err());
}
