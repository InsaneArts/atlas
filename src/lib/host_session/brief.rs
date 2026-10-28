//! What a session start says of a host session's own work in the helper:
//! one sentence when it, or the session before its `/clear`, left copies,
//! plans or unknown actions there, and nothing otherwise. It reads the
//! records, copy folders and marker only.
use super::{
    HostSession,
    record::{Leftovers, Record},
    store::Store,
};
use crate::{
    client::Client,
    task::{Owned, State},
    ui::text,
};

/// The sentence a session start adds when `session`, or the session before
/// its `/clear` in the same host process, left something in the helper.
pub(crate) fn leftovers(client: &Client, session: &HostSession) -> Option<String> {
    let store = Store::at(client.output_dir().ok()?);
    let causes = client.controller.pending_causes().unwrap_or_default();
    let records = store.all();
    let own = records.iter().find(|record| record.key == *session.key());

    let tasks = crate::task::store::Store::at(client.output_dir().ok()?).of_session(session.key());
    let unfinished = tasks.iter().any(|task| task.unfinished_cleanup().is_some());
    let installed = tasks
        .iter()
        .flat_map(|task| task.claims())
        .filter(|(owned, state)| {
            matches!(owned, Owned::Install { .. }) && matches!(state, State::Intent | State::Made)
        })
        .count();
    let cleanup = if unfinished {
        " Its cleanup is unfinished: atlas cleanup shows what is left."
    } else {
        ""
    };

    if let Some(own) = own {
        let left = store.leftovers(own, &causes);
        if !left.is_empty() || installed > 0 {
            return Some(format!(
                "This session left {} in the helper; atlas activity lists {}.{cleanup}",
                said(&left, installed),
                them(&left, installed)
            ));
        }
        if unfinished {
            return Some(
                "This session's cleanup is unfinished: atlas cleanup shows what is left.".into(),
            );
        }
    }

    let before = before_clear(&records, own, session)?;
    let left = store.leftovers(before, &causes);
    (!left.is_empty()).then(|| {
        format!(
            "The session before /clear left {} in the helper; atlas activity --session {} lists {}.",
            said(&left, 0),
            before.key,
            them(&left, 0)
        )
    })
}

/// The session `session` followed in its host process: the one its record
/// names, else the newest other record of the same process.
fn before_clear<'a>(
    records: &'a [Record],
    own: Option<&Record>,
    session: &HostSession,
) -> Option<&'a Record> {
    if let Some(previous) = own.and_then(|own| own.previous.as_ref()) {
        return records.iter().find(|record| record.key == *previous);
    }
    let process = session.process()?;
    records
        .iter()
        .find(|record| record.key != *session.key() && record.host_process == Some(process))
}

/// What was left, `installed` apps among it, in words: "1 open copy and 1
/// applied environment plan".
fn said(left: &Leftovers, installed: usize) -> String {
    let parts: Vec<String> = [
        (left.copies.len(), "open copy", "open copies"),
        (
            left.plans,
            "applied environment plan",
            "applied environment plans",
        ),
        (installed, "installed app", "installed apps"),
        (left.uncertain, "unknown action", "unknown actions"),
    ]
    .into_iter()
    .filter(|(count, ..)| *count > 0)
    .map(|(count, one, many)| match count {
        1 => format!("1 {one}"),
        _ => format!("{count} {many}"),
    })
    .collect();

    text::listed(&parts)
}

/// "it" for one thing left, "them" for more.
fn them(left: &Leftovers, installed: usize) -> &'static str {
    if left.copies.len() + left.plans + installed + left.uncertain == 1 {
        "it"
    } else {
        "them"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_session::{
        Host, Process,
        store::{Kind, note},
    };
    use serde_json::json;

    fn session(id: &str) -> HostSession {
        HostSession::new(Host::Claude, id)
            .unwrap()
            .run_by(Process::current())
    }

    /// `session` applied an environment plan, and so left it in the helper.
    fn applied(client: &Client, session: &HostSession) {
        let reply = json!({"ok": true, "planId": crate::control::token(), "phase": "applied"});
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
        let client = crate::transaction_test::client(directory.path());
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
        let client = crate::transaction_test::client(directory.path());
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
        let client = crate::transaction_test::client(directory.path());
        let own = session("cleaning");
        clicked(&client, &own);
        let task = Task::of(own.key(), None).unwrap();
        let tasks = Tasks::at(client.output_dir().unwrap());
        let receipt = crate::control::Token::new();
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
            copies: vec![crate::control::Token::new(); copies],
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
}
