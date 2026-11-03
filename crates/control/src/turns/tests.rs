use super::*;
use atlas_core::process::Instance;

#[test]
fn waiters_with_equal_timestamps_use_their_names_to_order() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let since = now_ms();
    let later = Turn::join_at(&folder, None, since, "ffffffffffffffff");
    let earlier = Turn::join_at(&folder, None, since, "0000000000000000");
    assert!(!later.first());
    assert!(earlier.first());
    drop(earlier);
    assert!(later.first());
}

/// A waiting note in `folder` for `process`, begun `since`.
fn noted(folder: &Path, process: Instance, since: u64) -> PathBuf {
    make_folder(folder).unwrap();
    let mut name = atlas_core::files::token();
    name.truncate(NAME_LEN);
    let path = folder.join(format!("{name}.json"));
    let note = Note {
        session: None,
        host: None,
        folder: None,
        named: None,
        process,
        since,
    };
    note.write(&path).unwrap();
    path
}

#[test]
fn a_written_note_goes_with_its_turn_even_when_it_could_not_be_kept_open() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let turn = Turn::join(&folder, None);
    let path = folder.join(&turn.name);
    assert!(turn.written && path.exists());

    let mut unopened = turn;
    unopened.note = None;
    drop(unopened);
    assert!(!path.exists());
}

#[test]
fn every_waiter_counts_however_many_there_are_and_whatever_else_is_there() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let this = Instance::current().unwrap();
    let now = now_ms();
    let turn = Turn::join(&folder, None);

    for n in 0..100 {
        fs::write(folder.join(format!("litter-{n}.json")), b"{}").unwrap();
        fs::write(folder.join(format!("{n:016x}.json")), b"not a note").unwrap();
    }
    for n in 0..70 {
        noted(&folder, this, now + 1000 + n);
    }
    assert!(turn.first(), "seventy newer waiters wait behind it");

    let older = noted(&folder, this, now - 1000);
    assert!(
        !turn.first(),
        "one older waiter, wherever it is listed, goes first"
    );
    fs::remove_file(older).unwrap();
    assert!(turn.first());
}

#[test]
fn a_suspended_waiter_is_passed_over_at_once_and_kept() {
    use nix::sys::signal::{Signal, kill};

    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let mut child = atlas_core::test_support::TestChild::spawn(
        std::process::Command::new("/bin/sh").args(["-c", "echo atlas-test:ready; read -r permit"]),
    );
    child.expect("ready");
    let pid = i32::try_from(child.process.id()).unwrap();
    let suspended = atlas_core::host::ancestry::process(pid).unwrap().instance;
    let path = noted(&folder, suspended, now_ms() - 1000);
    assert!(queued(&folder), "a running waiter is in line");

    kill(nix::unistd::Pid::from_raw(pid), Signal::SIGSTOP).unwrap();
    let started = std::time::Instant::now();
    while suspended.state() != State::Stopped {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!queued(&folder), "a suspended one is not, at once");
    assert!(path.exists(), "it is not gone, so its note stays");

    let _ = kill(nix::unistd::Pid::from_raw(pid), Signal::SIGCONT);
    child.process.kill().unwrap();
    assert!(!child.wait().success());
    assert!(!queued(&folder));
    assert!(!path.exists(), "an ended waiter's note is removed");
}

#[test]
fn a_waiter_macos_will_not_describe_is_skipped_but_never_removed() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    // launchd is another account's process: macOS will not say more.
    let other = Instance::new(1, [1, 0]);
    assert_eq!(other.state(), State::Unknown);
    let path = noted(&folder, other, now_ms() - 1000);

    assert!(!queued(&folder));
    assert!(path.exists());
}

#[test]
fn stale_waiting_notes_are_ignored_and_reading_them_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let this = Instance::current().unwrap();
    let now = now_ms();
    let live = noted(&folder, this, now - 1000);
    let stale = noted(
        &folder,
        this,
        now - u64::try_from(STALE.as_millis()).unwrap() - 1000,
    );
    let gone = noted(&folder, Instance::new(i32::MAX, [1, 0]), now - 1000);

    let notes = waiting(&folder);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].since, now - 1000);
    assert_eq!(checked_waiting(&folder).unwrap(), notes);
    assert!(
        live.exists() && stale.exists() && gone.exists(),
        "a read for the apps removes nothing"
    );
    assert_eq!(waiters(&folder).len(), 1);
    assert!(!gone.exists(), "a waiter's own look tidies up");
}

