//! What the viewer reads for the app without changing anything: the
//! status and overview, recent activity, host sessions and the screenshots
//! agents took, and what a cleanup would do.
use super::{
    Viewer,
    control::cleanup_session,
    http::{json_response, response},
    unpoisoned,
};
use atlas_client::{
    activity_log::{self, Whose},
    before,
    client::Client,
    helper_info::{self, Capture},
    host_session::{
        names::{name_of, names},
        reading::{SessionFacts, SessionReading},
        record::Seen,
        store::Store,
    },
    overview::{READ_DEADLINE, late},
};
use atlas_core::host::Key;
use atlas_core::{
    account::{Title, asked_real_name},
    error::{Code, Error, Result},
};
use axum::{body::Body, response::Response};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

/// The most activity rows and screenshots one read lists: what the app's
/// tabs show, a day or so of an agent's work.
pub(super) const ACTIVITY_ROWS: usize = 200;
const SHOTS: usize = 200;

/// A picture the app may read, by the name that says which kind it is.
enum Named {
    Before(before::Before),
    Capture(Capture),
}

impl Viewer {
    /// [`Viewer::status_for`] any caller but the person's own app.
    #[cfg(test)]
    pub(super) async fn status(self: &Arc<Self>) -> Value {
        self.status_for(false).await
    }

