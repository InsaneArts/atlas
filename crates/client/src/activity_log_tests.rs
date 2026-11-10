use super::*;
use serde_json::json;
use std::{fs, io::Write};

#[test]
fn created_and_rotated_activity_files_keep_the_writers_private_permissions() {
    use std::os::unix::fs::MetadataExt;

    let directory = tempfile::tempdir().unwrap();
    let row = Row::parse(&agents(1)).unwrap();
    append(directory.path(), &row).unwrap();
    let mut large = row.clone();
    large.actor = Some("a".repeat(ROTATE_AT as usize));
    append(directory.path(), &large).unwrap();
    assert!(fs::metadata(directory.path().join(FILE)).unwrap().len() > ROTATE_AT);
    assert!(!directory.path().join(PREVIOUS).exists());

    append(directory.path(), &row).unwrap();
    for name in [FILE, PREVIOUS, LOCK] {
        let metadata = fs::symlink_metadata(directory.path().join(name)).unwrap();
        assert!(metadata.is_file(), "{name}");
        assert_eq!(metadata.uid(), nix::unistd::getuid().as_raw(), "{name}");
        assert_eq!(metadata.mode() & 0o777, 0o600, "{name}");
    }
    assert!(fs::metadata(directory.path().join(PREVIOUS)).unwrap().len() > ROTATE_AT);
    // All three steps name the same command, so its newest row is shown.
    assert_eq!(times(&recent(directory.path(), 3)), ["1"]);
}

#[test]
fn a_failed_append_rolls_back_its_prefix_before_the_row_is_retried() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE);
    let original = log([agents(1)]);
    let row = Row::parse(&agents(2)).unwrap();
    for fraction in 0..=4 {
        fs::write(&path, &original).unwrap();
        let error = append_using(directory.path(), &row, |file, bytes| {
            file.write_all(&bytes[..bytes.len() * fraction / 4])?;
            Err(std::io::Error::other("simulated disk exhaustion"))
        })
        .unwrap_err();
        assert!(error.error.contains("simulated disk exhaustion"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        append(directory.path(), &row).unwrap();
        assert_eq!(times(&recent(directory.path(), 5)), ["2", "1"]);
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
    }
}

#[test]
fn an_unfinished_tail_is_removed_before_an_append_or_rotation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE);
    let row = Row::parse(&agents(2)).unwrap();
    let tails = [
        "{\"operation\":".to_owned(),
        agents(2).to_string(),
        "x".repeat(ROTATE_AT as usize + 1),
    ];
    for original in [String::new(), log([agents(1)])] {
        for tail in &tails {
            fs::write(&path, format!("{original}{tail}")).unwrap();
            append(directory.path(), &row).unwrap();
            let expected = format!("{original}{}\n", serde_json::to_value(&row).unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
            assert!(!directory.path().join(PREVIOUS).exists());
        }
    }
}

#[test]
fn a_failed_append_after_rotation_keeps_the_previous_rows_and_can_be_retried() {
    let directory = tempfile::tempdir().unwrap();
    let original = log([agents(1)]).repeat(ROTATE_AT as usize / log([agents(1)]).len() + 1);
    fs::write(directory.path().join(FILE), &original).unwrap();
    let row = Row::parse(&agents(2)).unwrap();
    assert!(
        append_using(directory.path(), &row, |file, bytes| {
            file.write_all(&bytes[..bytes.len() / 2])?;
            Err(std::io::Error::other("simulated disk exhaustion"))
        })
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(directory.path().join(PREVIOUS)).unwrap(),
        original
    );
    assert_eq!(fs::metadata(directory.path().join(FILE)).unwrap().len(), 0);
    append(directory.path(), &row).unwrap();
    assert_eq!(times(&recent(directory.path(), 2)), ["2", "1"]);
}

