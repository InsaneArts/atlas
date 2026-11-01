use super::{
    crop, facts,
    protocol::{Crop, Region, Request, failure, refused},
};
use crate::{
    error::{Code, Result},
    native::{capture, windows},
    session::InputEpoch,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use serde_json::{Value, json};

/// Real display capture in the daemon, synthetic pixels in contract tests.
pub(super) type Capture = fn(u32, Option<CGRect>, Option<u32>) -> Result<capture::Shot>;

/// Observations from this helper login, shared by capture and window reads.
pub(super) struct Screen {
    pub(super) capture: Capture,
    pub(super) windows: fn() -> Result<Vec<windows::Window>>,
    pub(super) bundle_id: fn(i64) -> Option<String>,
    pub(super) dock: fn(&[windows::Window]) -> windows::Dock,
    pub(super) on_console: fn() -> bool,
    pub(super) session: fn() -> Result<Value>,
}

/// Windows that lie entirely outside the main display.
fn off_display_windows(windows: &[windows::Window]) -> Vec<Value> {
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
) -> serde_json::Map<String, Value> {
    let mut keys = serde_json::Map::new();
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
    let capture = screen.capture;
    let list = screen.windows;
    let bundle_id = screen.bundle_id;
    let dock = screen.dock;
    let on_console = screen.on_console;
    let session = screen.session;
    let epoch = match session().and_then(|proof| InputEpoch::from_proof(&proof, std::process::id()))
    {
        Ok(epoch) => epoch,
        Err(error) => return refused(error),
    };
    let taken = tokio::task::spawn_blocking(move || {
        let request = Request::Screenshot {
            max_dimension,
            per_point,
            crop,
        };
        let validate = || {
            if let Some(refusal) = request.policy().allowed(on_console()) {
                return Err(refusal);
            }
            let current = InputEpoch::from_proof(&session().map_err(refused)?, std::process::id())
                .map_err(refused)?;
            if current != epoch {
                return Err(failure(
                    Code::SessionChanged,
                    "Session changed during capture; observe again",
                ));
            }
            Ok(())
        };
        validate()?;

        // One read of the window list serves the crop and the report of
        // windows off the display.
        let windows = list();
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
                let found = crop::app(on_screen, pid, &display, &dock(on_screen)).ok_or_else(|| {
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
        let shot = capture(epoch.display_id as u32, area.map(rect), limit).map_err(refused)?;
        let off_display = off_display_windows(windows.as_deref().unwrap_or_default());

        let cropped = area.map(|area| crop_keys(area, app, bundle_id));
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
