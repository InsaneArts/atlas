//! The helper's on-screen windows: which ones a command is about, read
//! through its daemon, listed, and waited for until one appears or
//! none is left.
use crate::client::{Client, WINDOW_LOOKUP};
use atlas_core::{
    error::{Code, Error, Result},
    name::same_text,
    screen::windows::Window,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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
    pub(crate) fn matches(&self, window: &Window) -> bool {
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

    /// The windows among `windows` this filter selects.
    fn selected(&self, windows: Vec<Window>) -> Vec<Window> {
        windows
            .into_iter()
            .filter(|window| self.matches(window))
            .collect()
    }
}

/// List the helper windows selected by `filter`, retaining their observed routing facts.
///
/// # Errors
///
/// Returns identity, window-read or remembered-observation storage failures. Malformed
/// helper data is never interpreted as an empty window list.
pub async fn list(client: &Client, filter: &WindowFilter) -> Result<Value> {
    let windows = if client.host_session.is_some() {
        let verified = client.verify().await?;
        let snapshot = client
            .windows_in(Some(&verified), true, WINDOW_LOOKUP)
            .await?;
        client.remember_look(&verified, Some(&snapshot))?;
        snapshot.windows
    } else {
        client.windows_in(None, false, WINDOW_LOOKUP).await?.windows
    };

    Ok(json!({ "ok": true, "windows": filter.selected(windows) }))
}

/// Poll the window list until a matching window appears (or, with `gone`,
/// until none remains). The timeout is a deadline for starting a look; the
/// last look counts even when it finishes after it, and a zero timeout
/// means one observation.
///
/// # Errors
///
/// Returns observation failures or a timeout if the requested window state does not arrive.
/// Waiting does not synthesize that state or send input.
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
            WINDOW_LOOKUP
        } else {
            probe_budget(remaining)
        };

        let listed = client.windows_in(None, false, budget).await?;
        let matching = filter.selected(listed.windows);

        if matching.is_empty() == gone {
            return Ok(json!({
                "ok": true,
                "waitedMs": atlas_core::time::ms(started.elapsed()),
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
                Error::new(Code::WaitFailed, error).with("timedOut", json!(true))
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

    let Ok(apps) = client.running_apps().await else {
        return fallback;
    };

    let running = apps
        .rows()
        .iter()
        .find(|running| running.name().is_some_and(|name| same_text(name, app)));
    match running {
        None => format!("{app} is not running"),
        Some(running) if running.policy() == Some("accessory") => format!(
            "{app} is running but shows no window: a menu bar app opens its windows from its \
             menu bar item"
        ),
        Some(_) => format!("{app} is running but shows no window"),
    }
}
