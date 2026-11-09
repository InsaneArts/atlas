use super::super::{FILE, HISTORY, PREVIOUS};
use super::*;
use crate::host_session::record::Record;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    fs,
    os::unix::fs::PermissionsExt,
};

/// What happens as a file is about to be read on this thread.
type BeforeScan = Box<dyn FnMut(&Path)>;

thread_local! {
    static SCANNED: Cell<usize> = const { Cell::new(0) };
    static BEFORE_SCAN: RefCell<Option<BeforeScan>> = const { RefCell::new(None) };
}

/// Count a file about to be read on this thread, once it is open, and do
/// what a test asked to then.
pub(super) fn scanned(path: &Path) {
    SCANNED.with(|count| count.set(count.get() + 1));
    let hook = BEFORE_SCAN.with(|hook| hook.borrow_mut().take());
    if let Some(mut hook) = hook {
        hook(path);
        BEFORE_SCAN.with(|kept| {
            kept.borrow_mut().get_or_insert(hook);
        });
    }
}

/// How many files this thread read since it last asked.
fn files_read() -> usize {
    SCANNED.with(|count| count.replace(0))
}

const KEY: &str = "0123456789ab";
const OTHER: &str = "ba9876543210";

fn key(text: &str) -> Key {
    Key::parse(text).unwrap()
}

/// An agent's command at `at` seconds, in `session` when it names one.
fn step(at: u64, session: Option<&str>) -> String {
    let mut row = json!({"at": (u128::from(at) * 1_000_000_000).to_string(),
        "operation": "input", "ok": true, "actor": "agent", "verb": "click",
        "command": format!("{at:012}-{}", session.unwrap_or("none"))});
    if let Some(session) = session {
        row["session"] = json!(session);
    }
    format!("{row}\n")
}

fn person(at: u64) -> String {
    format!(
        "{}\n",
        json!({"at": (u128::from(at) * 1_000_000_000).to_string(), "operation": "input",
            "ok": true, "actor": "viewer", "session": KEY})
    )
}

/// A folder whose log holds `files`, newest first: the current file, the
/// previous one, then the history.
fn folder(files: &[String]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join(HISTORY);
    fs::create_dir(&history).unwrap();
    fs::set_permissions(&history, fs::Permissions::from_mode(0o700)).unwrap();
    for (index, text) in files.iter().enumerate() {
        let path = match index {
            0 => dir.path().join(FILE),
            1 => dir.path().join(PREVIOUS),
            _ => history.join(format!("{}.jsonl", files.len() - index)),
        };
        fs::write(path, text).unwrap();
    }
    dir
}

fn seconds(rows: &SessionRows) -> Vec<u64> {
    rows.rows
        .iter()
        .map(|row| u64::try_from(row.at_nanos().unwrap() / 1_000_000_000).unwrap())
        .collect()
}

/// A cold read of `whose` rows, as a new process reads them.
fn rows(directory: &Path, whose: &Whose, limit: usize) -> SessionRows {
    Cache { kept: Vec::new() }
        .rows(directory, whose, limit)
        .unwrap()
}

fn record(started_ms: u64, actions: u64) -> Record {
    let mut record: Record = serde_json::from_value(json!({
        "version": 1, "key": KEY, "host": "codex", "hostSession": "demo",
        "started": started_ms, "lastSeen": started_ms + 60_000,
    }))
    .unwrap();
    if actions > 0 {
        record.counts.insert("click".into(), actions);
    }
    record
}

#[test]
fn a_sessions_rows_are_found_in_the_oldest_kept_file_newest_first() {
    let dir = folder(&[
        step(9, Some(OTHER)),
        step(8, Some(OTHER)),
        step(5, Some(OTHER)),
        step(1, Some(KEY)) + &step(2, Some(KEY)) + &step(3, Some(OTHER)),
    ]);
    files_read();
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(seconds(&found), [2, 1]);
    assert_eq!(found.reach, Reach::Since(1_000));
    assert_eq!(files_read(), 4, "every file is read once");
    assert!(
        found
            .rows
            .iter()
            .all(|row| row.session.as_deref() == Some(KEY))
    );
}

