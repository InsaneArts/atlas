use super::*;

#[tokio::test(start_paused = true)]
async fn the_input_timer_keeps_a_newer_command_first_in_recent_activity() {
    let directory = tempfile::tempdir().unwrap();
    let input = PersonInput::default();
    let row = |at: &str, actor: &str| {
        Row::parse(&json!({
            "at": at, "operation": "input", "actor": actor,
            "ok": true, "actionCount": 1
        }))
        .unwrap()
    };
    input
        .gather(directory.path(), row("1000000000", "viewer"))
        .unwrap();
    let timer = input.untimed().unwrap();
    activity_log::append(directory.path(), &row("2000000000", "agent")).unwrap();
    tokio::time::advance(PERSON_INPUT_SPAN).await;
    input.flush_due(timer).unwrap();

    let one = activity_log::recent(directory.path(), 1);
    assert_eq!(one[0].at.as_deref(), Some("2000000000"));
    let both = activity_log::recent(directory.path(), 2);
    assert_eq!(
        both.iter().map(Row::actor).collect::<Vec<_>>(),
        ["agent", "viewer"]
    );
}

#[tokio::test(start_paused = true)]
async fn input_on_either_side_of_local_midnight_stays_separate_when_buffered_and_read() {
    if !crate::test_support::in_timezone(
        "client::record::tests::input_on_either_side_of_local_midnight_stays_separate_when_buffered_and_read",
        "Asia/Tbilisi",
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let input = PersonInput::default();
    // Midnight at UTC+4, while both timestamps belong to the same UTC day.
    let midnight = 72_000_000;
    let times = [midnight - 2, midnight - 1, midnight, midnight + 1];
    for at in times {
        let row = Row::parse(&json!({
            "at": (u128::from(at) * 1_000_000).to_string(),
            "operation": "input", "actor": "viewer", "ok": true,
            "actionCount": 1, "elapsedMs": 2.5
        }))
        .unwrap();
        input.gather(directory.path(), row).unwrap();
    }

    let read = || {
        std::fs::read_to_string(directory.path().join(activity_log::FILE))
            .unwrap()
            .lines()
            .map(|line| Row::parse(&serde_json::from_str(line).unwrap()).unwrap())
            .map(|row| (row.at_ms(), row.action_count, row.elapsed_ms))
            .collect::<Vec<_>>()
    };
    assert_eq!(read(), [(Some(midnight - 1), Some(2), Some(5.0))]);
    input.flush().unwrap();
    assert_eq!(
        read(),
        [
            (Some(midnight - 1), Some(2), Some(5.0)),
            (Some(midnight + 1), Some(2), Some(5.0))
        ]
    );
    let shown = activity_log::recent(directory.path(), 5);
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].at_ms(), Some(midnight + 1));
    assert_eq!(shown[1].at_ms(), Some(midnight - 1));
    assert!(shown.iter().all(|row| row.action_count == Some(2)));
}
