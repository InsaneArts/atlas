use super::*;
use serde_json::{Value, json};
use std::{fs, io::Write};

fn log(times: &[u64]) -> String {
    times
        .iter()
        .map(|at| line(json!({"at": at.to_string(), "operation": "input"})))
        .collect()
}

fn line(row: Value) -> String {
    format!("{row}\n")
}

fn times(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.at.as_deref().unwrap()).collect()
}

#[test]
fn a_commands_last_committed_step_wins_even_when_the_clock_moves_backwards() {
    let dir = tempfile::tempdir().unwrap();
    let start = line(
        json!({"at": "9000000000", "operation": "input", "command": "typing",
        "started": true}),
    );
    let end = line(
        json!({"at": "1000000000", "operation": "input", "command": "typing",
        "ok": false, "code": "focus-changed", "partial": true}),
    );
    let other = line(
        json!({"at": "5000000000", "operation": "run", "command": "other",
        "ok": true}),
    );

    for rotated in [false, true] {
        let mut cache = Cache { kept: None };
        fs::write(dir.path().join(FILE), &start).unwrap();
        assert!(cache.recent(dir.path(), 1)[0].under_way());
        if rotated {
            fs::rename(dir.path().join(FILE), dir.path().join(PREVIOUS)).unwrap();
            fs::write(dir.path().join(FILE), other.clone() + &end).unwrap();
        } else {
            fs::write(dir.path().join(FILE), start.clone() + &other + &end).unwrap();
        }
        for _ in 0..2 {
            let rows = cache.recent(dir.path(), 5);
            assert_eq!(times(&rows), ["5000000000", "1000000000"]);
            assert!(rows[1].failed());
            assert_eq!(rows[1].partial, Some(true));
            assert!(!rows[1].under_way());
            assert_eq!(times(&cache.recent(dir.path(), 1)), ["5000000000"]);
        }
    }
}

#[test]
fn a_command_completion_without_a_timestamp_still_replaces_its_start() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(FILE),
        line(json!({"at": "9", "operation": "run", "command": "one", "started": true}))
            + &line(json!({"operation": "run", "command": "one", "ok": true})),
    )
    .unwrap();
    let mut cache = Cache { kept: None };
    let rows = cache.recent(dir.path(), 5);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].worked());
    assert!(!rows[0].under_way());
}

#[test]
fn a_failed_read_is_retried_even_when_the_file_revision_does_not_change() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(FILE), log(&[1, 2])).unwrap();
    let probe = Observation::watch(dir.path());
    let mut cache = Cache { kept: None };
    let partial = cache.recent_using(dir.path(), 5, |file, room, rows| {
        read_back(file, room, rows)?;
        rows.truncate(1);
        Err(io::Error::other("transient read failure"))
    });
    assert_eq!(times(&partial), ["2"]);
    assert!(cache.kept.is_none());
    assert_eq!(times(&cache.recent(dir.path(), 5)), ["2", "1"]);
    assert_eq!(probe.threads().len(), 2);
    assert_eq!(times(&cache.recent(dir.path(), 5)), ["2", "1"]);
    assert_eq!(probe.threads().len(), 2);
}

#[test]
fn an_open_failure_is_not_cached_as_a_missing_log() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-a-directory");
    fs::write(&path, "").unwrap();
    let probe = Observation::watch(&path);
    let mut cache = Cache { kept: None };
    for _ in 0..2 {
        assert!(cache.recent(&path, 5).is_empty());
        assert!(cache.kept.is_none());
    }
    assert_eq!(probe.threads().len(), 2);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(cache.recent(&path, 5).is_empty());
    assert!(
        cache.kept.is_some(),
        "a genuinely missing log may be cached"
    );
}

#[test]
fn a_short_read_does_not_count_as_a_completed_log_read() {
    let mut room = READ_LIMIT;
    let mut rows = Vec::new();
    let bytes = log(&[1]);
    let error = super::super::read_rows(
        std::io::Cursor::new(bytes.as_bytes()),
        bytes.len() as u64 + 1,
        &mut room,
        &mut rows,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(rows.is_empty());
    assert_eq!(room, READ_LIMIT - bytes.len() as u64 - 1);
}

#[test]
fn an_unchanged_log_is_parsed_once_for_several_reads() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(FILE), log(&[1, 3])).unwrap();
    fs::write(dir.path().join(PREVIOUS), log(&[0, 2])).unwrap();
    let probe = Observation::watch(dir.path());
    let mut cache = Cache { kept: None };

    for _ in 0..3 {
        assert_eq!(times(&cache.recent(dir.path(), 1)), ["3"]);
        assert_eq!(times(&cache.recent(dir.path(), 12)), ["3", "2", "1", "0"]);
        assert_eq!(times(&cache.recent(dir.path(), 64)), ["3", "2", "1", "0"]);
        assert_eq!(times(&cache.recent(dir.path(), 200)), ["3", "2", "1", "0"]);
    }
    assert_eq!(probe.threads().len(), 1);
}