#[test]
fn a_session_read_never_lists_the_persons_input_or_another_sessions() {
    let dir = folder(&[person(4) + &step(3, Some(OTHER)) + &step(2, Some(KEY)) + &step(1, None)]);
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(seconds(&found), [2]);
    let terminal = rows(dir.path(), &Whose::Terminal, 200);
    assert_eq!(seconds(&terminal), [1]);
}

#[test]
fn a_key_elsewhere_in_a_row_is_not_its_session() {
    let mut row = json!({"at": "5000000000", "operation": "input", "ok": true,
        "actor": "agent", "task": KEY, "session": OTHER});
    let mut text = format!("{row}\n");
    row["session"] = json!(KEY);
    row["task"] = json!(KEY);
    row["at"] = json!("6000000000");
    text += &format!("{row}\n");
    let dir = folder(&[text]);
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(
        seconds(&found),
        [6],
        "one row, read once though it names the key twice"
    );
}

#[test]
fn a_session_read_stops_once_it_has_enough_rows() {
    let newest: String = (10..20).map(|at| step(at, Some(KEY))).collect();
    let dir = folder(&[newest, step(5, Some(KEY))]);
    files_read();
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 4);
    assert_eq!(seconds(&found), [19, 18, 17, 16]);
    assert_eq!(found.reach, Reach::Unknown);
    assert_eq!(files_read(), 1, "the previous file is never read");
}

#[test]
fn a_long_commands_many_steps_in_one_file_are_kept_as_its_last() {
    let steps: String = (1_u64..=50)
        .map(|at| {
            format!(
                "{}\n",
                json!({"at": (u128::from(at) * 1_000_000_000).to_string(), "operation": "input",
                    "ok": true, "actor": "agent", "command": "typing", "session": KEY})
            )
        })
        .collect();
    let dir = folder(&[step(60, Some(KEY)), steps]);
    let mut cache = Cache { kept: Vec::new() };
    let found = cache.rows(dir.path(), &Whose::Host(key(KEY)), 200).unwrap();
    assert_eq!(seconds(&found), [60, 50]);
    let kept: usize = cache.kept[0].scans.iter().map(|scan| scan.rows.len()).sum();
    assert_eq!(kept, 2, "one row a command is kept");
}

#[test]
fn a_history_listed_only_in_part_never_says_how_far_back_the_log_reaches() {
    let dir = folder(&[step(9, Some(OTHER))]);
    for number in 0..=1024 {
        fs::write(dir.path().join(HISTORY).join(format!("x{number}")), "").unwrap();
    }
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert!(found.rows.is_empty());
    assert_eq!(found.reach, Reach::Unknown);
    let mut wrote = record(5_000, 3);
    wrote.logs_rows = true;
    wrote.logged = Some(1);
    assert!(!found.cleared(&wrote));
}

#[test]
fn a_commands_newest_step_stands_for_it_across_files() {
    let start = format!(
        "{}\n",
        json!({"at": "1000000000", "operation": "run", "started": true, "actor": "agent",
            "command": "cmd", "session": KEY})
    );
    let end = format!(
        "{}\n",
        json!({"at": "2000000000", "operation": "run", "ok": true, "actor": "agent",
            "command": "cmd", "session": KEY})
    );
    let dir = folder(&[end, start]);
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(seconds(&found), [2]);
    assert!(found.rows[0].worked());
}

