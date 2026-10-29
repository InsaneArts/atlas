//! Screenshots of the helper's display: a [`Frame`] read and checked
//! against the session it came from, a crop checked against the one asked
//! for, and the saved capture that says where its pixels lie on the
//! display. Captures are kept for a week.
use super::{Client, Verified, record::recorded_code};
use crate::{
    daemon::protocol::{Crop, Region},
    error::{Code, Error, Result},
    last_capture,
    session::{Display, Epoch},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Frame {
    pub png: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub display: Display,
    pub epoch: Epoch,
    pub at: Instant,
}

/// What a cropped screenshot's reply says it shows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cropped {
    pub(super) region: Region,
    /// The app shown, for an app's crop.
    pub(super) app: Value,
    /// Other apps' windows in front of it, for an app's crop.
    pub(super) overlapping: Value,
}

/// Whether `region` and the display share any area.
pub(super) fn meets(region: &Region, display: &Display) -> bool {
    let (x, y) = (f64::from(region.x), f64::from(region.y));
    x < display.width
        && y < display.height
        && x + f64::from(region.width) > 0.0
        && y + f64::from(region.height) > 0.0
}

/// The crop a screenshot reply says it shows, checked against the one asked
/// for: an older helper that ignores a crop sends the whole display, which
/// must never be read as the crop.
pub(super) fn cropped(
    reply: &Value,
    crop: Option<&Crop>,
    display: &Display,
) -> Result<Option<Cropped>> {
    let sent = reply.get("region").filter(|region| !region.is_null());
    let (crop, sent) = match (crop, sent) {
        (None, None) => return Ok(None),
        (None, Some(_)) => {
            return Err(Error::new(
                Code::Protocol,
                "The helper sent a crop that was not asked for",
            ));
        }
        (Some(_), None) => {
            return Err(Error::new(
                Code::Daemon,
                "The helper is running an older Atlas that cannot crop screenshots; run atlas \
                 restart, then try again",
            ));
        }
        (Some(crop), Some(sent)) => (crop, sent),
    };

    let malformed = |what: &str| Error::new(Code::Protocol, format!("The helper sent {what}"));
    let region: Region = serde_json::from_value(sent.clone())
        .map_err(|_| malformed("a crop that could not be read"))?;
    let (left, top) = (i64::from(region.x), i64::from(region.y));
    let (right, bottom) = (
        left + i64::from(region.width),
        top + i64::from(region.height),
    );
    let on_display = left >= 0
        && top >= 0
        && region.width >= 1
        && region.height >= 1
        && right as f64 <= display.width.round()
        && bottom as f64 <= display.height.round();
    if !on_display {
        return Err(malformed("a crop outside the display"));
    }

    match crop {
        Crop::Region(asked) => {
            let inside = left >= i64::from(asked.x)
                && top >= i64::from(asked.y)
                && right <= i64::from(asked.x) + i64::from(asked.width)
                && bottom <= i64::from(asked.y) + i64::from(asked.height);
            if !inside {
                return Err(malformed("a crop other than the region asked for"));
            }
        }
        Crop::App { pid } => {
            if reply["app"]["pid"].as_i64() != Some(*pid) || !reply["overlapping"].is_array() {
                return Err(malformed("a crop of another app"));
            }
        }
    }

    let (app, overlapping) = match crop {
        Crop::App { .. } => (reply["app"].clone(), reply["overlapping"].clone()),
        Crop::Region(_) => (Value::Null, Value::Null),
    };

    Ok(Some(Cropped {
        region,
        app,
        overlapping,
    }))
}

/// Where a screenshot's top-left pixel lies, in display points, and how many
/// points each of its pixels spans: pixel (px, py) is the point (x + px ×
/// scale, y + py × scale).
pub(super) fn placement(
    display: &Display,
    width: u32,
    region: Option<&Region>,
) -> ((f64, f64), f64) {
    match region {
        Some(region) => (
            (f64::from(region.x), f64::from(region.y)),
            f64::from(region.width) / f64::from(width),
        ),
        None => ((0.0, 0.0), display.width / f64::from(width)),
    }
}

