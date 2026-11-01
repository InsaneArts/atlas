//! The helper's on-screen windows: which ones a command is about, read
//! through its daemon, listed, and waited for until one appears or
//! none is left.
use crate::{
    client::{Client, Verified},
    error::{Code, Error, Result},
    last_capture,
    native::windows::{Bounds, Shown, Window, hit_windows, same_routing},
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

/// The window list alone, with the daemon read bounded by `limit`.
async fn window_list(client: &Client, limit: Duration) -> Result<Vec<Window>> {
    Ok(client.windows_in(None, false, limit).await?.windows)
}

/// The window list and where the Dock takes clicks, to judge a point by.
pub(super) async fn pointer_view(client: &Client) -> Result<Shown> {
    client.windows_in(None, true, PROBE_LIMIT).await
}

/// The windows and process identities read together inside the helper.
pub(super) async fn window_snapshot(
    client: &Client,
    verified: &Verified,
    dock: DockRead,
) -> Result<Shown> {
    client
        .windows_in(Some(verified), matches!(dock, DockRead::Read), PROBE_LIMIT)
        .await
}

/// Whether a window read also reads where the Dock takes clicks, which
/// only a check of a point needs.
#[derive(Clone, Copy)]
pub(super) enum DockRead {
    Skip,
    Read,
}

pub async fn list(client: &Client, filter: &WindowFilter) -> Result<Value> {
    let windows = if client.host_session.is_some() {
        let verified = client.verify().await?;
        let snapshot = window_snapshot(client, &verified, DockRead::Read).await?;
        client.remember_look(&verified, Some(&snapshot))?;
        snapshot.windows
    } else {
        window_list(client, PROBE_LIMIT).await?
    };
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

impl Client {
    pub(crate) async fn look<T>(
        &self,
        verified: &Verified,
        look: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        if self.host_session.is_none() {
            return look.await;
        }
        let before = window_snapshot(self, verified, DockRead::Read).await.ok();
        let result = look.await?;
        let after = window_snapshot(self, verified, DockRead::Skip).await.ok();
        let unchanged = before.as_ref().filter(|before| {
            after
                .as_ref()
                .is_some_and(|after| same_routing(&before.windows, &after.windows))
        });
        self.remember_look(verified, unchanged)?;
        Ok(result)
    }

    fn remember_look(&self, verified: &Verified, snapshot: Option<&Shown>) -> Result<()> {
        let Some(session) = &self.host_session else {
            return Ok(());
        };
        let epoch = &verified.session().epoch;
        let display = &epoch.display;
        let windows = snapshot.map_or_else(Vec::new, |snapshot| {
            hit_windows(
                &snapshot.windows,
                &Bounds::display(display.width, display.height),
                &snapshot.dock,
                snapshot.starts.as_ref(),
            )
        });
        last_capture::remember_windows(self.output_dir()?, session.key().as_str(), &windows, epoch)
    }
}
