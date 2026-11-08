use super::*;
use serde_json::json;

const AGENT: &str = "com.apple.BTServer.cloudpairing";

/// `count` exits of `target`, ten seconds apart, ending at 12:59:50, each
/// after `ms` milliseconds.
fn exits(target: &str, count: usize, ms: u64) -> String {
    (0..count)
        .map(|index| {
            let at = 12 * 3600 + 59 * 60 + 50 - (count - 1 - index) as i64 * 10;
            format!(
                "2026-11-08 {:02}:{:02}:{:02}.302405 ({target} [{}]) <Notice>: exited due to exit(0), ran for {ms}ms\n",
                at / 3600,
                at / 60 % 60,
                at % 60,
                1000 + index,
            )
        })
        .collect()
}

#[test]
fn an_apple_job_quitting_at_once_more_than_thirty_times_an_hour_is_restarting() {
    let log = exits("gui/503/com.apple.example", 31, 64);

    assert_eq!(
        restarting(&log, 503),
        [Restart {
            label: "com.apple.example".into(),
            runs: 31,
        }]
    );
}

#[test]
fn thirty_short_runs_or_runs_of_a_second_or_more_are_not_restarting() {
    assert!(restarting(&exits("gui/503/com.apple.example", 30, 64), 503).is_empty());
    assert!(restarting(&exits("gui/503/com.apple.example", 40, 1000), 503).is_empty());
}

#[test]
fn only_apple_jobs_in_the_helper_s_own_domains_are_restarting() {
    let log = [
        exits("gui/501/com.apple.example", 40, 5),
        exits("user/502/com.apple.contactsd", 40, 5),
        exits("gui/503/com.example.other", 40, 5),
        exits("user/503/com.apple.contactsd", 35, 5),
    ]
    .concat();

    assert_eq!(
        restarting(&log, 503),
        [Restart {
            label: "com.apple.contactsd".into(),
            runs: 35,
        }]
    );
}

#[test]
fn only_the_last_hour_of_the_log_counts() {
    // Six hours of runs every ten seconds, 360 an hour, then nothing but
    // one other line thirteen hours later.
    let old = exits("gui/503/com.apple.example", 2160, 64).replace("2026-11-08", "2026-11-07");
    let later = "2026-11-08 02:00:00.000000 (system) <Notice>: removing child: pid/53068\n";
    let log = format!("{old}{later}");

    assert!(restarting(&log, 503).is_empty());
}

#[test]
fn jobs_come_most_restarts_first() {
    let log = [
        exits("gui/503/com.apple.fewer", 32, 5),
        exits("gui/503/com.apple.more", 50, 5),
    ]
    .concat();

    let found: Vec<_> = restarting(&log, 503)
        .into_iter()
        .map(|candidate| candidate.label)
        .collect();
    assert_eq!(found, ["com.apple.more", "com.apple.fewer"]);
}

#[test]
fn lines_that_are_not_exits_are_ignored() {
    let log = "2026-11-08 12:32:37.460742 (gui/503/com.apple.example) <Notice>: Service only ran for 0 seconds. Pushing respawn out by 10 seconds.\n\
               not a log line at all\n\
               2026-11-08 12:3x:37.460742 (gui/503/com.apple.example [1]) <Notice>: exited due to exit(0), ran for 5ms\n"
        .repeat(40);

    assert!(restarting(&log, 503).is_empty());
}

#[test]
fn dates_count_days_across_months_and_leap_years() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    assert_eq!(
        days_from_civil(2024, 3, 1) - days_from_civil(2024, 2, 28),
        2
    );
    assert_eq!(
        seconds("2026-11-08 00:00:00.000000 x").unwrap()
            - seconds("2026-11-07 23:59:59.999999 x").unwrap(),
        1
    );
}

#[test]
fn the_log_s_end_is_read_from_its_first_whole_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("launchd.log");
    std::fs::write(&path, "first line\nsecond line\nthird line\n").unwrap();

    assert_eq!(tail(&path, 15).unwrap().0, "third line\n");
    assert_eq!(
        tail(&path, 1_000).unwrap().0,
        "first line\nsecond line\nthird line\n"
    );
    assert!(tail(&dir.path().join("missing"), 15).is_none());
}

