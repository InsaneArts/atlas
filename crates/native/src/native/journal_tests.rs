use super::*;

#[test]
fn an_invalid_journal_names_the_decode_step_without_copying_its_contents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    fs::write(&path, b"private-journal-canary").unwrap();

    let error = load(&path, &test_epoch()).err().unwrap();
    assert_eq!(error.code, Code::InvalidJson);
    assert!(!error.uncertain);
    assert!(error.error.contains("Decode held-input journal"));
    assert!(error.error.contains(&path.display().to_string()));
    assert!(
        !serde_json::to_string(&error)
            .unwrap()
            .contains("private-journal-canary")
    );
}

fn test_epoch() -> InputEpoch {
    InputEpoch {
        uid: nix::unistd::getuid().as_raw(),
        security_session_id: 10,
        display_id: 1,
        width: 1600.0,
        height: 900.0,
        process_id: 1,
    }
}

#[test]
fn nonzero_press_number_survives_restart_and_cleanup_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    let mut epoch = test_epoch();
    let held = Held {
        epoch: epoch.clone(),
        button: Some(Button::Right),
        keys: vec![55],
        x: 10.5,
        y: 20.5,
        count: 2,
        mouse_number: 37,
        flags: 1 << 20,
    };
    Journal::new(path.clone()).write(held.clone()).unwrap();

    // A replacement input process must retain the interrupted press's
    // number so its release can match the already-posted mouse-down.
    epoch.process_id = 2;
    let mut recovered = load(&path, &epoch).unwrap().unwrap();
    assert!(recovered == held);
    assert_eq!(recovered.mouse_number, 37);

    // This validates the recovery record, not desktop event delivery.
    recovered.epoch = epoch.clone();
    recovered.button = None;
    recovered.keys.clear();
    recovered.flags = 0;

    Journal::new(path.clone()).write(recovered.clone()).unwrap();
    let clean = load(&path, &epoch).unwrap().unwrap();
    assert!(clean == recovered);
    assert_eq!(clean.mouse_number, 37);
}

#[test]
fn negative_press_number_is_not_replayed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    let epoch = test_epoch();
    let mut held = Held {
        epoch: epoch.clone(),
        button: Some(Button::Left),
        keys: Vec::new(),
        x: 10.0,
        y: 10.0,
        count: 1,
        mouse_number: 1,
        flags: 0,
    };
    store(&path, &held).unwrap();
    assert!(load(&path, &epoch).unwrap().is_some());

    held.mouse_number = -1;
    store(&path, &held).unwrap();

    let Err(error) = load(&path, &epoch) else {
        panic!("A negative mouse event number was accepted for recovery");
    };

    assert_eq!(error.code, "identity");
}

#[test]
fn clean_journals_survive_login_changes_but_held_input_does_not_cross() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    let mut epoch = test_epoch();
    let mut held = Held {
        epoch: epoch.clone(),
        button: None,
        keys: Vec::new(),
        x: 10.0,
        y: 10.0,
        count: 1,
        mouse_number: 0,
        flags: 0,
    };
    store(&path, &held).unwrap();

    epoch.security_session_id = 20;
    assert!(load(&path, &epoch).unwrap().is_none());

    held.button = Some(Button::Left);
    store(&path, &held).unwrap();
    let original = fs::metadata(&path).unwrap();
    assert_eq!(original.uid(), epoch.uid);
    assert_eq!(original.mode() & 0o777, 0o600);
    assert!(
        load(&path, &epoch).unwrap().is_none(),
        "nothing is replayed"
    );
    assert!(!path.exists(), "the next login starts clean");

    let kept: Vec<_> = fs::read_dir(directory.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().into_string().unwrap())
        .filter(|name| name.starts_with("held.10.") && name.ends_with(".json"))
        .collect();
    assert_eq!(kept.len(), 1, "the earlier login's journal is kept");
    let archive = directory.path().join(&kept[0]);
    let archived = fs::symlink_metadata(&archive).unwrap();
    assert!(archived.is_file());
    assert_eq!(archived.ino(), original.ino());
    assert_eq!(archived.uid(), original.uid());
    assert_eq!(archived.mode(), original.mode());

    let evidence: Held =
        serde_json::from_slice(&fs::read(directory.path().join(&kept[0])).unwrap()).unwrap();
    assert!(evidence == held);

    store(&path, &held).unwrap();
    assert!(load(&path, &epoch).unwrap().is_none());
    assert!(!path.exists(), "a second one never replaces the first");
    assert_eq!(fs::metadata(&archive).unwrap().ino(), original.ino());
    let archives: Vec<_> = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(archives.len(), 2);
    for archive in archives {
        let evidence: Held = serde_json::from_slice(&fs::read(archive).unwrap()).unwrap();
        assert!(evidence == held);
    }
}

