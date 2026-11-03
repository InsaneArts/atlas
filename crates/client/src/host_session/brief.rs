//! What a session start says of a host session's own work in the helper:
//! one sentence when it, or the session before its `/clear`, left copies,
//! plans or unknown actions there, and nothing otherwise. It reads the
//! records, copy folders and marker only.
use super::{
    HostSession,
    record::{Leftovers, Record},
    store::Store,
};
use crate::client::Client;
use atlas_core::name::listed;

/// The sentence a session start adds when `session`, or the session before
/// its `/clear` in the same host process, left something in the helper.
pub fn leftovers(client: &Client, session: &HostSession) -> Option<String> {
    let output = client.output_dir().ok()?;
    let store = Store::at(output);
    let causes = client.controller.pending_causes().unwrap_or_default();
    let records = store.all();
    let own = records.iter().find(|record| record.key == *session.key());

    let tasks = crate::task::store::Store::at(output).of_session(session.key());
    let unfinished = crate::task::record::unfinished_cleanup_of(&tasks, session.key()).is_some();
    let installed = crate::task::record::installs_of(&tasks, session.key()).len();
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

    listed(&parts)
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
mod tests;
