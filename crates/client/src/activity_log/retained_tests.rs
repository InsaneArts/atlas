use super::super::{FILE, PREVIOUS, ROTATE_AT, Row, append, recent};
use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn row(at: u64) -> Row {
    Row::parse(
        &json!({"at": at.to_string(), "operation": "input", "ok": true,
        "actor": "agent", "command": format!("{at:012}")}),
    )
    .unwrap()
}

/// Fill the current file past [`ROTATE_AT`] with one row at `at`, so the
/// next append starts the log over.
fn fill(directory: &Path, at: u64) {
    let mut large = row(at);
    large.label = Some("x".repeat(usize::try_from(ROTATE_AT).unwrap()));
    append(directory, &large).unwrap();
}

fn kept(directory: &Path) -> Vec<String> {
    history(directory)
        .unwrap()
        .files
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect()
}

#[test]
fn starting_over_keeps_the_previous_file_in_the_history_under_the_next_number() {
    let dir = tempfile::tempdir().unwrap();
    fill(dir.path(), 1);
    append(dir.path(), &row(2)).unwrap();
    assert!(
        kept(dir.path()).is_empty(),
        "the first file filled is the previous one"
    );
    fill(dir.path(), 3);
    append(dir.path(), &row(4)).unwrap();
    assert_eq!(kept(dir.path()), ["1.jsonl"]);
    fill(dir.path(), 5);
    append(dir.path(), &row(6)).unwrap();
    assert_eq!(kept(dir.path()), ["2.jsonl", "1.jsonl"]);

    let folder = dir.path().join(HISTORY);
    assert_eq!(
        fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // Each file starts with the row the one before it filled up to.
    let rows = |path: &Path| {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| {
                Row::parse(&serde_json::from_str(line).unwrap())
                    .unwrap()
                    .at
                    .unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&folder.join("1.jsonl")), ["1"]);
    assert_eq!(rows(&folder.join("2.jsonl")), ["2", "3"]);
    assert_eq!(rows(&dir.path().join(PREVIOUS)), ["4", "5"]);
    assert_eq!(rows(&dir.path().join(FILE)), ["6"]);
    // The global read still goes through the current and previous files only.
    assert_eq!(
        recent(dir.path(), 10)
            .iter()
            .map(|row| row.at.clone().unwrap())
            .collect::<Vec<_>>(),
        ["6", "5", "4"]
    );
}

#[test]
fn a_previous_file_an_older_build_left_moves_into_the_history_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(PREVIOUS),
        "{\"at\":\"1\",\"operation\":\"input\"}\n",
    )
    .unwrap();
    fill(dir.path(), 2);
    append(dir.path(), &row(3)).unwrap();
    assert_eq!(kept(dir.path()), ["1.jsonl"]);
    assert_eq!(
        fs::read_to_string(dir.path().join(HISTORY).join("1.jsonl")).unwrap(),
        "{\"at\":\"1\",\"operation\":\"input\"}\n"
    );
}

#[test]
fn a_start_cut_short_after_keeping_the_previous_file_keeps_it_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(PREVIOUS),
        "{\"at\":\"1\",\"operation\":\"input\"}\n",
    )
    .unwrap();
    keep_previous(dir.path()).unwrap();
    keep_previous(dir.path()).unwrap();
    assert_eq!(kept(dir.path()), ["1.jsonl"]);
    fill(dir.path(), 2);
    append(dir.path(), &row(3)).unwrap();
    assert_eq!(kept(dir.path()), ["1.jsonl"], "it was kept already");
}

#[test]
fn a_history_that_is_a_link_is_never_written_and_the_log_still_starts_over() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(HISTORY)).unwrap();
    fs::write(
        dir.path().join(PREVIOUS),
        "{\"at\":\"1\",\"operation\":\"input\"}\n",
    )
    .unwrap();
    fill(dir.path(), 2);
    append(dir.path(), &row(3)).unwrap();
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);
    assert!(history(dir.path()).is_err());
    assert!(
        fs::read_to_string(dir.path().join(PREVIOUS))
            .unwrap()
            .contains("\"at\":\"2\"")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(FILE))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn only_numbered_files_are_kept_files_newest_number_first() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join(HISTORY);
    fs::create_dir(&folder).unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    for name in [
        "9.jsonl", "10.jsonl", "2.jsonl", "x.jsonl", "+3.jsonl", ".jsonl", "4.json",
    ] {
        fs::write(folder.join(name), "").unwrap();
    }
    assert_eq!(kept(dir.path()), ["10.jsonl", "9.jsonl", "2.jsonl"]);
}

#[test]
fn kept_files_go_once_a_month_old_or_oldest_first_past_the_bound() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join(HISTORY);
    fs::create_dir(&folder).unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
    let now = SystemTime::now();
    let megabyte = usize::try_from(ROTATE_AT).unwrap();
    let write = |number: u64, bytes: usize, age: Duration| {
        let path = folder.join(format!("{number}.jsonl"));
        fs::write(&path, vec![b'\n'; bytes]).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(now - age)
            .unwrap();
    };
    fs::write(dir.path().join(FILE), vec![b'\n'; megabyte]).unwrap();
    fs::write(dir.path().join(PREVIOUS), vec![b'\n'; megabyte]).unwrap();
    let day = Duration::from_secs(24 * 60 * 60);
    // 62 recent megabytes fill the bound with the current and previous files.
    for number in 10..72 {
        write(number, megabyte, day);
    }
    // Past the bound: older, and one small enough to fit on its own.
    write(9, megabyte, day);
    write(8, 1, day);
    // A month old, even though there is room: none here, but it goes for age.
    write(7, 1, 31 * day);
    fs::write(folder.join("notes.txt"), "kept").unwrap();

    prune(dir.path(), now).unwrap();
    let left = kept(dir.path());
    assert_eq!(left.len(), 62);
    assert_eq!(left.first().map(String::as_str), Some("71.jsonl"));
    assert_eq!(left.last().map(String::as_str), Some("10.jsonl"));
    assert!(folder.join("notes.txt").exists(), "only numbered files go");
    assert!(dir.path().join(FILE).exists() && dir.path().join(PREVIOUS).exists());

    write(70, 1, 40 * day);
    prune(dir.path(), now).unwrap();
    assert!(
        !folder.join("70.jsonl").exists(),
        "a month old goes with room to spare"
    );
    assert!(folder.join("69.jsonl").exists());
}

#[test]
fn the_log_stays_within_its_bound_as_it_starts_over_and_fills_again() {
    let dir = tempfile::tempdir().unwrap();
    let total = |directory: &Path| {
        let history: u64 = history(directory)
            .unwrap()
            .files
            .iter()
            .map(|path| fs::metadata(path).unwrap().len())
            .sum();
        let length = |name: &str| fs::metadata(directory.join(name)).map_or(0, |found| found.len());
        history + length(FILE) + length(PREVIOUS)
    };
    let mut largest = 0;
    for at in 0..36 {
        fill(dir.path(), at * 2);
        append(dir.path(), &row(at * 2 + 1)).unwrap();
        largest = largest.max(total(dir.path()));
        // The current file, filled again before the next start.
        fill(dir.path(), at * 2 + 1);
        largest = largest.max(total(dir.path()));
    }
    // The current file ends a few hundred bytes past its megabyte here, the
    // end of the row that took it past.
    assert!(largest <= KEEP_BYTES + 4096, "{largest}");
    assert!(
        kept(dir.path()).len() > 50,
        "the bound is used, not just a few files"
    );
}