#[test]
fn an_unchanged_log_is_read_again_from_what_was_kept_and_only_a_grown_file_is_read() {
    let dir = folder(&[
        step(9, Some(OTHER)),
        step(8, Some(OTHER)),
        step(5, Some(OTHER)),
        step(1, Some(KEY)),
    ]);
    let whose = Whose::Host(key(KEY));
    let mut cache = Cache { kept: Vec::new() };
    files_read();
    let first = cache.rows(dir.path(), &whose, 200).unwrap();
    assert_eq!(files_read(), 4);
    assert_eq!(cache.rows(dir.path(), &whose, 200).unwrap(), first);
    assert_eq!(files_read(), 0);

    let mut current = fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join(FILE))
        .unwrap();
    std::io::Write::write_all(&mut current, step(10, Some(KEY)).as_bytes()).unwrap();
    let grown = cache.rows(dir.path(), &whose, 200).unwrap();
    assert_eq!(files_read(), 1);
    assert_eq!(seconds(&grown), [10, 1]);

    // Going to another session and back keeps the first one's read.
    cache
        .rows(dir.path(), &Whose::Host(key(OTHER)), 200)
        .unwrap();
    files_read();
    assert_eq!(cache.rows(dir.path(), &whose, 200).unwrap(), grown);
    assert_eq!(files_read(), 0);
}

#[test]
fn a_read_that_the_log_starting_over_twice_overtook_is_read_again_in_order() {
    let start = format!(
        "{}\n",
        json!({"at": "1000000000", "operation": "run", "started": true, "actor": "agent",
            "command": "cmd", "session": KEY})
    );
    let end = format!(
        "{}\n",
        json!({"at": "2000000000", "operation": "run", "ok": true, "actor": "agent",
            "command": "cmd", "session": KEY})
    );
    let dir = folder(&[start]);
    let directory = dir.path().to_owned();
    let mut started_over = false;
    BEFORE_SCAN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |path: &Path| {
            if started_over || path != directory.join(FILE) {
                return;
            }
            started_over = true;
            // Once with the command's end, then once more with another row,
            // while the first current file is open but not yet read.
            for next in [end.clone(), step(3, Some(OTHER))] {
                retained::rotate(&directory).unwrap();
                fs::write(directory.join(FILE), next).unwrap();
            }
        }));
    });
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    BEFORE_SCAN.with(|hook| hook.borrow_mut().take());
    assert_eq!(seconds(&found), [2]);
    assert!(
        found.rows[0].worked(),
        "the command's last step stands for it"
    );

    // A log that starts over during every try is not read out of order.
    let directory = dir.path().to_owned();
    BEFORE_SCAN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |path: &Path| {
            if path == directory.join(FILE) {
                retained::rotate(&directory).unwrap();
                fs::write(directory.join(FILE), step(4, Some(OTHER))).unwrap();
            }
        }));
    });
    let error = Cache { kept: Vec::new() }
        .rows(dir.path(), &Whose::Host(key(KEY)), 200)
        .unwrap_err();
    BEFORE_SCAN.with(|hook| hook.borrow_mut().take());
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
}

#[test]
fn a_read_begun_while_the_log_had_no_current_file_is_read_again_in_order() {
    let row = |at: &str, ended: bool| {
        let mut row = json!({"at": at, "operation": "run", "actor": "agent", "command": "cmd",
            "session": KEY});
        row[if ended { "ok" } else { "started" }] = json!(true);
        format!("{row}\n")
    };
    // Just started over: a previous file, and no current one yet.
    let dir = folder(&[String::new(), row("1000000000", false)]);
    fs::remove_file(dir.path().join(FILE)).unwrap();
    let directory = dir.path().to_owned();
    let mut started_over = false;
    let end = row("2000000000", true);
    BEFORE_SCAN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move |path: &Path| {
            if started_over || path != directory.join(PREVIOUS) {
                return;
            }
            started_over = true;
            for next in [end.clone(), step(3, Some(OTHER))] {
                fs::write(directory.join(FILE), next).unwrap();
                retained::rotate(&directory).unwrap();
            }
        }));
    });
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    BEFORE_SCAN.with(|hook| hook.borrow_mut().take());
    assert_eq!(seconds(&found), [2]);
    assert!(
        found.rows[0].worked(),
        "the command's last step stands for it"
    );
}