#[test]
fn the_activity_log_is_read_only_as_a_plain_file_never_a_link_or_a_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = dir.path().join("elsewhere.jsonl");
    fs::write(&elsewhere, "{\"at\":\"1\",\"operation\":\"input\"}\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.path().join(FILE)).unwrap();
    assert!(recent(dir.path(), 5).is_empty(), "a link");

    fs::remove_file(dir.path().join(FILE)).unwrap();
    nix::unistd::mkfifo(
        &dir.path().join(FILE),
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    // A pipe would hold a blocking open forever; this returns at once.
    assert!(recent(dir.path(), 5).is_empty(), "a pipe");
}

#[test]
fn a_command_of_several_steps_is_one_row() {
    let dir = tempfile::tempdir().unwrap();
    let rows = [
        json!({"operation": "run", "ok": true, "verb": "open", "command": "a"}),
        json!({"operation": "input", "ok": true, "verb": "type", "command": "b"}),
        json!({"operation": "input", "ok": false, "verb": "type", "command": "b"}),
        json!({"operation": "input", "ok": true}),
        json!({"operation": "input", "ok": true}),
    ];
    let log = rows
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    fs::write(dir.path().join(FILE), log).unwrap();

    let shown = recent(dir.path(), 12);
    let says = shown.iter().map(crate::doing::says).collect::<Vec<_>>();
    assert_eq!(
        says,
        [
            "used the mouse and keyboard",
            "used the mouse and keyboard",
            "couldn't type",
            "opened a file"
        ],
        "the newest step stands for its command; rows of no command stay apart"
    );
    assert_eq!(recent(dir.path(), 2).len(), 2);
}

#[test]
fn the_newest_rows_are_read_from_the_logs_tail() {
    let dir = tempfile::tempdir().unwrap();
    assert!(recent(dir.path(), 5).is_empty());

    let rows: String = (0..30)
        .map(|i| format!("{{\"at\":\"{i}\",\"operation\":\"input\",\"ok\":true}}\n"))
        .chain(["not json\n".to_owned()])
        .collect();
    fs::write(dir.path().join(FILE), rows).unwrap();
    let newest = recent(dir.path(), 3);
    assert_eq!(newest.len(), 3);
    assert_eq!(newest[0].at.as_deref(), Some("29"));
    assert_eq!(newest[2].at.as_deref(), Some("27"));
}

#[test]
fn a_complete_row_without_its_newline_is_not_read_as_written() {
    for name in [FILE, PREVIOUS] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let mut start = agents(1);
        start["ok"] = Value::Null;
        start["started"] = json!(true);
        let mut end = agents(2);
        end["command"] = start["command"].clone();
        for original in [String::new(), log([start.clone()])] {
            fs::write(&path, format!("{original}{end}")).unwrap();
            let shown = recent(dir.path(), 5);
            if original.is_empty() {
                assert!(shown.is_empty());
            } else {
                assert_eq!(times(&shown), ["1"]);
                assert!(shown[0].under_way());
            }

            OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"\n")
                .unwrap();
            let shown = recent(dir.path(), 5);
            assert_eq!(times(&shown), ["2"]);
            assert!(shown[0].worked());
        }
    }
}

#[test]
fn a_row_split_across_a_chunk_boundary_still_reads_once_it_is_terminated() {
    for name in [FILE, PREVIOUS] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let mut row = agents(2);
        row["pad"] = json!("é".repeat(TAIL as usize));
        let line = row.to_string();
        assert!(line.len() as u64 > 2 * TAIL);
        fs::write(&path, format!("{}{line}", log([agents(1)]))).unwrap();
        assert_eq!(times(&recent(dir.path(), 5)), ["1"]);

        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(format!("\n{}", log([agents(3)])).as_bytes())
            .unwrap();
        assert_eq!(times(&recent(dir.path(), 5)), ["3", "2", "1"]);
    }
}

#[test]
fn a_tail_that_starts_inside_a_character_still_reads() {
    let dir = tempfile::tempdir().unwrap();
    let mut text = format!(
        "{{\"operation\":\"old\",\"pad\":\"{}\"}}\n{{\"at\":\"1\",\"operation\":\"input\"}}\n",
        "é".repeat(40_000)
    );
    let cut = |text: &str| text.as_bytes()[text.len() - TAIL as usize];
    if cut(&text) & 0xC0 != 0x80 {
        text.push('\n');
    }

    assert_eq!(cut(&text) & 0xC0, 0x80, "the tail starts inside é");
    fs::write(dir.path().join(FILE), text).unwrap();
    let rows = recent(dir.path(), 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].at.as_deref(), Some("1"));

    let rows = recent(dir.path(), 5);
    assert_eq!(rows.len(), 2, "the line the tail cut is read whole");
    assert_eq!(rows[1].operation, Some(Op::Other("old".into())));
}

