use super::*;

#[test]
fn an_app_is_added_only_by_its_id_and_name() {
    for body in [
        json!({}),
        json!({"id": "com.tinyspeck.slackmacgap"}),
        json!({"id": " ", "name": "Slack"}),
        json!({"id": "x".repeat(201), "name": "Slack"}),
    ] {
        assert_eq!(add(&None, &body).unwrap_err().code, Code::Input, "{body}");
    }
}

#[test]
fn an_account_is_removed_only_by_its_kind_and_id() {
    for body in [
        json!({"id": "github.com"}),
        json!({"kind": "page", "id": "github.com"}),
        json!({"kind": "site"}),
    ] {
        assert_eq!(
            remove(&None, &body).unwrap_err().code,
            Code::Input,
            "{body}"
        );
    }
}

#[test]
fn a_level_is_set_only_by_its_name() {
    for body in [
        json!({"kind": "site", "id": "github.com"}),
        json!({"kind": "site", "id": "github.com", "level": "careful"}),
        json!({"id": "github.com", "level": "trusted"}),
    ] {
        assert_eq!(
            set_level(&None, &body).unwrap_err().code,
            Code::Input,
            "{body}"
        );
    }
}

#[test]
fn the_status_shows_a_question_but_never_what_would_be_sent() {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(dir.path());
    let canary = "the release notes canary 41c7";
    let ask = atlas_control::asks::Ask {
        id: String::new(),
        session: "s1".to_owned(),
        process: None,
        who: "Claude Code · atlas".to_owned(),
        task: None,
        account: "slack.com".to_owned(),
        question: json!({"what": "send-key", "batch": "b1", "message": {"preview": canary, "digest": "d"}}),
        asked: 0,
        ends: 0,
    };
    client
        .controller
        .asks()
        .ask(ask, atlas_core::time::now_ms())
        .unwrap();

    let (asks, allowed) = asks_for_status(&client);
    assert_eq!(asks[0]["account"], "slack.com");
    assert_eq!(asks[0]["what"], "send-key");
    assert!(!asks.to_string().contains(canary), "{asks}");
    assert_eq!(allowed, json!([]));
    assert!(
        super::asks(&client).to_string().contains(canary),
        "the person's own list shows the message"
    );
}

#[test]
fn an_answer_names_its_question_and_one_of_three_choices() {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(dir.path());
    for body in [
        json!({"choice": "once"}),
        json!({"id": "a", "choice": "maybe"}),
    ] {
        assert_eq!(
            answer(&client, &body).unwrap_err().code,
            Code::Input,
            "{body}"
        );
    }
    assert_eq!(
        answer(&client, &json!({"id": "gone", "choice": "deny"}))
            .unwrap_err()
            .code,
        Code::Input
    );
    assert_eq!(
        end_allowed(&client, &json!({})).unwrap_err().code,
        Code::Input
    );
}

#[test]
fn what_the_person_allowed_a_task_ends_with_its_host_process() {
    let dir = tempfile::tempdir().unwrap();
    let client = atlas_client::test_support::client(dir.path());
    let asks = client.controller.asks();
    let now = atlas_core::time::now_ms();
    let key = json!({"kind": "site", "id": "slack.com"});
    let live = atlas_core::process::Instance::current().unwrap();
    let gone = atlas_core::process::Instance::new(live.pid, [1, 0]);
    for (session, process) in [("live1", live), ("gone1", gone)] {
        let ask = atlas_control::asks::Ask {
            id: String::new(),
            session: session.to_owned(),
            process: Some(process),
            who: "Claude Code".to_owned(),
            task: None,
            account: "slack.com".to_owned(),
            question: json!({"accountKey": key, "bind": {"session": session}}),
            asked: 0,
            ends: 0,
        };
        let id = asks.ask(ask, now).unwrap();
        asks.answer(&id, atlas_control::asks::Answer::Task, true, now)
            .unwrap();
    }
    let (_, allowed) = asks_for_status(&client);
    let left: Vec<&str> = allowed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|grant| grant["session"].as_str())
        .collect();
    assert_eq!(left, ["live1"]);
}
