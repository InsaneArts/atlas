use super::{crop, facts};
use atlas_core::screen::windows::{Dock, Window};
use atlas_core::{
    error::{Code, Error, Result},
    protocol::{Crop, Policy, Region, Request, failure, refused},
    session::InputEpoch,
};
use atlas_native::native::capture;
use base64::{Engine, engine::general_purpose::STANDARD};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use serde_json::{Map, Value, json};

/// Real display capture in the daemon, synthetic pixels in contract tests.
pub(super) type Capture = fn(u32, Option<CGRect>, Option<u32>) -> Result<capture::Shot>;

/// Real raw display capture in the daemon, synthetic pixels in stream tests.
pub(super) type RawCapture = fn(u32, Option<u32>) -> Result<capture::RawShot>;

/// Observations from this helper login, shared by capture and window reads.
#[derive(Clone, Copy)]
pub(super) struct Screen {
    pub(super) capture: Capture,
    pub(super) raw_capture: RawCapture,
    pub(super) windows: fn() -> Result<Vec<Window>>,
    pub(super) bundle_id: fn(i64) -> Option<String>,
    pub(super) dock: fn(&[Window]) -> Dock,
    pub(super) on_console: fn() -> bool,
    pub(super) session: fn() -> Result<Value>,
}

impl Screen {
    pub(super) fn epoch(self) -> Result<InputEpoch> {
        (self.session)().and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
    }

    /// A fresh console check precedes the exact session and display comparison.
    pub(super) fn guard(
        self,
        epoch: &InputEpoch,
        policy: Policy,
    ) -> std::result::Result<(), Value> {
        if let Some(refusal) = policy.allowed((self.on_console)()) {
            return Err(refusal);
        }
        if self.epoch().map_err(refused)? != *epoch {
            return Err(refused(Error::session_changed()));
        }
        Ok(())
    }
}

/// Windows that lie entirely outside the main display.
fn off_display_windows(windows: &[Window]) -> Vec<Value> {
    let (x, y, width, height) = facts::display_bounds();
    windows
        .iter()
        .filter(|window| window.layer == 0)
        .filter(|window| {
            let b = &window.bounds;
            b.x + b.width <= x || b.y + b.height <= y || b.x >= x + width || b.y >= y + height
        })
        .map(
            |window| json!({"number": window.number, "owner": window.owner, "title": window.title}),
        )
        .collect()
}

/// A region as the rectangle a capture takes.
fn rect(region: Region) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(region.x), f64::from(region.y)),
        CGSize::new(f64::from(region.width), f64::from(region.height)),
    )
}

/// What a cropped screenshot's reply adds: the area captured and, for an
/// app, which app and what lies in front of it.
fn crop_keys(
    area: Region,
    app: Option<(i64, crop::AppCrop)>,
    bundle_id: fn(i64) -> Option<String>,
) -> Map<String, Value> {
    let mut keys = Map::new();
    keys.insert("region".into(), json!(area));

    if let Some((pid, found)) = app {
        let overlapping: Vec<Value> = found
            .overlapping
            .iter()
            .map(|window| {
                json!({
                    "owner": window.owner,
                    "pid": window.pid,
                    "title": window.title,
                    "bounds": window.bounds,
                })
            })
            .collect();
        keys.insert(
            "app".into(),
            json!({"pid": pid, "name": found.name, "bundleId": bundle_id(pid)}),
        );
        keys.insert("overlapping".into(), json!(overlapping));
    }

    keys
}

pub(super) async fn screenshot(
    screen: &Screen,
    max_dimension: Option<u32>,
    per_point: bool,
    crop: Option<Crop>,
) -> Value {
    let screen = *screen;
    let epoch = match screen.epoch() {
        Ok(epoch) => epoch,
        Err(error) => return refused(error),
    };
    let policy = Request::Screenshot {
        max_dimension,
        per_point,
        crop,
    }
    .policy();
    let taken = tokio::task::spawn_blocking(move || {
        let validate = || screen.guard(&epoch, policy);
        validate()?;

        // One read of the window list serves the crop and the report of
        // windows off the display.
        let windows = (screen.windows)();
        // The main display's corner is the origin of global points, so a
        // window's bounds are points of this display.
        let display = Region {
            x: 0,
            y: 0,
            width: epoch.width.round() as i32,
            height: epoch.height.round() as i32,
        };
        let (area, app) = match crop {
            None => (None, None),
            Some(Crop::Region(asked)) => {
                let area = crop::clamp(asked, &display).ok_or_else(|| {
                    failure(Code::BadRequest, "\"region\" lies outside the display")
                })?;
                (Some(area), None)
            }
            Some(Crop::App { pid }) => {
                let on_screen = windows
                    .as_ref()
                    .map_err(|_| failure(Code::Windows, "The window list is unavailable"))?;
                let dock = (screen.dock)(on_screen);
                let found = crop::app(on_screen, pid, &display, &dock).ok_or_else(|| {
                    failure(
                        Code::NoWindow,
                        format!(
                            "Process {pid} has no window on screen; open one, or take a full screenshot"
                        ),
                    )
                })?;
                (Some(found.region), Some((pid, found)))
            }
        };

        let limit = if per_point {
            let (width, height) = area.map_or((epoch.width, epoch.height), |area| {
                (f64::from(area.width), f64::from(area.height))
            });
            capture::per_point(max_dimension, width.max(height))
        } else {
            max_dimension
        };
        let shot =
            (screen.capture)(epoch.display_id as u32, area.map(rect), limit).map_err(refused)?;
        let off_display = off_display_windows(windows.as_deref().unwrap_or_default());

        let cropped = area.map(|area| crop_keys(area, app, screen.bundle_id));
        validate()?;

        Ok((shot, off_display, cropped))
    })
    .await;
    match taken {
        Ok(Ok((shot, off_display, cropped))) => {
            let mut reply = json!({
                "ok": true,
                "png": STANDARD.encode(&shot.png),
                "width": shot.width,
                "height": shot.height,
                "scale": shot.scale,
                "offDisplayWindows": off_display,
            });
            if let (Some(reply), Some(cropped)) = (reply.as_object_mut(), cropped) {
                reply.extend(cropped);
            }
            reply
        }
        Ok(Err(refusal)) => refusal,
        Err(_) => failure(Code::Capture, "The capture failed"),
    }
}
