//! What the person's side shows of the helper without acting there: the
//! apps open in it with their windows and icons, the screenshots agents
//! took, and the pictures made from them. `atlas mcp`'s view and the menu
//! bar's viewer service both read through here, so each shows the same
//! things under the same rules.
use crate::client::{AppList, AppRow};
use atlas_core::{
    error::{Code, Error, Result},
    files::open_plain,
    name::name_type,
    screen::windows::Window,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

/// The widest frame sent, and its JPEG quality; a smaller second try when
/// the first is too large for a tool result.
pub(crate) const FRAME: (u32, f64) = (1024, 0.6);
pub(crate) const FRAME_SMALL: (u32, f64) = (768, 0.45);
/// The most base64 characters a frame may take: well under the ~150,000
/// characters an agent app hydrates into a view.
pub(crate) const FRAME_BUDGET: usize = 120_000;
/// The largest screen or screenshot read, in bytes: a 5K display's PNG
/// takes well under it.
pub const FRAME_LIMIT: usize = 16 << 20;
/// A screenshot's thumbnail: its widest side and JPEG quality.
pub const THUMB: (u32, f64) = (480, 0.6);
/// App icons' size in pixels, the most sent in one answer, and the most
/// kept made.
const ICON: usize = 64;
pub const ICONS: usize = 12;
pub const ICON_CACHE: usize = 64;
/// The bytes of a PNG that say its size: the signature and the IHDR chunk.
const PNG_HEAD: u64 = 33;

/// App icons made, as base64 PNGs, by bundle path; None when an app has
/// none to read.
#[derive(Default)]
pub struct Icons(Mutex<HashMap<PathBuf, Option<String>>>);

impl Icons {
    /// Add its icon to each of the first `limit` of `apps` that has one.
    pub async fn add(&self, apps: &mut [Value], limit: usize) {
        for app in apps.iter_mut().take(limit) {
            if let Some(bundle) = app["bundle"].as_str().map(PathBuf::from)
                && let Some(icon) = self.get(bundle).await
            {
                app["icon"] = json!(icon);
            }
        }
    }

    /// An app's icon as a base64 PNG, made once. Only apps installed where
    /// apps are (not files any app could have put in a shared folder) are
    /// read on the person's side.
    pub(crate) async fn get(&self, bundle: PathBuf) -> Option<String> {
        if !installed(&bundle) {
            return None;
        }
        if let Some(known) = self.0.lock().await.get(&bundle) {
            return known.clone();
        }

        let made = tokio::task::spawn_blocking({
            let bundle = bundle.clone();
            move || icon(&bundle)
        })
        .await
        .ok()
        .flatten();
        let mut icons = self.0.lock().await;
        if icons.len() >= ICON_CACHE {
            icons.clear();
        }
        icons.insert(bundle, made.clone());
        made
    }
}

/// A PNG screen as the frame the view shows: a JPEG within the budget, as
/// base64, with its size and an id that changes with its content.
///
/// # Errors
///
/// Returns an error when the PNG can't be read or no JPEG fits the budget.
pub fn frame(png: &[u8]) -> Result<Value> {
    framed(|size| jpeg(png, size))
}

/// Packed 8-bit RGB pixels, `width`×`height` with rows `stride` bytes
/// apart, as the frame the view shows, the way [`frame`] makes one.
///
/// # Errors
///
/// Returns an error when the pixels can't be read or no JPEG fits the
/// budget.
pub fn frame_rgb(rgb: &[u8], pixels: (usize, usize, usize)) -> Result<Value> {
    framed(|size| jpeg_rgb(rgb, pixels, size))
}

/// The frame `encode` makes at the first size whose JPEG fits the budget.
fn framed(encode: impl Fn((u32, f64)) -> Result<(Vec<u8>, u32, u32)>) -> Result<Value> {
    let mut encoded = encode(FRAME)?;
    if base64_length(encoded.0.len()) > FRAME_BUDGET {
        encoded = encode(FRAME_SMALL)?;
    }

    if base64_length(encoded.0.len()) > FRAME_BUDGET {
        return Err(Error::new(
            Code::Capture,
            "The screen is too detailed to send",
        ));
    }

    let mut shown = picture(&encoded);
    shown["id"] = json!(atlas_core::name::hex(&Sha256::digest(&encoded.0)[..8]));
    Ok(shown)
}

/// A screenshot's thumbnail: a small JPEG, as base64, with its size.
///
/// # Errors
///
/// Returns an error when the PNG can't be read.
pub fn thumbnail(png: &[u8]) -> Result<Value> {
    Ok(picture(&jpeg(png, THUMB)?))
}

/// A JPEG and its size as a view takes it.
fn picture((bytes, width, height): &(Vec<u8>, u32, u32)) -> Value {
    json!({
        "mimeType": "image/jpeg",
        "data": STANDARD.encode(bytes),
        "width": width,
        "height": height,
    })
}

/// The regular apps in `apps`, each with its bundle identifier, when it was
/// launched and its windows from `windows` (front to back): apps with a
/// window first, in the order of their front one. A window carries its
/// number, title, bounds in desktop points and its place among every
/// app's windows, front to back.
///
/// # Errors
///
/// Returns an error when the list is too large or repeats a process.
pub fn open_apps(apps: &AppList, windows: &[Window]) -> Result<Vec<Value>> {
    let rows = apps.rows();
    // Reserve the added field names and null defaults per entry before
    // copying anything. Each source entry can contribute to the output once.
    let too_large = || Error::new(Code::Apps, "The app list is too large to read");
    let overhead = (rows.len() + windows.len())
        .checked_mul(128)
        .filter(|bytes| *bytes <= FRAME_LIMIT)
        .ok_or_else(too_large)?;
    let mut budget = ListBudget(FRAME_LIMIT - overhead);
    let fields: Vec<_> = rows.iter().map(AppRow::fields).collect();
    serde_json::to_writer(&mut budget, &fields).map_err(|_| too_large())?;
    serde_json::to_writer(&mut budget, windows).map_err(|_| too_large())?;

    let mut pids = HashSet::new();
    if !rows.iter().all(|app| pids.insert(app.pid())) {
        return Err(Error::new(Code::Apps, "The app list repeats a process ID"));
    }
    let mut grouped = HashMap::<i64, Vec<_>>::new();
    for (at, window) in windows.iter().enumerate() {
        grouped.entry(window.pid).or_default().push((at, window));
    }
    let mut listed = rows
        .iter()
        .filter(|app| app.policy() == Some("regular"))
        .map(|app| {
            let own = i64::try_from(app.pid())
                .ok()
                .and_then(|pid| grouped.remove(&pid))
                .unwrap_or_default();
            let front = own.first().map_or(usize::MAX, |(at, _)| *at);
            let listed = json!({
                "name": app.name(),
                "pid": app.pid(),
                "active": app.active(),
                "hidden": app.hidden(),
                "bundle": app.executable().and_then(bundle_of),
                "bundleId": app.bundle_id().and_then(crate::doing::bundle_id),
                "launchedAt": app.launched_at(),
                "windows": own
                    .iter()
                    .map(|(at, window)| json!({
                        "id": window.number,
                        "title": window.title,
                        "x": window.bounds.x,
                        "y": window.bounds.y,
                        "width": window.bounds.width,
                        "height": window.bounds.height,
                        "order": at,
                    }))
                    .collect::<Vec<_>>(),
            });
            (front, app.name().unwrap_or_default().to_lowercase(), listed)
        })
        .collect::<Vec<_>>();

    listed.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(listed.into_iter().map(|(_, _, app)| app).collect())
}

/// The app bundle the executable at `path` is in: the innermost, for a
/// helper app inside another.
pub fn bundle_of(path: &str) -> Option<&str> {
    path.rfind(".app/Contents/MacOS/")
        .map(|end| &path[..end + 4])
}

/// Count serialized source bytes without allocating another copy of them.
struct ListBudget(usize);

impl std::io::Write for ListBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("The app list is too large to read"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// When a screenshot named `name` was taken, in nanoseconds since 1970:
/// only names an agent's capture writes have one.
pub fn capture_time(name: &str) -> Option<u128> {
    let digits = name.strip_suffix("-capture.png")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

name_type! {
    /// The file name of a screenshot an agent's capture wrote: Atlas's own
    /// timestamp name, never a path or a person's file.
    #[derive(PartialOrd, Ord)]
    pub struct Capture;
    refused(name) = format!("{name} is not a capture's name");
}

impl Capture {
    /// `name` as a capture's, when [`capture_time`] reads it.
    pub fn parse(name: &str) -> Option<Self> {
        capture_time(name).map(|_| Self(name.to_owned()))
    }
}

/// The agents' screenshots in `output`, newest first, at most `limit`:
/// each one's name and when it was taken, in nanoseconds since 1970. Only
/// plain files a capture names count; links and other files never do.
pub fn captures(output: &Path, limit: usize) -> Vec<(u128, String)> {
    let Ok(entries) = std::fs::read_dir(output) else {
        return Vec::new();
    };

    let mut shots = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            Some((capture_time(&name)?, name))
        })
        .collect::<Vec<_>>();
    shots.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    shots.truncate(limit);
    shots
}

/// The screenshot `name` in `output`, whole. Only a name a capture writes,
/// and only a plain file the agent's capture wrote: never through a link,
/// and no more of it than a screenshot takes.
///
/// # Errors
///
/// Returns an error for a name no capture writes, or a file that is missing,
/// too large or not plain.
pub fn read_capture(output: &Path, name: &str) -> Result<Vec<u8>> {
    let unknown = || Error::new(Code::BadRequest, "No screenshot has that name");
    if capture_time(name).is_none() {
        return Err(unknown());
    }

    let file = open_plain(&output.join(name)).ok_or_else(unknown)?;
    let png = atlas_core::files::read_up_to(file, FRAME_LIMIT as u64 + 1)?;
    if png.len() > FRAME_LIMIT {
        return Err(Error::new(
            Code::Capture,
            "The screenshot is too large to read",
        ));
    }

    Ok(png)
}

/// The size in pixels of the screenshot `name` in `output`, read from its
/// PNG header under [`read_capture`]'s rules.
pub fn capture_size(output: &Path, name: &str) -> Option<(u32, u32)> {
    capture_time(name)?;
    let file = open_plain(&output.join(name))?;
    let head = atlas_core::files::read_up_to(file, PNG_HEAD).ok()?;
    atlas_core::screen::png::png_dimensions(&head).ok()
}

/// Whether `bundle` is an app installed where apps go: the system's and
/// the person's Applications folders, nowhere any app could write.
pub(crate) fn installed(bundle: &Path) -> bool {
    let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"));
    let roots = [
        Some(PathBuf::from("/Applications")),
        Some(PathBuf::from("/System/Applications")),
        // Finder and the system's other own apps: sealed by macOS, so no app
        // could have put anything there.
        Some(PathBuf::from("/System/Library/CoreServices")),
        home,
    ];
    bundle.is_absolute()
        && !bundle
            .components()
            .any(|part| part == std::path::Component::ParentDir)
        && roots.iter().flatten().any(|root| bundle.starts_with(root))
}

/// `bundle`'s icon as a base64 PNG. Call [`installed`] first.
#[cfg(target_os = "macos")]
fn icon(bundle: &Path) -> Option<String> {
    let png = atlas_native::native::icons::png(bundle, ICON).ok()?;
    Some(STANDARD.encode(png))
}

#[cfg(not(target_os = "macos"))]
fn icon(_bundle: &Path) -> Option<String> {
    None
}

/// How many base64 characters `bytes` bytes take.
pub(crate) fn base64_length(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// `png` as a JPEG at most `limit` pixels on its longest side, at
/// `quality`, with its size.
///
/// # Errors
///
/// Returns an error when the PNG can't be read.
#[cfg(target_os = "macos")]
pub fn jpeg(png: &[u8], (limit, quality): (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    atlas_native::native::capture::jpeg(png, limit, quality)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn jpeg(_png: &[u8], _: (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    Err(Error::new(Code::Platform, "Atlas runs on macOS only"))
}

#[cfg(target_os = "macos")]
fn jpeg_rgb(
    rgb: &[u8],
    pixels: (usize, usize, usize),
    (limit, quality): (u32, f64),
) -> Result<(Vec<u8>, u32, u32)> {
    atlas_native::native::capture::jpeg_rgb(rgb, pixels, limit, quality)
}

#[cfg(not(target_os = "macos"))]
fn jpeg_rgb(_: &[u8], _: (usize, usize, usize), _: (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    Err(Error::new(Code::Platform, "Atlas runs on macOS only"))
}

#[cfg(test)]
mod tests;