    /// The helper's status and overview as the app shows them, with
    /// whether control is held: its recent rows as the person's own app
    /// shows them when the caller is that app (`person`), with the account
    /// each acted in and a browser's action said on its site. Any other
    /// caller, as `atlas watch`, a chat app's view or a script, is shown
    /// them as an agent is.
    pub(super) async fn status_for(self: &Arc<Self>, person: bool) -> Value {
        // Reading waits on the helper, for a few seconds at most, following
        // the settings included.
        let until = tokio::time::Instant::now() + READ_DEADLINE;
        self.follow_settings().await;

        let client = self.client();
        let doctor = atlas_client::overview::report(&client, until).await;

        // Ending control waits on the helper too: it finishes by itself
        // when that takes longer than a moment.
        if !doctor.diagnosis.ready {
            self.release_shortly().await;
        }

        let active = self.lease.lock().await.active();

        let (pending, total) = atlas_client::overview::pending(&client.controller);

        // The status never waits for input to start; shutdown does.
        let _starting = self.start_input_after_login(
            &client,
            &doctor.diagnosis.observation,
            total == 0 && !active,
        );

        let activity_client = client.clone();
        let activity = tokio::task::spawn_blocking(move || {
            activity_client
                .output_dir()
                .map(|directory| {
                    atlas_client::activity_log::recent(directory, atlas_client::overview::ROWS)
                })
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        let now = atlas_core::time::timestamp();
        let overview = atlas_client::overview::overview(&doctor, active, pending, &activity, now);
        let mut status = atlas_client::overview::Reading::new(doctor, overview, active, now);
        // The recent rows as the person's app shows them, with the account
        // each acted in: no other caller's status carries it.
        if person {
            status.value["overview"]["activity"] = json!(
                activity
                    .iter()
                    .map(|row| atlas_client::overview::shown_to_person(row, now))
                    .collect::<Vec<_>>()
            );
        }
        atlas_client::overview::add_host_sessions(&mut status, &client, now).await;

        if let Some(name) = status.value["overview"]["account"].as_str() {
            let title = account_title(&self.account_name, name, asked_real_name).await;
            status.value["overview"]["accountName"] = json!(title);
        }

        // The helper's accounts, for the person's app alone: an agent's
        // status never carries them.
        if person {
            let (accounts, note) = super::accounts::for_status(&self.accounts_home());
            status.value["overview"]["accounts"] = accounts;
            status.value["overview"]["accountNote"] = note;
        }
        let (asks, allowed) = super::accounts::asks_for_status(&client);
        status.value["overview"]["asks"] = asks;
        status.value["overview"]["allowed"] = allowed;
        // While the person signs in, so the app keeps the lease through its
        // window losing focus or being covered.
        status.value["overview"]["signingIn"] = client
            .controller
            .private_hold()
            .map_or(Value::Null, |hold| json!(hold));

        status.value
    }

    /// What was done in the helper lately, newest first, as the app lists
    /// it: all recent sessions or the session selected by `query`. For the
    /// person's app (`person`), each row includes its account attribution.
    /// Reading it records nothing.
    pub(super) async fn activity_for(&self, query: Option<&str>, person: bool) -> Result<Value> {
        let whose = activity_session(query)?;
        let client = self.client();
        // A folder that can't be read is said, never shown as no activity.
        let directory = client.output_dir()?.to_path_buf();
        let unreadable = || Error::new(Code::Io, "The activity log could not be read.");
        let (rows, cleared, pictures, records) = tokio::task::spawn_blocking(move || {
            let records = Store::at(&directory).recent();
            let (mut rows, cleared) = match &whose {
                None => (
                    atlas_client::activity_log::try_recent(&directory, ACTIVITY_ROWS)?,
                    None,
                ),
                Some(whose) => {
                    let found = activity_log::session_rows(&directory, whose, ACTIVITY_ROWS)?;
                    let record = match whose {
                        Whose::Host(key) => records
                            .iter()
                            .map(|seen| &seen.record)
                            .find(|record| record.key == *key),
                        Whose::Terminal => None,
                    };
                    let cleared = record.is_some_and(|record| found.cleared(record));
                    (found.rows, Some(cleared))
                }
            };
            let kept = atlas_client::before::Listing::of(&directory);
            // A picture pruned or cleaned up since is no longer named. Each
            // kept one comes with the picture after it and its facts.
            let pictures = rows
                .iter_mut()
                .map(|row| {
                    row.before = row.before.take().filter(|name| kept.has(name));
                    row.before.as_deref().map(|before| {
                        let after = row.at_ms().and_then(|ended| kept.after(before, ended));
                        (after, atlas_client::before::facts(&directory, before))
                    })
                })
                .collect::<Vec<_>>();
            Ok::<_, std::io::Error>((rows, cleared, pictures, records))
        })
        .await
        .map_err(|error| unreadable().with_source(error))?
        .map_err(|error| unreadable().with_source(error))?;

        let now = atlas_core::time::timestamp();
        let rows = rows
            .iter()
            .zip(pictures)
            .map(|(row, picture)| {
                let mut shown = activity_row(row, now, person);
                let (after, facts) = picture.unwrap_or_default();
                let (after, after_at) = after.unzip();
                shown["after"] = json!(after);
                // When the picture after was taken, in milliseconds since 1970.
                shown["afterAt"] = json!(after_at);
                if let Some(facts) = facts {
                    shown["facts"] = facts;
                }
                shown
            })
            .collect::<Vec<_>>();
        let sessions =
            sessions_named(&client, &records, rows.iter().map(|row| &row["session"])).await;
        let mut reply = json!({"ok": true, "activity": rows, "sessions": sessions});
        if let Some(cleared) = cleared {
            reply["cleared"] = json!(cleared);
        }
        Ok(reply)
    }

    /// The host sessions of the last days, live and ended, newest first:
    /// who each is and what it made, for the app's session menu and
    /// details; and every live session as the status would list it, with
    /// no limit. Reading it records nothing and takes no control.
    pub(super) async fn sessions(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no sessions.
        client.output_dir()?;
        let now = atlas_core::time::timestamp();
        let facts = SessionFacts::local(&client.controller, now);
        let reading_client = client.clone();
        let reading = tokio::task::spawn_blocking(move || {
            SessionReading::collect(reading_client.output_dir().ok(), &reading_client.controller)
        })
        .await
        .ok()
        .flatten();
        let mut live = reading
            .as_ref()
            .map_or_else(Vec::new, |reading| reading.entries(facts));
        for entry in live.iter_mut().filter_map(Value::as_object_mut) {
            entry.remove("hostSession");
        }

        let history = tokio::task::spawn_blocking(move || {
            let Ok(output) = client.output_dir() else {
                return Vec::new();
            };
            let store = Store::at(output);
            // The records status reads share: only those with copies left
            // are looked at again, for which app each copy is.
            let seen = reading
                .as_ref()
                .map_or_else(|| store.recent(), |reading| Arc::clone(&reading.seen));
            let records: Vec<_> = seen.iter().map(|seen| seen.record.clone()).collect();
            let rows = atlas_client::activity_log::recent(output, ACTIVITY_ROWS);
            let captures = helper_info::captures(output, SHOTS)
                .into_iter()
                .map(|(_, name)| name)
                .collect();
            let copies = seen
                .iter()
                .filter(|seen| seen.copies_left > 0)
                .map(|seen| {
                    let apps = store
                        .leftovers(&seen.record, &[])
                        .copies
                        .iter()
                        .map(|token| atlas_client::handoff::copy_app(store.copies(), token))
                        .collect();
                    (seen.record.key.clone(), apps)
                })
                .collect();
            atlas_client::session_view::HistorySource {
                records: &records,
                rows: &rows,
                captures: &captures,
                copies: &copies,
                tasks: &atlas_client::task::store::Store::at(output).recent(),
                now: u64::try_from(now / 1_000_000).unwrap_or(u64::MAX),
            }
            .history_with(
                &reading
                    .as_ref()
                    .map(|reading| reading.processes.clone())
                    .unwrap_or_default(),
                |key| {
                    reading
                        .as_ref()
                        .is_some_and(|reading| reading.work(key, facts))
                },
            )
        })
        .await
        .unwrap_or_default();

        Ok(json!({"ok": true, "sessions": history, "live": live}))
    }

    /// The screenshots agents took, newest first: each one's name, when it
    /// was taken (milliseconds since 1970) and its size when its header
    /// reads.
    pub(super) async fn screenshots(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no screenshots.
        client.output_dir()?;
        let listing_client = client.clone();
        let (shots, records) = tokio::task::spawn_blocking(move || {
            let Ok(output) = listing_client.output_dir() else {
                return (Vec::new(), Arc::from([]));
            };
            let records = Store::at(output).recent();
            let taken_by = capture_sessions(&records);

            let shots = helper_info::captures(output, SHOTS)
                .into_iter()
                .map(|(at, name)| {
                    let size = helper_info::capture_size(output, &name);
                    let taken = u64::try_from(at / 1_000_000).unwrap_or(u64::MAX);
                    let mut shot = json!({"name": name, "takenAt": taken});
                    if let Some((width, height)) = size {
                        shot["width"] = json!(width);
                        shot["height"] = json!(height);
                    }
                    if let Some(session) = taken_by.get(name.as_str()) {
                        shot["session"] = json!(session);
                    }
                    shot
                })
                .collect::<Vec<_>>();
            (shots, records)
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), Arc::from([])));

        let sessions =
            sessions_named(&client, &records, shots.iter().map(|shot| &shot["session"])).await;
        // The folder they're in, so the app can show one in the Finder.
        let folder = client.output_dir().ok().map(Path::to_path_buf);
        Ok(json!({"ok": true, "screenshots": shots, "folder": folder, "sessions": sessions}))
    }

    /// One screenshot an agent took, or a picture taken before an agent's
    /// action, as `query` names it: its thumbnail as a JPEG, or with
    /// `size=full` the picture as kept. Any other name, a link or anything
    /// but a plain file is not found.
    pub(super) async fn screenshot(&self, query: Option<&str>) -> Response {
        let refused = || {
            json_response(
                404,
                json!(Error::new(Code::BadRequest, "No screenshot has that name")),
            )
        };

        let (mut name, mut size) = (None, None);
        for pair in query.unwrap_or_default().split('&') {
            match pair.split_once('=') {
                Some(("name", given)) => name = Some(given),
                Some(("size", given)) => size = Some(given),
                _ => {}
            }
        }

        let full = match size {
            None | Some("thumb") => false,
            Some("full") => true,
            Some(_) => return refused(),
        };
        let named = match name.map(|name| (before::Before::parse(name), Capture::parse(name))) {
            Some((Some(picture), _)) => Named::Before(picture),
            Some((None, Some(capture))) => Named::Capture(capture),
            Some((None, None)) | None => return refused(),
        };

        let client = self.client();
        let read = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, &'static str)> {
            let output = client.output_dir()?;
            let (kept, mime) = match &named {
                Named::Before(picture) => (before::read(output, picture)?, "image/jpeg"),
                Named::Capture(capture) => (
                    helper_info::read_capture(output, capture.as_str())?,
                    "image/png",
                ),
            };
            if full {
                return Ok((kept, mime));
            }
            let (thumbnail, _, _) = helper_info::jpeg(&kept, helper_info::THUMB)?;
            Ok((thumbnail, "image/jpeg"))
        })
        .await
        .unwrap_or_else(|error| {
            Err(Error::new(Code::Capture, "The screenshot could not be read").with_source(error))
        });