/// Only the journal file itself is read: never a link to one, even a
/// valid one, nor anything that is not a regular file.
#[test]
fn a_linked_or_irregular_journal_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let epoch = test_epoch();
    let real = directory.path().join("real.json");
    store(
        &real,
        &Held {
            epoch: epoch.clone(),
            button: None,
            keys: Vec::new(),
            x: 1.0,
            y: 1.0,
            count: 1,
            mouse_number: 0,
            flags: 0,
        },
    )
    .unwrap();

    assert!(load(&real, &epoch).unwrap().is_some());

    let link = directory.path().join("held.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(load(&link, &epoch).err().unwrap().code, "identity");
    assert_eq!(
        load(directory.path(), &epoch).err().unwrap().code,
        "identity"
    );

    let fifo = directory.path().join("fifo.json");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
    assert_eq!(load(&fifo, &epoch).err().unwrap().code, "identity");
    assert!(
        load(&directory.path().join("absent.json"), &epoch)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_failed_pointer_move_write_still_allows_the_release_to_be_posted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    let epoch = test_epoch();
    let held = Held {
        epoch: epoch.clone(),
        button: Some(Button::Left),
        keys: Vec::new(),
        x: 10.0,
        y: 10.0,
        count: 1,
        mouse_number: 1,
        flags: 0,
    };
    let mut journal = Journal::new(path.clone());
    let mut posted = Vec::new();
    {
        // Posting is recorded only after the conservative journal write succeeds.
        let mut post = |record, phase| -> Result<()> {
            journal.write(record)?;
            posted.push(phase);
            Ok(())
        };
        post(held.clone(), "mouse-down").unwrap();

        let pending = path.with_extension("pending");
        fs::create_dir(&pending).unwrap();
        let mut moved = held.clone();
        moved.x += 1.0;
        assert!(post(moved, "mouse-dragged").is_err());

        post(held.clone(), "mouse-up").unwrap();
    }
    assert_eq!(posted, ["mouse-down", "mouse-up"]);
    assert!(load(&path, &epoch).unwrap().unwrap() == held);

    let mut clean = held;
    clean.button = None;
    assert!(journal.write(clean.clone()).is_err());
    assert!(journal.write(clean.clone()).is_err());
    assert_eq!(
        load(&path, &epoch).unwrap().unwrap().button,
        Some(Button::Left)
    );

    fs::remove_dir(path.with_extension("pending")).unwrap();
    journal.write(clean.clone()).unwrap();
    assert!(load(&path, &epoch).unwrap().unwrap() == clean);
}

#[test]
fn failed_clean_commit_is_retried_without_losing_the_held_record() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("held.json");
    let epoch = test_epoch();
    let mut held = Held {
        epoch: epoch.clone(),
        button: None,
        keys: vec![0],
        x: 10.0,
        y: 10.0,
        count: 1,
        mouse_number: 0,
        flags: 1 << 20,
    };
    let mut journal = Journal::new(path.clone());
    journal.write(held.clone()).unwrap();

    held.keys.clear();
    held.flags = 0;

    // A failed replacement must preserve the old recoverable record and
    // must not convince the cache that cleanup was durably committed.
    let pending = path.with_extension("pending");
    fs::create_dir(&pending).unwrap();
    assert!(journal.write(held.clone()).is_err());
    let interrupted = load(&path, &epoch).unwrap().unwrap();
    assert_eq!(interrupted.keys, vec![0]);
    assert_eq!(interrupted.flags, 1 << 20);

    fs::remove_dir(pending).unwrap();
    journal.write(held).unwrap();
    let clean = load(&path, &epoch).unwrap().unwrap();
    assert!(clean.keys.is_empty());
    assert_eq!(clean.flags, 0);
}
