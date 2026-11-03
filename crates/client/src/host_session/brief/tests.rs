use super::*;
use crate::host_session::{
    Host, Instance,
    store::{Kind, note},
};
use serde_json::json;

fn session(id: &str) -> HostSession {
    HostSession::new(Host::Claude, id)
        .unwrap()
        .run_by(Instance::current())
}

/// `session` applied an environment plan, and so left it in the helper.
fn applied(client: &Client, session: &HostSession) {
    let reply = json!({"ok": true, "planId": atlas_core::files::token(), "phase": "applied"});
    note(
        client,
        session,
        Kind::Environment { changes: true },
        None,
        &Ok(reply),
    )
    .unwrap();
}

/// `session` did something that leaves nothing behind.
fn clicked(client: &Client, session: &HostSession) {
    let doing = crate::doing::Doing::new("click");
    note(
        client,
        session,
        Kind::Other,
        Some(&doing),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
}

#[test]
fn the_brief_mentions_leftovers_only_when_there_are_some() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let own = session("own");

    assert_eq!(leftovers(&client, &own), None, "nothing recorded");
    clicked(&client, &own);
    assert_eq!(leftovers(&client, &own), None, "nothing left");
    applied(&client, &own);
    assert_eq!(
        leftovers(&client, &own).as_deref(),
        Some(
            "This session left 1 applied environment plan in the helper; atlas activity lists it."
        )
    );
}

#[test]
fn the_brief_after_clear_points_to_the_session_before_it() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let (before, after) = (session("before"), session("after"));
    applied(&client, &before);
    applied(&client, &before);

    let said = format!(
        "The session before /clear left 2 applied environment plans in the helper; atlas activity --session {} lists them.",
        before.key()
    );
    assert_eq!(
        leftovers(&client, &after).as_deref(),
        Some(said.as_str()),
        "before its first command"
    );
    clicked(&client, &after);
    assert_eq!(
        leftovers(&client, &after).as_deref(),
        Some(said.as_str()),
        "and after"
    );
}

#[test]
fn an_unfinished_cleanup_is_a_leftover_the_brief_points_to() {
    use crate::task::{Task, record::CleanupState, store::Store as Tasks};

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let own = session("cleaning");
    clicked(&client, &own);
    let task = Task::of(own.key(), None).unwrap();
    let tasks = Tasks::at(client.output_dir().unwrap());
    let receipt = atlas_core::files::Token::new();
    tasks
        .update(&task, 1, |record| {
            record.noted_cleanup(&receipt, CleanupState::Incomplete, 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        leftovers(&client, &own).as_deref(),
        Some("This session's cleanup is unfinished: atlas cleanup shows what is left.")
    );

    applied(&client, &own);
    assert_eq!(
        leftovers(&client, &own).as_deref(),
        Some(
            "This session left 1 applied environment plan in the helper; atlas activity \
                 lists it. Its cleanup is unfinished: atlas cleanup shows what is left."
        )
    );

    tasks
        .update(&task, 2, |record| {
            record.noted_cleanup(&receipt, CleanupState::Complete, 2);
            Ok(())
        })
        .unwrap();
    assert!(!leftovers(&client, &own).unwrap().contains("cleanup"));
}

fn left(copies: usize, plans: usize, uncertain: usize) -> Leftovers {
    Leftovers {
        copies: vec![atlas_core::files::Token::new(); copies],
        plans,
        uncertain,
        running: 0,
    }
}

#[test]
fn what_was_left_is_said_in_plain_counts() {
    assert_eq!(said(&left(1, 0, 0), 0), "1 open copy");
    assert_eq!(
        said(&left(2, 1, 0), 0),
        "2 open copies and 1 applied environment plan"
    );
    assert_eq!(
        said(&left(1, 2, 1), 0),
        "1 open copy, 2 applied environment plans and 1 unknown action"
    );
    assert_eq!(them(&left(1, 0, 0), 0), "it");
    assert_eq!(them(&left(1, 1, 0), 0), "them");
    assert_eq!(
        said(&left(0, 1, 0), 1),
        "1 applied environment plan and 1 installed app"
    );
    assert_eq!(said(&left(0, 0, 0), 2), "2 installed apps");
    assert_eq!(them(&left(0, 0, 0), 1), "it");
}