#[test]
fn the_agent_that_slows_the_mac_restarting_is_said_and_others_alone_are_not() {
    let slowing = restarting(&exits(&format!("gui/503/{AGENT}"), 40, 64), 503);
    assert_eq!(Restarting::of(Some(&slowing)), Restarting::Restarting);

    let other = restarting(&exits("gui/503/com.apple.example", 40, 64), 503);
    assert_eq!(other.len(), 1);
    assert_eq!(Restarting::of(Some(&other)), Restarting::Quiet);
    assert_eq!(Restarting::of(Some(&[])), Restarting::Quiet);
    assert_eq!(Restarting::of(None), Restarting::Unknown);
}

#[test]
fn the_status_names_each_verdict_in_one_word() {
    assert_eq!(
        serde_json::to_value([
            Restarting::Quiet,
            Restarting::Restarting,
            Restarting::Unknown
        ])
        .unwrap(),
        json!(["none", "restarting", "unknown"])
    );
}

/// `lines` written as one log file at `path`.
fn write(path: &std::path::Path, lines: &[&str]) {
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
}

#[test]
fn a_job_whose_restarts_span_three_logs_still_counts_once_each() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = [
        "launchd.log",
        "launchd.log.1",
        "launchd.log.2",
        "launchd.log.3",
    ]
    .iter()
    .map(|name| dir.path().join(name))
    .collect();
    let all = exits("gui/503/com.apple.example", 300, 64);
    let lines: Vec<&str> = all.lines().collect();
    write(&paths[2], &lines[..100]);
    write(&paths[1], &lines[100..200]);
    write(&paths[0], &lines[200..]);

    let log = read_logs(&paths).unwrap();
    assert_eq!(log.lines().count(), 300);
    // 300 runs ten seconds apart span 50 minutes, all inside the hour.
    assert_eq!(restarting(&log, 503)[0].runs, 300);
}

#[test]
fn older_logs_are_read_only_until_the_hour_is_covered() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [
        dir.path().join("launchd.log"),
        dir.path().join("launchd.log.1"),
    ];
    let all = exits("gui/503/com.apple.example", 400, 64);
    let lines: Vec<&str> = all.lines().collect();
    write(&paths[0], &lines);
    write(&paths[1], &["older line"]);

    let log = read_logs(&paths).unwrap();
    assert!(!log.contains("older line"));
    assert_eq!(log.lines().count(), 400);
}

#[test]
fn a_log_moved_aside_between_reads_is_not_counted_twice() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [
        dir.path().join("launchd.log"),
        dir.path().join("launchd.log.1"),
    ];
    std::fs::write(&paths[0], exits("gui/503/com.apple.example", 16, 64)).unwrap();
    // The older one is the same file, as when launchd moves the current log
    // aside after it was read.
    std::fs::hard_link(&paths[0], &paths[1]).unwrap();

    let log = read_logs(&paths).unwrap();
    assert!(restarting(&log, 503).is_empty());
    assert_eq!(log.lines().count(), 16);
}

#[test]
fn a_missing_newest_log_reads_as_nothing_and_a_missing_older_one_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let paths = [
        dir.path().join("launchd.log"),
        dir.path().join("launchd.log.1"),
    ];
    assert!(read_logs(&paths).is_none());

    std::fs::write(&paths[0], exits("gui/503/com.apple.example", 16, 64)).unwrap();
    assert_eq!(read_logs(&paths).unwrap().lines().count(), 16);
}

#[test]
fn only_the_helper_s_own_answer_counts_as_answered() {
    assert!(answered(&json!({"ok": true, "daemon": {}})));
    for reply in [
        json!({"ok": false, "code": "blocked"}),
        json!({"ok": false, "code": "identity"}),
        json!({"ok": false, "code": "busy"}),
        json!({"ok": false, "code": "transport"}),
        json!(null),
    ] {
        assert!(!answered(&reply), "{reply}");
    }
}