/// `rows` as the log's lines.
fn log(rows: impl IntoIterator<Item = Value>) -> String {
    rows.into_iter().map(|row| format!("{row}\n")).collect()
}

/// One of the person's inputs through a viewer, at `at`.
fn persons(at: u64) -> Value {
    json!({"at": at.to_string(), "operation": "input", "ok": true, "actionCount": 1,
        "actor": "viewer", "elapsedMs": 2.5})
}

/// An agent's click, at `at`.
fn agents(at: u64) -> Value {
    json!({"at": at.to_string(), "operation": "input", "ok": true, "actionCount": 1,
        "actor": "agent", "verb": "click", "command": format!("{at:012}")})
}

#[test]
fn delayed_input_cannot_displace_a_newer_action_even_across_chunks_or_rotation() {
    let directory = tempfile::tempdir().unwrap();
    for rotated in [false, true] {
        let older = log([agents(1), agents(3)]);
        let delayed = format!("{}{}", filler(TAIL as usize), log([persons(2)]));
        if rotated {
            fs::write(directory.path().join(PREVIOUS), older).unwrap();
            fs::write(directory.path().join(FILE), delayed).unwrap();
        } else {
            fs::write(directory.path().join(FILE), format!("{older}{delayed}")).unwrap();
        }
        assert_eq!(times(&recent(directory.path(), 1)), ["3"]);
        assert_eq!(times(&recent(directory.path(), 3)), ["3", "2", "1"]);
    }
}