#[test]
fn display_candidates_require_recent_notes_and_recent_touches() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let process = Instance::new(i32::MAX, [1, 0]);
    let now_ms = 1_761_660_429_000;
    let now = SystemTime::UNIX_EPOCH + Duration::from_millis(now_ms);
    let oldest = now_ms - atlas_core::time::ms(STALE);
    let fresh = noted(&folder, process, oldest);
    let quiet = noted(&folder, process, now_ms);
    let stale = noted(&folder, process, oldest - 1);
    for (path, age) in [(&fresh, QUIET / 2), (&quiet, QUIET), (&stale, QUIET / 2)] {
        File::open(path).unwrap().set_modified(now - age).unwrap();
    }

    let candidates = candidates_at(&folder, now);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].since, oldest);
    assert!(fresh.exists() && quiet.exists() && stale.exists());
}

#[test]
fn tidying_removes_dead_waiters_even_when_their_notes_are_old_or_quiet() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let gone = Instance::new(i32::MAX, [1, 0]);
    assert_eq!(gone.state(), State::Gone);
    let stale = noted(&folder, gone, now_ms() - atlas_core::time::ms(STALE) - 1000);
    let quiet = noted(&folder, gone, now_ms());
    OpenOptions::new()
        .write(true)
        .open(&quiet)
        .unwrap()
        .set_modified(SystemTime::now() - QUIET - Duration::from_secs(1))
        .unwrap();
    assert!(checked_waiting(&folder).unwrap().is_empty());
    assert!(
        stale.exists() && quiet.exists(),
        "a service check only reads"
    );
    assert!(waiters(&folder).is_empty());
    assert!(
        !stale.exists() && !quiet.exists(),
        "a tidying scan removes both"
    );
    assert!(checked_waiting(&folder).unwrap().is_empty());
}

#[test]
fn failed_directory_entries_count_toward_the_budget_and_leave_the_read_incomplete() {
    let mut read = 0;
    let entries = std::iter::from_fn(|| {
        read += 1;
        assert!(read <= ENTRIES, "the scan must not exceed its budget");
        Some(Err(std::io::Error::other("unreadable entry")))
    });
    let (notes, complete) = collect_notes(entries, false);
    assert_eq!(read, ENTRIES);
    assert!(notes.is_empty());
    assert!(!complete);

    let (notes, complete) = collect_notes(
        std::iter::once(Err(std::io::Error::other("unreadable entry"))),
        false,
    );
    assert!(notes.is_empty());
    assert!(!complete);
}

#[test]
fn expired_unknown_waiters_do_not_prevent_service_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let other = Instance::new(1, [1, 0]);
    assert_eq!(other.state(), State::Unknown);
    let path = noted(
        &folder,
        other,
        now_ms() - atlas_core::time::ms(STALE) - 1000,
    );
    assert!(checked_waiting(&folder).unwrap().is_empty());
    assert!(path.exists(), "a service check does not remove notes");

    let mut note = Note::read(&path).unwrap();
    note.since = now_ms();
    note.write(&path).unwrap();
    assert!(checked_waiting(&folder).is_none(), "a fresh note refuses");

    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::now() - QUIET - Duration::from_secs(1))
        .unwrap();
    assert!(checked_waiting(&folder).unwrap().is_empty());
    assert!(path.exists());
}

#[test]
fn an_unknown_waiter_cannot_establish_idle_for_service_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("waiting");
    let other = Instance::new(1, [1, 0]);
    assert_eq!(other.state(), State::Unknown);
    let path = noted(&folder, other, now_ms());
    assert!(waiting(&folder).is_empty());
    assert!(checked_waiting(&folder).is_none());
    assert!(path.exists());
}