        match read {
            Ok((bytes, mime)) => response(200, mime, Body::from(bytes), &[]),
            Err(error) if error.code == Code::BadRequest => json_response(404, json!(error)),
            Err(error) => json_response(500, json!(error)),
        }
    }

    /// A helper just logged in has no input service yet, and switching
    /// accounts can leave one pinned to a display that is gone or renumbered
    /// (which taking control refuses). Input starts or re-pins once per
    /// helper daemon and display, as Recover would, so agents and viewers
    /// carry on by themselves: only in the background, with Accessibility on
    /// and nothing pending or held. Like any change in the helper it runs
    /// whole before shutdown ends, and not at all once shutdown began; the
    /// caller may wait on it or not.
    fn start_input_after_login(
        self: &Arc<Self>,
        client: &Arc<Client>,
        session: &atlas_core::session::Observation,
        settled: bool,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let native = &session.native;
        let starts = settled
            && native.control_active != Some(true)
            && native.held_input != Some(true)
            && session.accessibility == Some(true);
        let hidden = starts
            .then(|| session.require_hidden(client.config.helper_uid).ok())
            .flatten()
            .filter(|hidden| !hidden.input);
        let (Some(hidden), Some(daemon)) = (hidden, session.capture_pid) else {
            return None;
        };

        let display = &hidden.epoch.display;
        let key = format!(
            "{daemon}:{}:{}x{}",
            hidden.epoch.display_id, display.width, display.height
        );

        let first = self
            .input_started
            .lock()
            .map(|mut started| started.replace(key.clone()) != Some(key))
            .unwrap_or(false);
        if !first {
            return None;
        }
        let (viewer, client) = (self.clone(), client.clone());
        Some(tokio::spawn(async move {
            let Some(_changing) = viewer.changing().await else {
                return;
            };
            // A failed start is read again by the next status; its unknown
            // outcomes stay pending in the controller.
            let _ = client.start_input().await;
        }))
    }

    /// What cleaning up the session `query` names would do, as the person
    /// sees it before they confirm, and whether that session may still be
    /// working. It reads only and takes no control.
    pub(super) async fn cleanup_preview(&self, query: Option<&str>) -> Result<Value> {
        let session = query
            .unwrap_or_default()
            .split('&')
            .find_map(|pair| pair.strip_prefix("session="));
        let session = cleanup_session(session)?;
        let client = self.client();
        tokio::time::timeout(
            READ_DEADLINE,
            atlas_client::cleanup::for_person(
                &client,
                &session,
                atlas_client::cleanup::Mode::Preview,
            ),
        )
        .await
        .map_err(|_| late())?
    }
}