#[test]
fn timestamps_are_ordered_numerically_and_ties_keep_reverse_append_order() {
    let directory = tempfile::tempdir().unwrap();
    let rows = [
        json!({"operation": "read", "at": "10", "app": "first"}),
        json!({"operation": "read", "at": "10", "app": "second"}),
        json!({"operation": "read", "at": "9", "app": "older"}),
        json!({"operation": "read", "app": "missing"}),
        json!({"operation": "read", "at": "bad", "app": "invalid"}),
    ];
    fs::write(directory.path().join(FILE), log(rows)).unwrap();
    let shown = recent(directory.path(), 10);
    assert_eq!(
        shown
            .iter()
            .map(|row| row.app.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["second", "first", "older", "invalid", "missing"]
    );
}

#[test]
fn interleaved_command_steps_are_counted_once_before_the_limit_is_applied() {
    let directory = tempfile::tempdir().unwrap();
    let mut start = agents(1);
    start["ok"] = Value::Null;
    start["started"] = json!(true);
    let mut end = agents(3);
    end["command"] = start["command"].clone();
    fs::write(
        directory.path().join(FILE),
        log([start, persons(2), end, persons(0)]),
    )
    .unwrap();
    let shown = recent(directory.path(), 3);
    assert_eq!(times(&shown), ["3", "2"]);
    assert_eq!(shown[0].ok, Some(true));
    assert_eq!(shown[1].action_count, Some(2));
}

#[test]
fn a_run_of_the_persons_input_counts_once_so_agents_rows_stay_in_reach() {
    let dir = tempfile::tempdir().unwrap();
    let rows = (0..3)
        .map(agents)
        .chain((10..60).map(persons))
        .chain([agents(70)])
        .chain((80..90).map(persons));
    fs::write(dir.path().join(FILE), log(rows)).unwrap();

    let shown = recent(dir.path(), 5);
    let read = shown
        .iter()
        .map(|row| (row.actor(), row.at.as_deref().unwrap(), row.action_count))
        .collect::<Vec<_>>();
    assert_eq!(
        read,
        [
            ("viewer", "89", Some(10)),
            ("agent", "70", Some(1)),
            ("viewer", "59", Some(50)),
            ("agent", "2", Some(1)),
            ("agent", "1", Some(1)),
        ],
        "the newest of a run stands for it, with every action it sent"
    );

    let mut failed = persons(91);
    failed["ok"] = json!(false);
    failed["code"] = json!("busy");
    let mut file = OpenOptions::new()
        .append(true)
        .open(dir.path().join(FILE))
        .unwrap();
    file.write_all(log([failed]).as_bytes()).unwrap();
    let shown = recent(dir.path(), 2);
    assert_eq!(
        shown.iter().map(Row::failed).collect::<Vec<_>>(),
        [true, false],
        "a failure is never folded into input that worked"
    );
}

#[test]
fn accepted_action_counts_saturate_when_a_personal_input_run_overflows() {
    let dir = tempfile::tempdir().unwrap();
    for counts in [
        [u64::MAX, 1],
        [1, u64::MAX],
        [u64::MAX, u64::MAX],
        [u64::MAX - 1, 1],
    ] {
        let rows = counts.into_iter().enumerate().map(|(at, count)| {
            let mut row = persons(at as u64);
            row["actionCount"] = json!(count);
            row
        });
        fs::write(dir.path().join(FILE), log(rows)).unwrap();
        let shown = recent(dir.path(), 1);
        assert_eq!(shown.len(), 1, "{counts:?}");
        assert_eq!(shown[0].action_count, Some(u64::MAX), "{counts:?}");
        assert_eq!(shown[0].at.as_deref(), Some("1"));
    }
}

#[test]
fn personal_input_after_a_pause_remains_separate() {
    let dir = tempfile::tempdir().unwrap();
    let rows = [1, INPUT_GAP_MS + 2].map(|at| {
        let mut row = persons(0);
        row["at"] = json!((u128::from(at) * 1_000_000).to_string());
        row
    });
    fs::write(dir.path().join(FILE), log(rows)).unwrap();
    let shown = recent(dir.path(), 5);
    assert_eq!(shown.len(), 2);
    assert!(shown.iter().all(|row| row.action_count == Some(1)));
}

#[test]
fn personal_input_at_local_midnight_stays_separate_across_rotation() {
    if !atlas_core::test_support::in_timezone(
        "activity_log::tests::personal_input_at_local_midnight_stays_separate_across_rotation",
        "Asia/Tbilisi",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let row = |at: u64| {
        let mut row = persons(0);
        row["at"] = json!((u128::from(at) * 1_000_000).to_string());
        row
    };
    // Older builds wrote individual inputs; both stages must preserve the day.
    fs::write(
        dir.path().join(PREVIOUS),
        log([row(71_999_998), row(71_999_999)]),
    )
    .unwrap();
    fs::write(
        dir.path().join(FILE),
        log([row(72_000_000), row(72_000_001)]),
    )
    .unwrap();
    for limit in [1, 5] {
        let shown = recent(dir.path(), limit);
        assert_eq!(shown.len(), limit.min(2));
        assert_eq!(shown[0].at_ms(), Some(72_000_001));
        assert!(shown.iter().all(|row| row.action_count == Some(2)));
        if limit > 1 {
            assert_eq!(shown[1].at_ms(), Some(71_999_999));
        }
    }
}

#[test]
fn continuous_personal_input_compares_adjacent_times_across_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let row = |at: u64| {
        let mut row = persons(0);
        row["at"] = json!((u128::from(at) * 1_000_000).to_string());
        row
    };
    fs::write(dir.path().join(PREVIOUS), log([row(0), row(INPUT_GAP_MS)])).unwrap();
    fs::write(
        dir.path().join(FILE),
        log([row(2 * INPUT_GAP_MS), row(3 * INPUT_GAP_MS)]),
    )
    .unwrap();
    let shown = recent(dir.path(), 1);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].action_count, Some(4));
    assert_eq!(shown[0].at_ms(), Some(3 * INPUT_GAP_MS));
}

#[test]
fn the_last_selected_input_run_counts_every_action_across_chunks_and_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let count = 2_000;
    let current = log((count..2 * count).map(persons));
    assert!(current.len() as u64 > TAIL);
    fs::write(dir.path().join(FILE), current).unwrap();
    fs::write(
        dir.path().join(PREVIOUS),
        log([agents(0)].into_iter().chain((1..count).map(persons))),
    )
    .unwrap();

    let one = recent(dir.path(), 1);
    let several = recent(dir.path(), 5);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].action_count, Some(2 * count - 1));
    assert_eq!(one[0].action_count, several[0].action_count);
    assert_eq!(several.len(), 2);
}

