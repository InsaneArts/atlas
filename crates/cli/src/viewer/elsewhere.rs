//! What the service shows of each desktop it serves but does not view: the
//! sessions working there, listed beside the viewed desktop's own, and a
//! session's Activity there. Both come from the records Atlas keeps of that
//! desktop in this account, as the feed's own look at it reads them: never
//! from the desktop itself, so nothing starts a VM or asks one anything,
//! whether it runs, is stopped or a sign-in keeps it private, and never
//! with a picture of it, which stays with its own Screen. One listing of
//! the other desktops answers every read for a moment, and is forgotten as
//! soon as the feed tells of a change.
use super::{
    Viewer,
    desks::Desk,
    reads::{ACTIVITY_ROWS, activity_of, activity_session},
};
use atlas_client::{
    client::Client,
    host_session::reading::{SessionFacts, SessionReading},
};
use atlas_core::error::Result;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

/// How long one listing of a desktop's sessions answers every read.
const KEPT_FOR: Duration = Duration::from_secs(1);

/// What an Activity row says of the pictures taken around it.
const PICTURES: [&str; 5] = ["before", "after", "afterAt", "facts", "capture"];

/// The newest listing of each other desktop's sessions, and how many times
/// listings were forgotten since the service started.
#[derive(Default)]
pub(super) struct Elsewhere {
    kept: tokio::sync::Mutex<BTreeMap<String, Kept>>,
    forgotten: AtomicU64,
}

/// One desktop's live sessions, as listed from its records.
struct Kept {
    /// The client they were read through: a desktop the settings changed
    /// since is read again.
    client: Arc<Client>,
    at: Instant,
    /// How many times listings were forgotten before this one began.
    forgotten: u64,
    sessions: Vec<Value>,
}

impl Kept {
    /// Whether it still answers a read of `desk`.
    fn answers(&self, desk: &Desk, forgotten: u64) -> bool {
        Arc::ptr_eq(&self.client, &desk.client)
            && self.forgotten == forgotten
            && self.at.elapsed() < KEPT_FOR
    }
}

impl Viewer {
    /// `reply`, the viewed desktop's `sessions.json`, with the desktop each
    /// live session acts on named, every other desktop's live sessions
    /// listed after them, and which desktop is viewed.
    pub(super) async fn list_elsewhere(&self, reply: &mut Value, viewed: &Client) {
        let viewed = viewed.desktop().id();
        let (here, others): (Vec<Desk>, Vec<Desk>) = self
            .desks()
            .into_iter()
            .partition(|desk| desk.client.desktop().id() == viewed);
        if let (Some(desk), Some(live)) = (here.first(), reply["live"].as_array_mut()) {
            let named = desk.named();
            for entry in live.iter_mut() {
                entry["desktop"] = named.clone();
            }
        }
        let elsewhere = self.sessions_elsewhere(others).await;
        if let Some(live) = reply["live"].as_array_mut() {
            live.extend(elsewhere);
        }
        reply["viewed"] = json!(viewed.as_str());
    }

    /// The live sessions of each of `others`, in their order, each naming
    /// its desktop: a listing kept from a moment ago answers, and the reads
    /// that come while one is made wait for it.
    async fn sessions_elsewhere(&self, others: Vec<Desk>) -> Vec<Value> {
        if others.is_empty() {
            return Vec::new();
        }
        let mut kept = self.elsewhere.kept.lock().await;
        let forgotten = self.elsewhere.forgotten.load(Ordering::SeqCst);
        kept.retain(|id, _| others.iter().any(|desk| id_of(desk) == *id));
        let stale: Vec<Desk> = others
            .iter()
            .filter(|desk| {
                !kept
                    .get(&id_of(desk))
                    .is_some_and(|kept| kept.answers(desk, forgotten))
            })
            .cloned()
            .collect();
        if !stale.is_empty() {
            let read = tokio::task::spawn_blocking(move || {
                stale
                    .into_iter()
                    .map(|desk| {
                        let sessions = live_on(&desk);
                        (desk, sessions)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            let at = Instant::now();
            for (desk, sessions) in read {
                let kept_now = Kept {
                    client: desk.client.clone(),
                    at,
                    forgotten,
                    sessions,
                };
                kept.insert(id_of(&desk), kept_now);
            }
        }
        others
            .iter()
            .filter_map(|desk| kept.get(&id_of(desk)))
            .flat_map(|kept| kept.sessions.iter().cloned())
            .collect()
    }

    /// Forget every listing of the other desktops' sessions: the next read
    /// lists them again. It never waits for a listing.
    pub(super) fn forget_elsewhere(&self) {
        self.elsewhere.forgotten.fetch_add(1, Ordering::SeqCst);
    }

    /// What was done on `desk`, a desktop served but not viewed, as the
    /// viewed desktop's `activity.json` lists it for `query`, its rows
    /// naming no picture.
    ///
    /// # Errors
    ///
    /// As [`activity_of`], and an input error for a session key that is
    /// not one.
    pub(super) async fn activity_elsewhere(
        &self,
        desk: &Desk,
        query: Option<&str>,
        person: bool,
    ) -> Result<Value> {
        let whose = activity_session(query)?;
        let mut reply = activity_of(&desk.client, whose, ACTIVITY_ROWS, person).await?;
        let rows = reply["activity"].as_array_mut().into_iter().flatten();
        for row in rows.filter_map(Value::as_object_mut) {
            for picture in PICTURES {
                row.remove(picture);
            }
        }
        Ok(reply)
    }
}

/// The desktop `desk` serves, by its ID.
fn id_of(desk: &Desk) -> String {
    desk.client.desktop().id().as_str().to_owned()
}

/// The live sessions on `desk`, as the viewed desktop's `sessions.json`
/// lists its own, each naming the desktop: from its records alone.
fn live_on(desk: &Desk) -> Vec<Value> {
    let now = atlas_core::time::timestamp();
    let facts = SessionFacts::local(&desk.client.controller, now);
    let named = desk.named();
    SessionReading::of(&desk.client)
        .map_or_else(Vec::new, |reading| reading.entries(facts))
        .into_iter()
        .map(|mut entry| {
            if let Some(fields) = entry.as_object_mut() {
                fields.remove("hostSession");
                fields.insert("desktop".to_owned(), named.clone());
            }
            entry
        })
        .collect()
}

#[cfg(test)]
#[path = "elsewhere_tests.rs"]
mod tests;