/// The longest side of an agent's screenshot, in pixels, unless it asks for
/// another size: enough to see layout, menu titles and controls at about
/// half the cost of the full display.
pub const SCREENSHOT_SIDE: u32 = 1456;

/// How large a saved screenshot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// At most one pixel per display point and at most [`SCREENSHOT_SIDE`]
    /// pixels on its longer side, so a crop of a Retina display costs no more
    /// than the same crop at one pixel per point.
    Standard,
    /// At most this many pixels on its longer side, never enlarged.
    Longest(u32),
    /// Every pixel the display has.
    Full,
}

impl Size {
    /// The most pixels the longer side may have, if any limit.
    fn longest(self) -> Option<u32> {
        match self {
            Size::Standard => Some(SCREENSHOT_SIDE),
            Size::Longest(side) => Some(side),
            Size::Full => None,
        }
    }
}

/// The helper's request for a screenshot of `size`, before any crop.
pub(super) fn screenshot_request(size: Size) -> Value {
    let mut request = json!({"op": "screenshot"});

    if let Some(max) = size.longest() {
        request["maxDimension"] = json!(max);
    }
    // An older helper ignores this and keeps a small crop's full size.
    if size == Size::Standard {
        request["perPoint"] = json!(true);
    }

    request
}

/// Screenshots are kept for a week; older ones are removed as new ones come.
pub(crate) fn prune_captures(output: &Path) {
    const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);

    let Ok(entries) = fs::read_dir(output) else {
        return;
    };

    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > WEEK);

        let capture = entry
            .file_name()
            .to_str()
            .and_then(crate::helper_info::capture_time)
            .is_some();
        if old && capture && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// The file name of the capture a screenshot's reply `saved` names.
pub(super) fn capture_name(saved: &Value) -> Option<&str> {
    Path::new(saved["path"].as_str()?).file_name()?.to_str()
}

pub fn png_dimensions(png: &[u8]) -> Result<(u32, u32)> {
    let invalid = || Error::new(Code::Capture, "Invalid PNG header or dimensions");

    if png.len() < 33 || &png[..16] != b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR" {
        return Err(invalid());
    }

    let width = u32::from_be_bytes(png[16..20].try_into().map_err(|_| invalid())?);
    let height = u32::from_be_bytes(png[20..24].try_into().map_err(|_| invalid())?);
    if width == 0 || height == 0 || width > 32768 || height > 32768 {
        return Err(invalid());
    }

    Ok((width, height))
}

impl Client {
    pub async fn frame(&self, maximum: Option<u32>) -> Result<Frame> {
        let size = maximum.map_or(Size::Full, Size::Longest);

        self.framed(size, None, None).await.map(|(frame, _)| frame)
    }