#[test]
fn completing_the_last_input_run_stays_within_the_byte_budget() {
    let dir = tempfile::tempdir().unwrap();
    let line = log([persons(1)]);
    fs::write(
        dir.path().join(FILE),
        line.repeat((READ_LIMIT as usize / line.len()) + 2),
    )
    .unwrap();
    let mut room = READ_LIMIT;
    let mut rows = Vec::new();
    read_back(
        fs::File::open(dir.path().join(FILE)).unwrap(),
        &mut room,
        &mut rows,
    )
    .unwrap();
    assert_eq!(room, 0);
    let expected = ((READ_LIMIT - 1) / line.len() as u64) as usize;
    assert_eq!(rows.len(), expected);
    assert_eq!(recent(dir.path(), 1)[0].action_count, Some(expected as u64));
    assert_eq!(
        try_recent(dir.path(), 1).unwrap()[0].action_count,
        Some(expected as u64)
    );
}

#[test]
fn a_long_run_of_the_persons_input_past_the_first_tail_still_finds_the_agents_rows_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let run = 20_000;
    let rows = (0..3).map(agents).chain((10..10 + run).map(persons));
    let text = log(rows);
    assert!(text.len() as u64 > 10 * TAIL && (text.len() as u64) < READ_LIMIT);
    fs::write(dir.path().join(FILE), text).unwrap();

    let shown = recent(dir.path(), 200);
    assert_eq!(shown.len(), 4);
    assert_eq!(shown[0].action_count, Some(run));
    assert_eq!(
        shown[1..]
            .iter()
            .map(|row| row.at.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["2", "1", "0"]
    );
}

#[test]
fn the_read_never_goes_past_its_byte_cap() {
    let dir = tempfile::tempdir().unwrap();
    let filler = format!("{{\"pad\":\"{}\"}}\n", "x".repeat(1000));
    let lines = READ_LIMIT as usize / filler.len() + 1;
    let text = format!(
        "{}{}{}",
        log([agents(0)]),
        filler.repeat(lines),
        log([agents(9)])
    );
    assert!(text.len() as u64 > READ_LIMIT);
    fs::write(dir.path().join(FILE), &text).unwrap();

    let shown = recent(dir.path(), 5);
    assert_eq!(
        shown
            .iter()
            .map(|row| row.at.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["9"],
        "the row past the cap is never reached"
    );

    let near = format!(
        "{}{}{}",
        log([agents(0)]),
        filler.repeat(lines / 2),
        log([agents(9)])
    );
    fs::write(dir.path().join(FILE), near).unwrap();
    assert_eq!(recent(dir.path(), 5).len(), 2, "one within it is");
}

/// The `at` of each of `rows`, in order.
fn times(rows: &[Row]) -> Vec<&str> {
    rows.iter()
        .map(|row| row.at.as_deref().unwrap_or_default())
        .collect()
}

/// A line that names no operation, about `bytes` long, to fill a log.
fn filler(bytes: usize) -> String {
    format!("{{\"pad\":\"{}\"}}\n", "x".repeat(bytes))
}

#[test]
fn right_after_the_log_rotates_the_rows_before_it_still_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(PREVIOUS), log((0..5).map(agents))).unwrap();
    fs::write(dir.path().join(FILE), log((10..12).map(agents))).unwrap();

    assert_eq!(
        times(&recent(dir.path(), 5)),
        ["11", "10", "4", "3", "2"],
        "the new log's rows, then the previous log's newest"
    );
    assert_eq!(
        times(&recent(dir.path(), 2)),
        ["11", "10"],
        "the limit applies after both logs are read"
    );

    fs::remove_file(dir.path().join(FILE)).unwrap();
    assert_eq!(
        times(&recent(dir.path(), 2)),
        ["4", "3"],
        "nor is it lost in the moment before the new log is made"
    );
}

#[test]
fn a_command_or_a_run_of_the_persons_input_across_the_rotation_is_one_row() {
    let dir = tempfile::tempdir().unwrap();
    let step = |at: u64, ok: bool| {
        json!({"at": at.to_string(), "operation": "input", "ok": ok, "verb": "type",
            "command": "typing"})
    };
    fs::write(dir.path().join(PREVIOUS), log([agents(0), step(1, true)])).unwrap();
    fs::write(dir.path().join(FILE), log([step(2, false)])).unwrap();
    assert_eq!(
        times(&recent(dir.path(), 5)),
        ["2", "0"],
        "the newest step stands for its command"
    );

    fs::write(
        dir.path().join(PREVIOUS),
        log([agents(0), persons(1), persons(2)]),
    )
    .unwrap();
    fs::write(dir.path().join(FILE), log([persons(3)])).unwrap();
    let shown = recent(dir.path(), 5);
    assert_eq!(times(&shown), ["3", "0"]);
    assert_eq!(
        shown[0].action_count,
        Some(3),
        "the run counts every action"
    );
}