/// Whose activity `query` asks for: every session's without `session=`,
/// Terminal's for an empty key, else the session its key names.
///
/// # Errors
///
/// Returns an input error for a key that is not one.
fn activity_session(query: Option<&str>) -> Result<Option<Whose>> {
    let Some(given) = query
        .unwrap_or_default()
        .split('&')
        .find_map(|pair| pair.strip_prefix("session="))
    else {
        return Ok(None);
    };
    if given.is_empty() {
        return Ok(Some(Whose::Terminal));
    }
    cleanup_session(Some(given)).map(|key| Some(Whose::Host(key)))
}

/// Account `name`'s full name, as `ask` finds it, kept in `known`. It is
/// asked once per account (off the runtime, as it asks directory services),
/// also when it has none, but again after an ask that had no answer.
async fn account_title(
    known: &std::sync::Mutex<Option<(String, Title)>>,
    name: &str,
    ask: fn(&str) -> Title,
) -> Option<String> {
    let kept = unpoisoned(known)
        .clone()
        .filter(|(account, _)| account == name);
    if let Some((_, title)) = kept {
        return title.titled();
    }

    let account = name.to_owned();
    let title = tokio::task::spawn_blocking(move || ask(&account))
        .await
        .unwrap_or(Title::Unknown);
    if title == Title::Unknown {
        return None;
    }

    *unpoisoned(known) = Some((name.to_owned(), title.clone()));
    title.titled()
}

