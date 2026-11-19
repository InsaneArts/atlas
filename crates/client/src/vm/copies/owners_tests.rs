use super::*;
use crate::host_session::Host;
use std::os::unix::fs::PermissionsExt;

fn owner(copy: &Token) -> Owner {
    Owner {
        copy: copy.clone(),
        session: Some(Key::of(Host::Codex, "one")),
        task: None,
        plan: None,
        fixture: None,
    }
}

#[test]
fn a_copys_owner_is_kept_privately_read_back_whole_and_forgotten() {
    let records = tempfile::tempdir().unwrap();
    let owners = Owners::in_records(records.path());
    let copy = Token::new();

    assert_eq!(owners.of(&copy), Owned::Unknown, "nothing kept yet");
    owners.keep(&owner(&copy)).unwrap();

    assert_eq!(owners.of(&copy), Owned::By(owner(&copy)));
    let folder = records.path().join(FOLDER);
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(&folder.join(format!("{copy}.json"))), 0o600);
    assert!(owner(&copy).names(Some(&Key::of(Host::Codex, "one"))));
    assert!(!owner(&copy).names(Some(&Key::of(Host::Codex, "two"))));
    assert!(!owner(&copy).names(None));
    owners.forget(&copy).unwrap();
    assert_eq!(owners.of(&copy), Owned::Unknown);
    owners.forget(&copy).unwrap();
}

#[test]
fn a_record_that_does_not_read_or_names_another_copy_is_nobodys() {
    let records = tempfile::tempdir().unwrap();
    let owners = Owners::in_records(records.path());
    let (copy, other) = (Token::new(), Token::new());
    owners.keep(&owner(&other)).unwrap();
    let folder = records.path().join(FOLDER);

    std::fs::write(folder.join(format!("{copy}.json")), b"not json").unwrap();
    assert_eq!(owners.of(&copy), Owned::Unreadable);
    std::fs::copy(
        folder.join(format!("{other}.json")),
        folder.join(format!("{copy}.json")),
    )
    .unwrap();
    assert_eq!(owners.of(&copy), Owned::Unreadable, "another copy's record");
}