#[test]
fn the_previous_log_is_read_within_the_same_byte_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mib = 1024 * 1024;
    let current = format!(
        "{}{}{}",
        log([agents(5)]),
        filler(3 * mib),
        log([agents(9)])
    );
    let previous = format!(
        "{}{}{}",
        log([agents(0)]),
        filler(2 * mib),
        log([agents(1)])
    );
    assert!((current.len() as u64) < READ_LIMIT);
    assert!(((current.len() + previous.len()) as u64) > READ_LIMIT);
    fs::write(dir.path().join(FILE), current).unwrap();
    fs::write(dir.path().join(PREVIOUS), previous).unwrap();

    assert_eq!(
        times(&recent(dir.path(), 5)),
        ["9", "5", "1"],
        "the previous log's oldest row is past what one read goes through"
    );
}

#[test]
fn the_previous_log_is_never_read_twice_or_through_a_link() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(FILE), log((0..3).map(agents))).unwrap();
    fs::hard_link(dir.path().join(FILE), dir.path().join(PREVIOUS)).unwrap();
    assert_eq!(
        times(&recent(dir.path(), 10)),
        ["2", "1", "0"],
        "the same file under both names, as when the log rotates mid-read"
    );

    fs::remove_file(dir.path().join(PREVIOUS)).unwrap();
    let elsewhere = dir.path().join("elsewhere.jsonl");
    fs::write(&elsewhere, log([agents(7)])).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.path().join(PREVIOUS)).unwrap();
    assert_eq!(times(&recent(dir.path(), 10)), ["2", "1", "0"]);
}

#[test]
fn a_row_reads_back_as_the_bytes_it_was_written_as() {
    let line = r#"{"actionCount":2,"actor":"agent","app":"TextEdit","at":"5","code":"focus-changed","command":"c","elapsedMs":1.5,"ok":false,"operation":"input","partial":true,"reason":"window","session":"0123456789ab","started":true,"task":"ba9876543210","verb":"type"}"#;
    let row = Row::parse(&serde_json::from_str(line).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&row).unwrap().to_string(), line);
}

#[test]
fn a_rows_bundle_id_reads_back_as_written_and_a_row_without_one_still_reads() {
    let line = r#"{"actionCount":1,"actor":"agent","app":"TextEdit","at":"5","bundleId":"com.apple.TextEdit","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
    let row = Row::parse(&serde_json::from_str(line).unwrap()).unwrap();
    assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(serde_json::to_value(&row).unwrap().to_string(), line);

    let older = r#"{"actionCount":1,"actor":"agent","app":"TextEdit","at":"5","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
    let row = Row::parse(&serde_json::from_str(older).unwrap()).unwrap();
    assert_eq!(row.bundle_id, None);
    assert_eq!(
        serde_json::to_value(&row).unwrap().to_string(),
        older,
        "a row with none is written as before"
    );
    let odd = Row::parse(&json!({"operation": "input", "bundleId": 7})).unwrap();
    assert_eq!(odd.bundle_id, None);
}

#[test]
fn a_rows_picture_before_reads_back_as_written_and_a_row_without_one_still_reads() {
    let line = r#"{"actionCount":1,"actor":"agent","at":"5","before":"5-before.jpg","command":"c","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
    let row = Row::parse(&serde_json::from_str(line).unwrap()).unwrap();
    assert_eq!(row.before.as_deref(), Some("5-before.jpg"));
    assert_eq!(serde_json::to_value(&row).unwrap().to_string(), line);

    let older = r#"{"actionCount":1,"actor":"agent","at":"5","command":"c","elapsedMs":2.0,"ok":true,"operation":"input","verb":"click"}"#;
    let row = Row::parse(&serde_json::from_str(older).unwrap()).unwrap();
    assert_eq!(row.before, None);
    assert_eq!(
        serde_json::to_value(&row).unwrap().to_string(),
        older,
        "a row with none is written as before"
    );
    let odd = Row::parse(&json!({"operation": "input", "before": ["5-before.jpg"]})).unwrap();
    assert_eq!(odd.before, None);
}

