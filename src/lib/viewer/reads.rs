//! What the viewer reads for the app without changing anything: the
//! status and overview, recent activity, host sessions, the helper's apps
//! and the screenshots agents took, and what a cleanup would do.
use super::{
    Viewer,
    control::cleanup_session,
    http::{json_response, response},
    unpoisoned,
};
use crate::{
    client::Client,
    error::{Code, Error, Result},
    helper_info,
    host_session::{
        Key,
        names::{name_of, names},
        record::Seen,
        store::Store,
    },
    overview::{READ_DEADLINE, late},
    setup::Title,
};
use axum::{body::Body, response::Response};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The most activity rows and screenshots one read lists: what the app's
/// tabs show, a day or so of an agent's work.
pub(super) const ACTIVITY_ROWS: usize = 200;
const SHOTS: usize = 200;

impl Viewer {
    /// The helper's status and overview as the app shows them, with
    /// whether control is held.
    pub(super) async fn status(self: &Arc<Self>) -> Value {
        // Reading waits on the helper, for a few seconds at most, following
        // the settings included.
        let until = tokio::time::Instant::now() + READ_DEADLINE;
        self.follow_settings().await;

        let client = self.client();
        let mut status = crate::overview::report(&client, until).await;

        // Ending control waits on the helper too: it finishes by itself
        // when that takes longer than a moment.
        if status["ready"] != true {
            let ending = self.release_in_background();
            let _ = tokio::time::timeout(Duration::from_secs(1), ending).await;
        }

        let active = self.lease.lock().await.active();
        status["controlActive"] = json!(active);

        let (pending, total) = crate::overview::pending(&client.controller);

        self.start_input_after_login(&client, &status["session"], total == 0 && !active);

        let activity_client = client.clone();
        let activity = tokio::task::spawn_blocking(move || {
            activity_client
                .output_dir()
                .map(|directory| crate::activity_log::recent(directory, crate::overview::ROWS))
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        let now = crate::client::timestamp();
        status["overview"] = crate::overview::overview(&status, active, pending, &activity, now);
        crate::overview::add_host_sessions(&mut status, &client, now).await;

        if let Some(name) = status["overview"]["account"].as_str() {
            status["overview"]["accountName"] = json!(self.account_title(name).await);
        }

        status
    }

    /// The helper account `name`'s full name, as people know it.
    async fn account_title(&self, name: &str) -> Option<String> {
        account_title(&self.account_name, name, crate::setup::asked_real_name).await
    }

    /// What was done in the helper lately, newest first, as the app lists
    /// it. Reading it records nothing.
    pub(super) async fn activity(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no activity.
        client.output_dir()?;
        let reading_client = client.clone();
        let (rows, records) = tokio::task::spawn_blocking(move || {
            reading_client
                .output_dir()
                .map(|directory| {
                    let rows = crate::activity_log::recent(directory, ACTIVITY_ROWS);
                    (rows, Store::at(directory).recent())
                })
                .unwrap_or_else(|_| (Vec::new(), Arc::from([])))
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), Arc::from([])));

        let now = crate::client::timestamp();
        let rows = rows
            .iter()
            .map(|row| activity_row(row, now))
            .collect::<Vec<_>>();
        let sessions =
            sessions_named(&client, &records, rows.iter().map(|row| &row["session"])).await;
        Ok(json!({"ok": true, "activity": rows, "sessions": sessions}))
    }

    /// The host sessions of the last days, live and ended, newest first:
    /// who each is and what it made, for the app's session menu and
    /// details; and every live session as the status would list it, with
    /// no limit. Reading it records nothing and takes no control.
    pub(super) async fn sessions(&self) -> Result<Value> {
        let client = self.client();
        // A folder that can't be read is said, never shown as no sessions.
        client.output_dir()?;
        let now = crate::client::timestamp();
        let facts = crate::overview::SessionFacts::local(&client.controller, now);
        let reading_client = client.clone();
        let reading = tokio::task::spawn_blocking(move || {
            crate::overview::SessionReading::collect(&reading_client)
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
            let seen = reading.as_ref().map_or_else(
                || store.recent(),
                |reading| std::sync::Arc::clone(&reading.seen),
            );
            let records: Vec<_> = seen.iter().map(|seen| seen.record.clone()).collect();
            let rows = crate::activity_log::recent(output, ACTIVITY_ROWS);
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
                        .map(|token| crate::handoff::copy_app(store.copies(), token))
                        .collect();
                    (seen.record.key.clone(), apps)
                })
                .collect();
            crate::session_view::HistorySource {
                records: &records,
                rows: &rows,
                captures: &captures,
                copies: &copies,
                tasks: &crate::task::store::Store::at(output).recent(),
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

    /// The apps open in the helper with their windows, front to back, the
    /// icons of those installed where apps go, and which agent's session
    /// opened each, with those sessions' names. It only reads: nothing is
    /// recorded and no control is taken.
    pub(super) async fn apps(&self) -> Result<Value> {
        let client = self.client();
        let read = async {
            let apps = client.apps().await?;
            let filter = crate::agent::WindowFilter {
                app: None,
                title: None,
                all: false,
            };
            let windows = crate::agent::list(&client, &filter).await?;
            Ok::<_, Error>((listed_apps(&apps["apps"], &windows["windows"])?, apps))
        };
        let (mut apps, running) = tokio::time::timeout(READ_DEADLINE, read)
            .await
            .map_err(|_| late())??;

        let reading_client = client.clone();
        let opened = tokio::task::spawn_blocking(move || {
            let Ok(output) = reading_client.output_dir() else {
                return (HashMap::new(), Arc::from([]));
            };
            let tasks = crate::task::store::Store::at(output).recent();
            let rows = crate::activity_log::recent(output, ACTIVITY_ROWS);
            let copies = Path::new(crate::handoff::ROOT);
            let origins = super::origins::origins(&running["apps"], &tasks, &rows, copies);
            (origins, Store::at(output).recent())
        });
        let (origins, records) = opened
            .await
            .unwrap_or_else(|_| (HashMap::new(), Arc::from([])));
        for app in &mut apps {
            let origin = app["pid"].as_u64().and_then(|pid| origins.get(&pid));
            app["origin"] = json!(origin);
        }
        let opened_by: Vec<Value> = origins
            .values()
            .map(|origin| json!(origin.session))
            .collect();
        let sessions = sessions_named(&client, &records, opened_by.iter()).await;

        for app in apps.iter_mut().take(helper_info::ICON_CACHE) {
            if let Some(bundle) = app["bundle"].as_str().map(PathBuf::from)
                && let Some(icon) = self.icons.get(bundle).await
            {
                app["icon"] = json!(icon);
            }
        }

        Ok(json!({"ok": true, "apps": apps, "sessions": sessions}))
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

    /// One screenshot an agent took, as `query` names it: its thumbnail as
    /// a JPEG, or with `size=full` the PNG as taken. Any other name, a link
    /// or anything but a plain file is not found.
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
        let Some(name) = name
            .filter(|name| helper_info::capture_time(name).is_some())
            .map(str::to_owned)
        else {
            return refused();
        };

        let client = self.client();
        let read = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, &'static str)> {
            let png = helper_info::read_capture(client.output_dir()?, &name)?;
            if full {
                return Ok((png, "image/png"));
            }
            let (thumbnail, _, _) = helper_info::jpeg(&png, helper_info::THUMB)?;
            Ok((thumbnail, "image/jpeg"))
        })
        .await;

        match read {
            Ok(Ok((bytes, mime))) => response(200, mime, Body::from(bytes), &[]),
            Ok(Err(error)) if error.code == Code::BadRequest => json_response(404, json!(error)),
            Ok(Err(error)) => json_response(500, json!(error)),
            Err(_) => json_response(
                500,
                json!(Error::new(
                    Code::Capture,
                    "The screenshot could not be read"
                )),
            ),
        }
    }

    /// A helper just logged in has no input service yet, and switching
    /// accounts can leave one pinned to a display that is gone or renumbered
    /// (which taking control refuses). Input starts or re-pins once per
    /// helper daemon and display, as Recover would, so agents and viewers
    /// carry on by themselves: only in the background, with Accessibility on
    /// and nothing pending or held.
    fn start_input_after_login(&self, client: &Arc<Client>, session: &Value, settled: bool) {
        let native = &session["inputService"];
        let starts = settled
            && native["controlActive"] != true
            && native["heldInput"] != true
            && session["permissions"]["accessibility"] == true;
        let Some(hidden) = starts
            .then(|| crate::session::require_hidden(session, client.config.helper_uid).ok())
            .flatten()
            .filter(|hidden| !hidden.input)
        else {
            return;
        };

        let Some(daemon) = session["daemon"]["pid"].as_u64() else {
            return;
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
        if first {
            let client = client.clone();
            tokio::spawn(async move {
                let _ = client.start_input().await;
            });
        }
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
            crate::cleanup::for_person(&client, &session, crate::cleanup::Mode::Preview),
        )
        .await
        .map_err(|_| late())?
    }
}

/// Account `name`'s full name, as `ask` finds it, kept in `known`. It is
/// asked once per account (off the runtime, as it asks directory services),
/// also when it has none, but again after an ask that had no answer.
pub(super) async fn account_title(
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
/// the app, control and keys its words name. Only these fields go, through
/// [`crate::overview::shown`]: never typed text, argv, coordinates or
/// titles, which the log never keeps either.
pub(super) fn activity_row(row: &crate::activity_log::Row, now: u128) -> Value {
    let shown = crate::overview::shown(row, now);
    json!({
        "id": shown.id,
        "logicalId": row.logical_id(),
        // When it started, in milliseconds since 1970, so its time never
        // moves between readings.
        "startedAt": row.at_ms(),
        "says": shown.says,
        "ok": shown.ok,
        "started": shown.started,
        "actor": shown.actor,
        "ageSeconds": shown.age_seconds,
        "reason": shown.reason,
        "operation": shown.operation,
        "count": shown.action_count,
        "session": shown.session,
        "capture": shown.capture,
        "verb": shown.verb,
        "app": shown.app,
        "label": shown.label,
        "bundleId": shown.bundle_id,
        "keys": shown.keys(),
        "nothingSent": shown.nothing_sent,
        "partial": shown.partial,
        "uncertain": shown.uncertain,
    })
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
        let reading = crate::overview::SessionReading::collect(&client);
        let processes = reading.as_ref().map(|reading| &reading.processes);
        let now = crate::host_session::now_ms();
        let facts = crate::overview::SessionFacts::local(&client.controller, u128::from(now) * 1_000_000);
        let sessions: Map<String, Value> = records()
            .filter(|record| wanted.contains(record.key.as_str()))
            .map(|record| {
                let work = reading.as_ref().is_some_and(|reading| reading.work(&record.key, facts));
                let session = json!({
                    "name": name_of(&named, &record.key, Some(record.host)),
                    "host": record.host,
                    "hostLabel": record.host.label(),
                    "started": record.started,
                    "ended": record.ended.or_else(|| processes.and_then(|processes| processes.lifetime(record, now, work).ended)),
                });
                (record.key.to_string(), session)
            }).collect();
        Value::Object(sessions)
    }).await.unwrap_or_else(|_| json!({}))
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

/// The helper's regular apps as the app lists them, front to back, each
/// with its bundle identifier, whether it is hidden, when it was launched
/// and its windows front to back. A window's id is its window number, and
/// its order its place among every app's windows, front to back.
pub(super) fn listed_apps(apps: &Value, windows: &Value) -> Result<Vec<Value>> {
    Ok(helper_info::open_apps(apps, windows)?
        .into_iter()
        .map(|app| {
            json!({
                "pid": app["pid"],
                "name": app["name"],
                "bundle": app["bundle"],
                "bundleId": app["bundleId"],
                "front": app["active"],
                "hidden": app["hidden"],
                "launchedAt": app["launchedAt"],
                "windows": app["windows"],
            })
        })
        .collect())
}
