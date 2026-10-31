//! The helper's on-screen windows: which ones a command is about, read
//! through a read-only probe, listed, and waited for until one appears or
//! none is left.
use crate::{
    client::{Client, Run, Verified},
    error::{Code, Error, Result},
    native::windows::{Shown, Window},
    ui::text::same_text,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// How long a read-only look at the helper (its window list) may take.
const PROBE_LIMIT: Duration = Duration::from_secs(5);

/// Which helper windows a command is about.
#[derive(Debug, Default, Clone)]
pub struct WindowFilter {
    /// Owning application name, compared case-insensitively.
    pub app: Option<String>,
    /// Text the window title must contain, compared case-insensitively.
    pub title: Option<String>,
    /// Include menus, the menu bar, the Dock and other system layers.
    pub all: bool,
}

impl WindowFilter {
    pub fn matches(&self, window: &Window) -> bool {
        // An app's floating panels and alerts count as its windows, so a
        // system alert in front of everything is listed too.
        (self.all || window.is_apps())
            && self.app.as_ref().is_none_or(|app| {
                window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| same_text(owner, app))
            })
            && self.title.as_ref().is_none_or(|title| {
                window
                    .title
                    .as_deref()
                    .is_some_and(|own| own.to_lowercase().contains(&title.to_lowercase()))
            })
    }
}

/// The window list alone, with the helper probe bounded by `limit`.
async fn window_list(client: &Client, limit: Duration) -> Result<Vec<Window>> {
    let probe = windows_probe(client, DockRead::Skip);
    let reply = client.run(probe, Run::inspect().within(limit)).await?;
    Ok(shown(&reply)?.windows)
}

/// The window list and where the Dock takes clicks, to judge a point by.
pub(super) async fn pointer_view(client: &Client) -> Result<Shown> {
    let probe = windows_probe(client, DockRead::Read);
    let reply = client
        .run(probe, Run::inspect().within(PROBE_LIMIT))
        .await?;
    shown(&reply)
}

/// The windows and process identities read together inside the helper.
pub(super) async fn verified_window_snapshot(
    client: &Client,
    verified: &Verified,
) -> Result<Shown> {
    let probe = windows_probe(client, DockRead::Skip);
    let reply = client
        .run(probe, Run::inspect().within(PROBE_LIMIT).pinned(verified))
        .await?;
    shown(&reply)
}

/// Whether a window probe also reads where the Dock takes clicks, which
/// only a check of a point needs.
#[derive(Clone, Copy)]
pub(super) enum DockRead {
    Skip,
    Read,
}

/// The probe that lists the helper's windows.
pub(super) fn windows_probe(client: &Client, dock: DockRead) -> Vec<String> {
    let probe = client.config.identity_probe.to_string_lossy().into_owned();
    let mut argv = vec![probe, "windows-info".into()];
    if matches!(dock, DockRead::Read) {
        argv.push("--dock".into());
    }
    argv
}

/// What the window probe's `reply` shows.
fn shown(reply: &Value) -> Result<Shown> {
    crate::native::windows::from_probe(reply)
        .ok_or_else(|| Error::new(Code::Windows, "The helper window list is unavailable"))
}

pub async fn list(client: &Client, filter: &WindowFilter) -> Result<Value> {
    let verified = client.verify().await?;
    let windows = verified_window_snapshot(client, &verified).await?.windows;
    client.remember_windows(&windows, &verified.session().epoch.display)?;
    let matching: Vec<_> = windows
        .into_iter()
        .filter(|window| filter.matches(window))
        .collect();

    Ok(json!({ "ok": true, "windows": matching }))
}

/// Poll the window list until a matching window appears (or, with `gone`,
/// until none remains). The timeout is a deadline for starting a look; the
/// last look counts even when it finishes after it, and a zero timeout
/// means one observation.
pub async fn wait(
    client: &Client,
    filter: &WindowFilter,
    gone: bool,
    timeout: Duration,
) -> Result<Value> {
    if filter.app.is_none() && filter.title.is_none() {
        return Err(Error::new(Code::Usage, "Wait for --app, --title, or both"));
    }

    let started = Instant::now();
    let deadline = started
        .checked_add(timeout)
        .ok_or_else(|| Error::new(Code::Usage, "That --timeout is too long to wait"))?;

    loop {
        // Each observation gets what is left of the deadline (at least half
        // a second, at most what the helper lets a command take); a zero
        // timeout is exactly one observation.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let budget = if timeout.is_zero() {
            PROBE_LIMIT
        } else {
            probe_budget(remaining)
        };

        let matching: Vec<_> = window_list(client, budget)
            .await?
            .into_iter()
            .filter(|window| filter.matches(window))
            .collect();

        if matching.is_empty() == gone {
            return Ok(json!({
                "ok": true,
                "waitedMs": crate::time::ms(started.elapsed()),
                "windows": matching,
            }));
        }

        let observed = Instant::now();
        if timeout.is_zero() || observed >= deadline {
            let error = if gone {
                "The window did not close in time".to_owned()
            } else {
                no_window(client, filter).await
            };

            return Ok(json!(
                Error::new(Code::WaitFailed, error).with("timedOut", true)
            ));
        }

        tokio::time::sleep((deadline - observed).min(Duration::from_millis(250))).await;
    }
}

/// How long one look at the window list may take with `remaining` left to
/// wait: at least half a second, and no longer than the helper allows.
pub(super) fn probe_budget(remaining: Duration) -> Duration {
    remaining
        .max(Duration::from_millis(500))
        .min(crate::client::RUN_TIMEOUT_MAX)
}

/// Why no window appeared: the app is not running, or it runs without a
/// window, as a menu bar app does until its menu bar item opens one.
async fn no_window(client: &Client, filter: &WindowFilter) -> String {
    let fallback = "No matching window appeared in time".to_owned();

    let Some(app) = filter.app.as_deref().filter(|_| filter.title.is_none()) else {
        return fallback;
    };

    let Ok(apps) = client.apps().await else {
        return fallback;
    };

    let running = apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|running| {
            running["name"]
                .as_str()
                .is_some_and(|name| same_text(name, app))
        });
    match running {
        None => format!("{app} is not running"),
        Some(running) if running["policy"] == "accessory" => format!(
            "{app} is running but shows no window: a menu bar app opens its windows from its \
             menu bar item"
        ),
        Some(_) => format!("{app} is running but shows no window"),
    }
}