/// A logged row as the app's activity lists it: what was done in words, by
/// whom, how long ago, whether it worked and how many actions it sent, and
/// the app, control and keys its words name, and the picture taken before
/// it. Only these fields go, through [`atlas_client::overview::shown`]:
/// never typed text, argv, coordinates or titles, which the log never keeps
/// either.
///
/// For the person's own app (`person`), it names the listed account it
/// acted in and what let it, and a browser's action is said on its site;
/// for anyone else, as for agents, neither.
pub(super) fn activity_row(
    row: &atlas_client::activity_log::Row,
    now: u128,
    person: bool,
) -> Value {
    let shown = if person {
        atlas_client::overview::shown_to_person(row, now)
    } else {
        atlas_client::overview::shown(row, now)
    };
    let keys = shown.keys();
    let mut fields = json!(shown);
    rename(&mut fields, "actionCount", "count");
    fields["logicalId"] = json!(row.logical_id());
    // When it started, in milliseconds since 1970, so its time never moves
    // between readings.
    fields["startedAt"] = json!(row.at_ms());
    fields["keys"] = json!(keys);
    // Only the person's app is given it; no agent's reply names it.
    fields["before"] = json!(row.before);
    fields
}

/// `value` with its member `from` called `to`.
fn rename(value: &mut Value, from: &str, to: &str) {
    if let Some(fields) = value.as_object_mut()
        && let Some(member) = fields.remove(from)
    {
        fields.insert(to.to_owned(), member);
    }
}

/// Each host session `keys` name that a record still names, as the app
/// shows it: its name, host and times. A key whose record is gone is left
/// out, and its rows show no session.
async fn sessions_named<'a>(
    client: &Client,
    seen: &Arc<[Seen]>,
    keys: impl Iterator<Item = &'a Value>,
) -> Value {
    let seen = Arc::clone(seen);
    let wanted: BTreeSet<String> = keys.filter_map(Value::as_str).map(str::to_owned).collect();
    let client = client.clone();
    tokio::task::spawn_blocking(move || {
        let records = || seen.iter().map(|seen| &seen.record);
        let named = names(records());
        let reading = SessionReading::collect(client.output_dir().ok(), &client.controller);
        let processes = reading.as_ref().map(|reading| &reading.processes);
        let now = atlas_core::time::now_ms();
        let facts = SessionFacts::local(&client.controller, u128::from(now) * 1_000_000);
        let sessions: Map<String, Value> = records()
            .filter(|record| wanted.contains(record.key.as_str()))
            .map(|record| {
                let work = reading
                    .as_ref()
                    .is_some_and(|reading| reading.work(&record.key, facts));
                let ended = record.ended.or_else(|| {
                    processes.and_then(|processes| processes.lifetime(record, now, work).ended)
                });
                let session = json!({
                    "name": name_of(&named, &record.key, Some(record.host)),
                    "host": record.host,
                    "hostLabel": record.host.label(),
                    "started": record.started,
                    "ended": ended,
                });
                (record.key.to_string(), session)
            })
            .collect();
        Value::Object(sessions)
    })
    .await
    .unwrap_or_else(|_| json!({}))
}

/// Which host session took each screenshot the records list: the first
/// record to list it, newest first.
fn capture_sessions(seen: &[Seen]) -> BTreeMap<&str, &Key> {
    let mut taken_by = BTreeMap::new();
    for record in seen.iter().map(|seen| &seen.record) {
        for name in &record.screenshots {
            taken_by.entry(name.as_str()).or_insert(&record.key);
        }
    }
    taken_by
}

#[cfg(test)]
mod tests;
