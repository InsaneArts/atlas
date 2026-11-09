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
    if !atlas_core::test_support::in_timezone(
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

/// An agent's click in a command, as its row is made from `reply`.
fn clicked(reply: Value) -> Row {
    let directory = tempfile::tempdir().unwrap();
    let mut client = crate::test_support::client_in(directory.path());
    client.doing = Some(crate::doing::Doing::new("click"));
    client.row(
        &json!({"op": "input", "actions": [{"type": "click"}]}),
        &reply,
        Duration::ZERO,
        Step::End,
    )
}

#[test]
fn an_agents_input_row_names_the_listed_account_and_what_let_it_act_there() {
    let account = json!({"name": "github.com",
        "key": {"kind": "site", "id": "github.com", "browser": "com.google.Chrome"}});
    for (allowed, kept) in [
        ("once", activity_log::Allowed::Once),
        ("task", activity_log::Allowed::Task),
        ("trusted", activity_log::Allowed::Trusted),
    ] {
        let row = clicked(json!({"ok": true, "app": "Google Chrome",
            "account": account, "allowed": allowed}));
        assert_eq!(row.account.as_ref().unwrap().name, "github.com");
        assert_eq!(row.allowed, Some(kept), "{allowed}");
        assert_eq!(crate::doing::says_to_person(&row), "clicked on github.com");
        assert_eq!(crate::doing::says(&row), "clicked in Google Chrome");
    }
    // In an account that asks, a click that sent nothing needed allowing.
    let row = clicked(json!({"ok": true, "account": account}));
    assert!(row.account.is_some());
    assert_eq!(row.allowed, None);
}

#[test]
fn an_unlisted_app_or_an_allowance_without_its_account_names_none() {
    let row = clicked(json!({"ok": true, "app": "TextEdit"}));
    assert_eq!((row.account, row.allowed), (None, None));
    let row = clicked(json!({"ok": true, "app": "TextEdit", "allowed": "trusted"}));
    assert_eq!((row.account, row.allowed), (None, None));
    let row =
        clicked(json!({"ok": true, "account": {"name": " ", "key": {"kind": "app", "id": "x"}}}));
    assert_eq!(row.account, None, "a blank name is no account");
}