#[test]
fn a_field_an_older_build_wrote_otherwise_reads_as_missing_and_an_unknown_op_is_kept() {
    let row = Row::parse(
        &json!({"operation": "later", "ok": "yes", "actionCount": -1,
        "verb": 3, "app": "Notes", "more": {"x": 1}}),
    )
    .unwrap();
    assert_eq!(row.operation, Some(Op::Other("later".into())));
    assert_eq!((row.ok, row.action_count, &row.verb), (None, None, &None));
    assert_eq!(row.app.as_deref(), Some("Notes"));
    assert_eq!(
        serde_json::to_value(&row).unwrap()["operation"],
        "later",
        "kept as it was written"
    );
    assert!(Row::parse(&json!({"operation": 7})).is_none());
    assert!(Row::parse(&json!(["input"])).is_none());
}

#[test]
fn a_rows_age_and_time_read_from_its_nanoseconds() {
    let row = Row::parse(&json!({"operation": "run", "at": "5000000000"})).unwrap();
    assert_eq!(row.age(7_000_000_000), Some(2_000_000_000));
    assert_eq!(row.age(1), Some(0));
    assert_eq!(row.at_ms(), Some(5000));
    let undated = Row::parse(&json!({"operation": "run", "at": "soon"})).unwrap();
    assert_eq!((undated.age(1), undated.at_ms()), (None, None));
}

#[test]
fn a_row_keeps_the_account_it_acted_in_and_what_let_it_and_reads_without_them() {
    let account = json!({"name": "github.com",
        "key": {"kind": "site", "id": "github.com", "browser": "com.google.Chrome"}});
    for allowed in ["once", "task", "trusted"] {
        let written = json!({"at": "1", "operation": "input", "verb": "click",
            "account": account, "allowed": allowed});
        let row = Row::parse(&written).unwrap();
        let read = serde_json::to_value(&row).unwrap();
        assert_eq!(read["account"], account);
        assert_eq!(read["allowed"], allowed);
        let again = Row::parse(&read).unwrap();
        assert_eq!(
            (again.account, again.allowed),
            (row.account, row.allowed),
            "{allowed}"
        );
    }
    let app = json!({"name": "Slack", "key": {"kind": "app", "id": "com.tinyspeck.slackmacgap"}});
    let row = Row::parse(&json!({"at": "1", "operation": "input", "account": app})).unwrap();
    assert_eq!(row.account.as_ref().unwrap().key.browser, None);
    assert_eq!(row.account.as_ref().unwrap().site(), None);
    assert_eq!(row.allowed, None);
    assert_eq!(serde_json::to_value(&row).unwrap()["account"], app);

    // A row an older build wrote has neither, and writes neither back.
    let old = Row::parse(&json!({"at": "1", "operation": "input", "verb": "click"})).unwrap();
    assert_eq!((old.account.clone(), old.allowed), (None, None));
    let written = serde_json::to_value(&old).unwrap();
    assert!(written.get("account").is_none() && written.get("allowed").is_none());
}

#[test]
fn an_account_or_allowance_that_doesnt_read_is_dropped_and_its_row_kept() {
    for (account, allowed) in [
        (json!(null), json!(null)),
        (json!("github.com"), json!("sometimes")),
        (json!({"name": "github.com"}), json!(3)),
        (
            json!({"name": "github.com", "key": {"kind": "page", "id": "github.com"}}),
            json!(true),
        ),
        (
            json!({"name": 5, "key": {"kind": "site", "id": "github.com"}}),
            json!({"once": true}),
        ),
        (
            json!({"name": "github.com", "key": {"kind": "site", "id": 7}}),
            json!(["once"]),
        ),
        (
            json!({"name": "github.com", "key": {"kind": "site", "id": "github.com", "browser": 1}}),
            json!("Once"),
        ),
    ] {
        let row = Row::parse(&json!({"at": "1", "operation": "input", "verb": "click",
            "account": account, "allowed": allowed}))
        .expect("the row is kept");
        assert_eq!(row.verb.as_deref(), Some("click"));
        assert_eq!(row.account, None, "{account}");
        assert_eq!(row.allowed, None, "{allowed}");
    }
}