    /// A frame of the whole display, or of the crop asked for, with what the
    /// helper says the crop shows. With `expected`, the helper must still be
    /// in that epoch when the capture starts, as when the crop names a
    /// process found in it.
    async fn framed(
        &self,
        size: Size,
        crop: Option<&Crop>,
        expected: Option<&Epoch>,
    ) -> Result<(Frame, Option<Cropped>)> {
        let maximum = size.longest();
        if maximum.is_some_and(|m| m == 0 || m > 8192) {
            return Err(Error::new(Code::Input, "Maximum dimension must be 1–8192"));
        }

        let before = self.identity().await?;
        if expected.is_some_and(|epoch| *epoch != before.epoch) {
            return Err(Error::new(
                Code::SessionChanged,
                "The helper changed since the app was found; look again",
            ));
        }
        let mut request = screenshot_request(size);

        match crop {
            None => {}
            Some(Crop::App { pid }) => request["pid"] = json!(pid),
            Some(Crop::Region(region)) => {
                let display = &before.epoch.display;
                if !meets(region, display) {
                    return Err(Error::new(
                        Code::Usage,
                        format!(
                            "The region lies outside the display, which is {} × {} points",
                            display.width, display.height
                        ),
                    ));
                }
                request["region"] = json!(region);
            }
        }

        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true {
            if reply["code"].is_string() {
                return Err(crate::transport::refusal(&reply));
            }

            return Err(Error::new(
                Code::Capture,
                reply["error"].as_str().unwrap_or("Capture failed"),
            ));
        }
        let shows = cropped(&reply, crop, &before.epoch.display)?;

        let at = Instant::now();
        let encoded = reply["png"]
            .as_str()
            .ok_or_else(|| Error::new(Code::Capture, "Missing PNG"))?;
        let png = STANDARD
            .decode(encoded)
            .map_err(|_| Error::new(Code::Capture, "Invalid PNG encoding"))?;
        let (width, height) = png_dimensions(&png)?;
        if reply["width"].as_u64() != Some(width.into())
            || reply["height"].as_u64() != Some(height.into())
        {
            return Err(Error::new(
                Code::Capture,
                "PNG dimensions disagree with metadata",
            ));
        }

        let after = self.identity().await?;
        if before.epoch != after.epoch {
            return Err(Error::new(
                Code::SessionChanged,
                "Session changed during capture; observe again",
            ));
        }

        Ok((
            Frame {
                png: png.into(),
                width,
                height,
                display: after.epoch.display.clone(),
                epoch: after.epoch,
                at,
            },
            shows,
        ))
    }

    /// Save a screenshot of the whole display, or of `crop`, and say where
    /// its pixels lie on the display. A capture with the same pixels as the
    /// last one of that target and size names the earlier file and says it
    /// is `unchanged`, unless `new_file` asks for a file anyway.
    pub async fn capture(&self, size: Size, crop: Option<&Crop>, new_file: bool) -> Result<Value> {
        self.capture_expecting(None, size, crop, new_file).await
    }

    /// [`Client::capture`] only while the helper is still the one `verified`
    /// names, as for a crop of a process found in it.
    pub(crate) async fn capture_in(
        &self,
        verified: &Verified,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        self.capture_expecting(Some(&verified.session().epoch), size, crop, new_file)
            .await
    }

    /// [`Client::capture`], refused unless the helper is in `expected`.
    async fn capture_expecting(
        &self,
        expected: Option<&Epoch>,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        let started = Instant::now();
        let captured = self
            .capture_unrecorded(size, crop, new_file, expected)
            .await;

        // An agent's look at the screen is activity too, naming the file it
        // gave; a recording that fails never fails the capture.
        let request = json!({"op": "screenshot"});
        let reply = match &captured {
            Ok(saved) => json!({"ok": true, "capture": capture_name(saved)}),
            Err(error) => json!({"ok": false, "code": recorded_code(error)}),
        };
        let _ = self.record(&request, &reply, started.elapsed()).await;

        captured
    }

    /// [`Client::capture`], kept in the output folder but not in the log.
    async fn capture_unrecorded(
        &self,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
        expected: Option<&Epoch>,
    ) -> Result<Value> {
        let (frame, shows) = self.framed(size, crop, expected).await?;
        let output = self.output_dir()?;
        let kept = last_capture::keep(
            output,
            &last_capture::key(size, crop),
            &frame,
            shows.as_ref().map(|shows| &shows.region),
            new_file,
        )?;
        if kept.written {
            prune_captures(output);
        }

        let ((x, y), scale) = placement(
            &frame.display,
            frame.width,
            shows.as_ref().map(|shows| &shows.region),
        );
        let mut saved = json!({
            "ok": true,
            "path": kept.path,
            "width": frame.width,
            "height": frame.height,
            "display": frame.display,
            // Input coordinates are display points: origin + pixels × this.
            "origin": {"x": x, "y": y},
            "pointsPerPixel": scale,
        });
        if kept.unchanged {
            saved["unchanged"] = json!(true);
        }
        if let Some(shows) = shows {
            saved["region"] = json!(shows.region);
            for (key, value) in [("app", shows.app), ("overlapping", shows.overlapping)] {
                if !value.is_null() {
                    saved[key] = value;
                }
            }
        }

        Ok(saved)
    }
}