#[test]
fn a_read_that_stops_at_the_current_file_is_not_read_again_for_a_previous_one() {
    let newest: String = (10..20).map(|at| step(at, Some(KEY))).collect();
    let dir = folder(&[newest, step(5, Some(KEY))]);
    files_read();
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 4);
    assert_eq!(seconds(&found), [19, 18, 17, 16]);
    assert_eq!(files_read(), 1);
}

#[test]
fn a_file_that_is_not_plain_is_refused_rather_than_read_as_nothing() {
    let dir = folder(&[step(1, Some(KEY))]);
    fs::remove_file(dir.path().join(FILE)).unwrap();
    fs::create_dir(dir.path().join(FILE)).unwrap();
    assert!(session_rows(dir.path(), &Whose::Host(key(KEY)), 200).is_err());
}

#[test]
fn an_unfinished_last_line_is_passed_over() {
    let unfinished = step(2, Some(KEY));
    let dir = folder(&[step(1, Some(KEY)) + unfinished.trim_end()]);
    let found = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(seconds(&found), [1]);
}

#[test]
fn a_session_is_cleared_only_once_the_log_starts_after_the_newest_row_it_wrote() {
    let empty = |reach| SessionRows {
        rows: Vec::new(),
        reach,
    };
    // Started at 5 s, its newest row written at 20 s, seen last at 65 s.
    let mut wrote = record(5_000, 3);
    wrote.logs_rows = true;
    wrote.logged = Some(20_000);
    assert!(empty(Reach::Since(21_000)).cleared(&wrote));
    assert!(
        !empty(Reach::Since(19_000)).cleared(&wrote),
        "the log reaches back to its newest row"
    );
    assert!(
        !empty(Reach::Nothing).cleared(&wrote),
        "a log with no rows was never filled"
    );
    assert!(!empty(Reach::Unknown).cleared(&wrote));
    let mut found = empty(Reach::Since(21_000));
    found.rows = rows(
        folder(&[step(1, Some(KEY))]).path(),
        &Whose::Host(key(KEY)),
        200,
    )
    .rows;
    assert!(!found.cleared(&wrote), "a row is kept");

    // Counted but never logged, as a command refused before it reached the
    // helper: nothing of it was cleared.
    let mut refused = record(5_000, 3);
    refused.logs_rows = true;
    assert!(!empty(Reach::Since(100_000)).cleared(&refused));

    let dir = folder(&[step(30, Some(OTHER))]);
    let read = rows(dir.path(), &Whose::Host(key(KEY)), 200);
    assert_eq!(read.reach, Reach::Since(30_000));
    assert!(read.cleared(&wrote));
}

#[test]
fn an_older_builds_record_is_cleared_only_once_the_log_starts_after_its_last_command() {
    let empty = |reach| SessionRows {
        rows: Vec::new(),
        reach,
    };
    // Started at 5 s and seen last at 65 s, with no time of its newest row.
    let acted = record(5_000, 3);
    assert!(empty(Reach::Since(66_000)).cleared(&acted));
    assert!(
        !empty(Reach::Since(64_000)).cleared(&acted),
        "the log may still reach a row it wrote late"
    );
    assert!(!empty(Reach::Nothing).cleared(&acted));
    assert!(
        !empty(Reach::Since(66_000)).cleared(&record(5_000, 0)),
        "it did nothing"
    );
    let mut rowless = record(5_000, 0);
    rowless.counts.insert(ENVIRONMENT_VERB.into(), 2);
    rowless.counts.insert("cleanup".into(), 1);
    assert!(
        !empty(Reach::Since(66_000)).cleared(&rowless),
        "its environment work and cleanup may have kept no row"
    );
}
