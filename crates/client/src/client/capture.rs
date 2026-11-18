//! Screenshots of the helper's display, or a VM's: a frame read and checked
//! against the session it came from, a crop checked against the one asked
//! for, and the saved capture that says where its pixels lie on the
//! display. Captures are kept for a week.
//!
//! A VM's picture is taken between two proofs of it, each with its window
//! closed, that show the same epoch, the same generation of its keeper's
//! session and Atlas VM's same picture fence: no opening of the window came
//! between them, so nothing the person did there is in it.
use super::{Client, Verified, record::recorded_code};
use crate::last_capture;
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Result},
    protocol::{APP_PARTS, Crop, Part, Region},
    screen::{
        frame::{Frame, Size},
        png::png_dimensions,
    },
    session::{Display, Epoch},
};
use atlas_session::compatibility::PICTURES;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

/// The longest side, in pixels, of a picture a VM's guest sends.
pub(super) const VM_SIDE: u32 = 1600;

/// Who sent a screenshot, as what is refused of it names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sender {
    Helper,
    Vm,
}

impl Sender {
    fn name(self) -> &'static str {
        match self {
            Self::Helper => "The helper",
            Self::Vm => "The VM",
        }
    }
}

/// What a screenshot is taken on: a proof made for it, refused unless still
/// in the epoch named, when one is; or the proof its target was found in.
#[derive(Clone, Copy, Debug)]
enum Proof<'a> {
    Fresh(Option<&'a Epoch>),
    Looked(&'a Verified),
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
    cropped_from(reply, crop, display, Sender::Helper)
}

/// [`cropped`], of a reply `sender` sent.
fn cropped_from(
    reply: &Value,
    crop: Option<&Crop>,
    display: &Display,
    sender: Sender,
) -> Result<Option<Cropped>> {
    let malformed =
        |what: &str| Error::new(Code::Protocol, format!("{} sent {what}", sender.name()));
    let sent = reply.get("region").filter(|region| !region.is_null());
    let (crop, sent) = match (crop, sent) {
        (None, None) => return Ok(None),
        (None, Some(_)) => return Err(malformed("a crop that was not asked for")),
        (Some(_), None) if sender == Sender::Vm => {
            return Err(malformed("the whole display for a crop"));
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

    let region: Region = serde_json::from_value(sent.clone())
        .map_err(|cause| malformed("a crop that could not be read").with_source(cause))?;
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

    let (app, overlapping) = match crop {
        Crop::Region(asked) => {
            let inside = left >= i64::from(asked.x)
                && top >= i64::from(asked.y)
                && right <= i64::from(asked.x) + i64::from(asked.width)
                && bottom <= i64::from(asked.y) + i64::from(asked.height);
            if !inside {
                return Err(malformed("a crop other than the region asked for"));
            }
            (Value::Null, Value::Null)
        }
        Crop::App { pid } | Crop::Part { pid, .. } => {
            if reply["app"]["pid"].as_i64() != Some(*pid) || !reply["overlapping"].is_array() {
                return Err(malformed("a crop of another app"));
            }
            if let Crop::Part {
                part: Part::Within(within),
                ..
            } = crop
            {
                let inside = left >= i64::from(within.x)
                    && top >= i64::from(within.y)
                    && right <= i64::from(within.x) + i64::from(within.width)
                    && bottom <= i64::from(within.y) + i64::from(within.height);
                if !inside {
                    return Err(malformed("a crop outside the region asked for"));
                }
            }
            (reply["app"].clone(), reply["overlapping"].clone())
        }
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

/// The PNG a screenshot's `reply` carries, and its width and height, which
/// its own header and the reply must agree on.
fn png_of(reply: &Value) -> Result<(Vec<u8>, u32, u32)> {
    let encoded = reply["png"]
        .as_str()
        .ok_or_else(|| Error::new(Code::Capture, "Missing PNG"))?;
    let png = STANDARD
        .decode(encoded)
        .map_err(|cause| Error::new(Code::Capture, "Invalid PNG encoding").with_source(cause))?;
    let (width, height) = png_dimensions(&png)?;
    if reply["width"].as_u64() != Some(width.into())
        || reply["height"].as_u64() != Some(height.into())
    {
        return Err(Error::new(
            Code::Capture,
            "PNG dimensions disagree with metadata",
        ));
    }
    Ok((png, width, height))
}

/// Whether a picture of `width` × `height` pixels has the shape of
/// `display`, to within a pixel: each of its pixels spans as many points
/// across as down, so a point read off it lands where it shows.
fn shaped_as(display: &Display, width: u32, height: u32) -> bool {
    let expected = f64::from(width) * display.height / display.width;
    (f64::from(height) - expected).abs() <= 1.0
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

impl Client {
    /// # Errors
    ///
    /// Returns the failure to verify the helper or to capture its screen.
    pub async fn frame(&self, maximum: Option<u32>) -> Result<Frame> {
        let size = maximum.map_or(Size::Full, Size::Longest);

        self.framed(size, None, Proof::Fresh(None))
            .await
            .map(|(frame, _)| frame)
    }

    /// A frame of the whole display, or of the crop asked for, with what the
    /// desktop says the crop shows, taken on `proof`. When it names an
    /// epoch, the desktop must still be in it when the capture starts, as
    /// when the crop names a process found in it.
    async fn framed(
        &self,
        size: Size,
        crop: Option<&Crop>,
        proof: Proof<'_>,
    ) -> Result<(Frame, Option<Cropped>)> {
        let maximum = size.longest();
        if maximum.is_some_and(|m| m == 0 || m > 8192) {
            return Err(Error::new(Code::Input, "Maximum dimension must be 1–8192"));
        }
        if matches!(self.desktop().kind(), Kind::Vm(_)) {
            let fresh;
            let verified = match proof {
                Proof::Looked(verified) => verified,
                Proof::Fresh(expected) => {
                    fresh = self.verify_vm_looking().await?;
                    if expected.is_some_and(|epoch| *epoch != fresh.session().epoch) {
                        return Err(self.vm_changed());
                    }
                    &fresh
                }
            };
            return self.framed_on_vm(verified, size, crop).await;
        }
        let expected = match proof {
            Proof::Fresh(expected) => expected,
            Proof::Looked(verified) => Some(&verified.session().epoch),
        };

        let verified = self.verify().await?;
        let before = verified.session();
        if expected.is_some_and(|epoch| *epoch != before.epoch) {
            return Err(Error::session_changed());
        }
        let mut request = screenshot_request(size);

        match crop {
            None => {}
            Some(Crop::App { pid }) => request["pid"] = json!(pid),
            Some(Crop::Part { pid, part }) => {
                if !verified.observation.has_capability(APP_PARTS) {
                    return Err(Error::new(
                        Code::Daemon,
                        "The helper is running an older Atlas that cannot capture one window or \
                         an app's part of a region; run atlas restart, then try again",
                    ));
                }
                request["pid"] = json!(pid);
                match part {
                    Part::Window(number) => request["window"] = json!(number),
                    Part::Within(within) => request["within"] = json!(within),
                }
            }
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

        self.look(&verified, async {
            let reply = self.link.call(&request).await?;
            if reply["ok"] != true {
                if reply["code"].is_string() {
                    return Err(atlas_core::transport::refusal(&reply));
                }

                return Err(Error::new(
                    Code::Capture,
                    reply["error"].as_str().unwrap_or("Capture failed"),
                ));
            }
            let shows = cropped(&reply, crop, &before.epoch.display)?;

            let at = Instant::now();
            let (png, width, height) = png_of(&reply)?;

            let after = self.identity().await?;
            if before.epoch != after.epoch {
                return Err(Error::session_changed());
            }

            Ok((
                Frame {
                    png: png.into(),
                    width,
                    height,
                    epoch: after.epoch,
                    at,
                },
                shows,
            ))
        })
        .await
    }

    /// A frame of a VM's whole display, or of the crop asked for, taken on
    /// the proof `verified` and kept only once a proof after it shows the
    /// same epoch, generation and picture fence: at most [`VM_SIDE`] pixels
    /// on its longer side, its crop of the guest's own pictures' contract.
    async fn framed_on_vm(
        &self,
        verified: &Verified,
        size: Size,
        crop: Option<&Crop>,
    ) -> Result<(Frame, Option<Cropped>)> {
        let pinned = verified.machine().ok_or_else(|| self.unreachable())?;
        let generation = self.generation_offering(verified, PICTURES, |name| {
            format!("{name}'s Atlas is from an older build that gives this Mac no pictures.")
        })?;
        self.fenced(pinned)?;
        let before = verified.session();
        let display = &before.epoch.display;
        let mut request = screenshot_request(size);
        request["maxDimension"] = json!(size.longest().map_or(VM_SIDE, |side| side.min(VM_SIDE)));
        match crop {
            None => {}
            Some(Crop::App { pid }) => request["pid"] = json!(pid),
            Some(Crop::Part { pid, part }) => {
                request["pid"] = json!(pid);
                match part {
                    Part::Window(number) => request["window"] = json!(number),
                    Part::Within(within) => request["within"] = json!(within),
                }
            }
            Some(Crop::Region(region)) => {
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
        request["session"] = json!(verified.pin()?);

        // Which windows it saw is remembered, as a helper's capture
        // remembers them, read in its generation on either side of it.
        self.look(verified, async {
            let reply = self
                .link
                .send_bulk(&request, generation)
                .await?
                .finish()
                .await?;
            let reply = crate::vm::guest_words::reply_said_of(reply, &self.selected.name);
            if reply["ok"] != true {
                if reply["code"].is_string() {
                    return Err(atlas_core::transport::refusal(&reply));
                }
                return Err(Error::new(
                    Code::Capture,
                    reply["error"].as_str().unwrap_or("Capture failed"),
                ));
            }
            let shows = cropped_from(&reply, crop, display, Sender::Vm)?;
            let at = Instant::now();
            let (png, width, height) = png_of(&reply)?;
            if width > VM_SIDE || height > VM_SIDE {
                return Err(Error::new(
                    Code::Protocol,
                    "The VM sent a picture larger than was asked for",
                ));
            }
            if shows.is_none() && !shaped_as(display, width, height) {
                return Err(Error::new(
                    Code::Protocol,
                    "The VM sent a picture of another shape than its display",
                ));
            }
            self.reproved(pinned).await?;
            Ok((
                Frame {
                    png: png.into(),
                    width,
                    height,
                    epoch: before.epoch.clone(),
                    at,
                },
                shows,
            ))
        })
        .await
    }

    /// Save a screenshot of the whole display, or of `crop`, and say where
    /// its pixels lie on the display. A capture with the same pixels as the
    /// last one of that target and size names the earlier file and says it
    /// is `unchanged`, unless `new_file` asks for a file anyway.
    ///
    /// # Errors
    ///
    /// Returns identity, crop, transport, capture or private-output failures. A failed read
    /// never establishes a successful screenshot.
    pub async fn capture(&self, size: Size, crop: Option<&Crop>, new_file: bool) -> Result<Value> {
        self.capture_expecting(None, size, crop, new_file).await
    }

    /// [`Client::capture`], refused unless the helper is in `expected`, as
    /// for a crop of a process found in the helper a verification named.
    pub(crate) async fn capture_expecting(
        &self,
        expected: Option<&Epoch>,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        self.capture_on(Proof::Fresh(expected), size, crop, new_file)
            .await
    }

    /// [`Client::capture`] of a VM, taken on `looked`, the proof its crop's
    /// target was found in, and kept only once a proof after it shows the
    /// same.
    pub(crate) async fn capture_looked(
        &self,
        looked: &Verified,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        self.capture_on(Proof::Looked(looked), size, crop, new_file)
            .await
    }

    /// [`Client::capture`], taken on `proof`, and recorded.
    async fn capture_on(
        &self,
        proof: Proof<'_>,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        let started = Instant::now();
        let captured = self.capture_unrecorded(proof, size, crop, new_file).await;

        // An agent's look at the screen is activity too, naming the file it
        // gave; a recording that fails never fails the capture.
        let request = json!({"op": "screenshot"});
        let reply = match &captured {
            Ok(saved) => json!({"ok": true, "capture": capture_name(saved), "app": saved["app"]}),
            Err(error) => json!({"ok": false, "code": recorded_code(error)}),
        };
        let _ = self.record(&request, &reply, started.elapsed()).await;

        captured
    }

    /// [`Client::capture`], kept in the output folder but not in the log.
    /// An agent's is refused while the person signs the helper in to an
    /// account, and never kept when a sign-in began or ended while it ran.
    async fn capture_unrecorded(
        &self,
        proof: Proof<'_>,
        size: Size,
        crop: Option<&Crop>,
        new_file: bool,
    ) -> Result<Value> {
        let admitted = match self.actor {
            super::Actor::Agent => Some(self.controller.screen_visible()?),
            super::Actor::Viewer => None,
        };
        let (frame, shows) = self.framed(size, crop, proof).await?;
        if let Some(admitted) = admitted {
            self.controller.still_visible(admitted)?;
        }
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
            &frame.epoch.display,
            frame.width,
            shows.as_ref().map(|shows| &shows.region),
        );
        let mut saved = json!({
            "ok": true,
            "path": kept.path,
            "width": frame.width,
            "height": frame.height,
            "display": frame.epoch.display,
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

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