#[test]
fn an_append_a_rotation_a_truncation_and_a_replacement_each_rebuild_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let previous = dir.path().join(PREVIOUS);
    fs::write(&path, log(&[1])).unwrap();
    let probe = Observation::watch(dir.path());
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["1"]);

    super::super::append(
        dir.path(),
        &Row::parse(&json!({"at": "2", "operation": "input"})).unwrap(),
    )
    .unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["2", "1"]);
    assert_eq!(probe.threads().len(), 2);

    fs::rename(&path, &previous).unwrap();
    fs::write(&path, log(&[3])).unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["3", "2", "1"]);
    assert_eq!(probe.threads().len(), 3);

    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["2", "1"]);
    assert_eq!(probe.threads().len(), 4);

    let replacement = dir.path().join("replacement");
    fs::write(&replacement, log(&[4])).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["4", "2", "1"]);
    assert_eq!(probe.threads().len(), 5);
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["4", "2", "1"]);
    assert_eq!(probe.threads().len(), 5);
}

#[test]
fn changes_to_the_previous_log_also_rebuild_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(PREVIOUS);
    fs::write(dir.path().join(FILE), log(&[9])).unwrap();
    fs::write(&path, log(&[1])).unwrap();
    let probe = Observation::watch(dir.path());
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["9", "1"]);

    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(log(&[2]).as_bytes())
        .unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["9", "2", "1"]);
    fs::write(&path, "").unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["9"]);
    let replacement = dir.path().join("replacement");
    fs::write(&replacement, log(&[3])).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["9", "3"]);
    fs::remove_file(path).unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["9"]);
    assert_eq!(probe.threads().len(), 5);
}

#[test]
fn a_same_length_rewrite_is_detected_by_its_modification_time() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    fs::write(&path, log(&[1])).unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let modified = file.metadata().unwrap().modified().unwrap();
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["1"]);

    fs::write(&path, log(&[2])).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(2)))
        .unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["2"]);
}

#[test]
fn replacement_is_detected_even_when_length_and_modification_time_match() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    fs::write(&path, log(&[1])).unwrap();
    let original = File::open(&path).unwrap();
    let modified = original.metadata().unwrap().modified().unwrap();
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["1"]);

    let replacement = dir.path().join("replacement");
    fs::write(&replacement, log(&[2])).unwrap();
    File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::rename(replacement, &path).unwrap();
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["2"]);
    assert_ne!(
        original.metadata().unwrap().ino(),
        fs::metadata(path).unwrap().ino()
    );
}

#[test]
fn a_warm_snapshot_never_serves_rows_after_a_log_becomes_a_link_or_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let elsewhere = dir.path().join("elsewhere");
    fs::write(&path, log(&[1])).unwrap();
    fs::write(&elsewhere, log(&[2])).unwrap();
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(dir.path(), 10)), ["1"]);

    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert!(cache.recent(dir.path(), 10).is_empty());
    fs::remove_file(&path).unwrap();
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
    assert!(cache.recent(dir.path(), 10).is_empty());
}

#[test]
fn selecting_a_shorter_limit_does_not_change_cached_input_run_counts() {
    let dir = tempfile::tempdir().unwrap();
    let person = |at| {
        line(
            json!({"at": at, "operation": "input", "actor": "viewer", "actionCount": 1, "ok": true}),
        )
    };
    fs::write(
        dir.path().join(FILE),
        person("1000000") + &person("2000000"),
    )
    .unwrap();
    let probe = Observation::watch(dir.path());
    let mut cache = Cache { kept: None };
    for limit in [1, 200, 12, 64, 1] {
        let rows = cache.recent(dir.path(), limit);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action_count, Some(2));
        assert_eq!(rows[0].logical_id(), "1000000");
    }
    assert_eq!(probe.threads().len(), 1);
    assert_eq!(cache.kept.as_ref().unwrap().rows.len(), 2);
}

#[test]
fn visiting_another_directory_replaces_the_retained_snapshot() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join(FILE), log(&[1])).unwrap();
    fs::write(second.path().join(FILE), log(&[2])).unwrap();
    let mut cache = Cache { kept: None };
    assert_eq!(times(&cache.recent(first.path(), 10)), ["1"]);
    assert_eq!(times(&cache.recent(second.path(), 10)), ["2"]);
    assert_eq!(cache.kept.as_ref().unwrap().directory, second.path());
    assert_eq!(times(&cache.recent(first.path(), 10)), ["1"]);
}
