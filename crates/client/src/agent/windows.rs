//! The helper's on-screen windows: which ones a command is about, read
//! through its daemon, listed, and waited for until one appears or
//! none is left.
use super::{
    elements::{Scanned, app_elements_within, compact},
    target::{Absent, Target, target_expect},
};
use crate::client::{Client, WINDOW_LOOKUP, require_looks};
use atlas_core::{
    error::{Code, Error, Result},
    name::same_text,
    screen::{
        geometry::Bounds,
        select::ElementFilter,
        wait::{Judged, State, Why, judge},
        windows::Window,
    },
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
pub async fn list(client: &Client, filter: &WindowFilter) -> Result<Vec<Window>> {
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

    Ok(filter.selected(windows))
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

/// How long a wait rests between two looks.
const POLL: Duration = Duration::from_millis(250);

/// The longest one scan of a wait may take.
const SCAN: Duration = Duration::from_secs(5);

/// Look at the target app's elements until what `filter` picks shows
/// `state`, as [`judge`] tells it. The timeout is a deadline for starting a
/// look, and a zero timeout means one. Each scan is stopped after what is
/// left of the deadline, at least half a second; finding the app and the
/// confirming look keep their own few seconds, so the last look may end a
/// little after the deadline, and counts. Each look reads the app through the
/// helper's daemon and keeps nothing; once one shows the state, the input
/// service looks once more and keeps that look, which must show it too, so
/// the reply names an element by a ref the agent can act on at once. An
/// app that shows no window yet has nothing to match; one that quit has
/// nothing left of what `gone` waits for.
///
/// # Errors
///
/// Refuses a wait with neither `--name` nor `--role`, and returns identity,
/// target and accessibility-read failures. A timeout is a `wait-failed`
/// reply saying what the last look saw. Waiting sends nothing.
pub async fn wait_for_element(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    state: &State,
    timeout: Duration,
) -> Result<Value> {
    if filter.name.is_none() && filter.role.is_none() {
        return Err(Error::new(
            Code::Usage,
            "Wait for an element by --name, --role or both",
        ));
    }

    let started = Instant::now();
    let deadline = started
        .checked_add(timeout)
        .ok_or_else(|| Error::new(Code::Usage, "That --timeout is too long to wait"))?;

    loop {
        // Each scan gets what is left of the deadline (at least half a
        // second, at most the five a scan usually has); a zero timeout is one
        // whole observation.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let budget = if timeout.is_zero() {
            SCAN
        } else {
            remaining.clamp(Duration::from_millis(500), SCAN)
        };
        let why = match observe(client, target, filter, state, budget).await? {
            Observed::Met(found) => {
                let mut said = json!({
                    "ok": true,
                    "waitedMs": atlas_core::time::ms(started.elapsed()),
                    "state": state.word(),
                    "matched": true,
                });
                if let Some((look, element)) = found {
                    said["look"] = json!(look);
                    said["element"] = element;
                }
                return Ok(said);
            }
            Observed::Unmet(why) => why,
        };

        let observed = Instant::now();
        if timeout.is_zero() || observed >= deadline {
            return Ok(json!(
                Error::new(
                    Code::WaitFailed,
                    format!("{}: {}", state.missed(), why.says())
                )
                .with("timedOut", json!(true))
                .with("state", json!(state.word()))
                .with("last", json!({"why": why, "says": why.says()}))
            ));
        }

        tokio::time::sleep((deadline - observed).min(POLL)).await;
    }
}

/// What one look, confirmed by a kept one when it shows the state, saw.
enum Observed {
    /// The state holds; the kept look and the element it is about, when
    /// the state names one.
    Met(Option<(String, Value)>),
    Unmet(Why),
}

/// One look at the target app's elements for `state`.
async fn observe(
    client: &Client,
    target: &Target,
    filter: &ElementFilter,
    state: &State,
    budget: Duration,
) -> Result<Observed> {
    let verified = client.verify().await?;
    let display = &verified.session().epoch.display;
    let display = Bounds::display(display.width, display.height);
    let expect = match target_expect(client, &verified, target).await {
        Ok(expect) => expect,
        // An app that shows no window, or no longer runs, has none of its
        // elements on screen.
        Err(error) if Absent::of(&error) => {
            return Ok(match state {
                State::Gone => Observed::Met(None),
                _ => Observed::Unmet(Why::None),
            });
        }
        Err(error) => return Err(error),
    };

    // A scan its budget stopped saw only part of the app, if anything.
    let Some(scanned) = app_elements_within(client, &verified, expect.pid(), budget).await? else {
        return Ok(Observed::Unmet(Why::Incomplete));
    };
    let index = match judged(&scanned, filter, state, &display) {
        Judged::Met(index) => index,
        Judged::Unmet(why) => return Ok(Observed::Unmet(why)),
    };
    if index.is_none() {
        return Ok(Observed::Met(None));
    }

    // The kept look is the one the reply is about: it must show the state.
    require_looks(&verified)?;
    let kept = client
        .look(&verified, async {
            Scanned::from_reply(client.look_at(&verified, &expect).await?)
        })
        .await?;
    let Some(look) = kept.look else {
        return Err(Error::new(
            Code::Elements,
            "The input service named no look",
        ));
    };
    match judged(&kept, filter, state, &display) {
        Judged::Met(Some(index)) => {
            let element = kept
                .elements
                .iter()
                .find(|element| element.index == index)
                .map(|element| {
                    let mut listed = compact(element, &mut Vec::new());
                    if let Some(listed) = listed.as_object_mut() {
                        listed.remove("window");
                    }
                    listed
                });
            Ok(Observed::Met(
                element.map(|element| (look.to_string(), element)),
            ))
        }
        Judged::Met(None) => Ok(Observed::Met(None)),
        Judged::Unmet(why) => Ok(Observed::Unmet(why)),
    }
}

/// [`judge`] on `scanned`, whose coverage is unknown unless its reply said.
fn judged(scanned: &Scanned, filter: &ElementFilter, state: &State, display: &Bounds) -> Judged {
    let incomplete = scanned.incomplete || !scanned.coverage_told;
    judge(&scanned.elements, incomplete, filter, state, display)
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

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
