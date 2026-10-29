//! What the person's side shows of the helper without acting there: the
//! apps open in it with their windows and icons, the screenshots agents
//! took, and the pictures made from them. `atlas mcp`'s view and the menu
//! bar's viewer service both read through here, so each shows the same
//! things under the same rules.
use crate::{
    error::{Error, Result},
    files::open_plain,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

/// The widest frame sent, and its JPEG quality; a smaller second try when
/// the first is too large for a tool result.
pub const FRAME: (u32, f64) = (1024, 0.6);
pub const FRAME_SMALL: (u32, f64) = (768, 0.45);
/// The most base64 characters a frame may take: well under the ~150,000
/// characters an agent app hydrates into a view.
pub const FRAME_BUDGET: usize = 120_000;
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
    /// An app's icon as a base64 PNG, made once. Only apps installed where
    /// apps are (not files any app could have put in a shared folder) are
    /// read on the person's side.
    pub async fn get(&self, bundle: PathBuf) -> Option<String> {
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
pub fn frame(png: &[u8]) -> Result<Value> {
    let mut encoded = jpeg(png, FRAME)?;
    if base64_length(encoded.0.len()) > FRAME_BUDGET {
        encoded = jpeg(png, FRAME_SMALL)?;
    }

    let (bytes, width, height) = encoded;
    if base64_length(bytes.len()) > FRAME_BUDGET {
        return Err(Error::new("capture", "The screen is too detailed to send"));
    }

    let digest = Sha256::digest(&bytes);
    let id = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    use base64::{Engine, engine::general_purpose::STANDARD};
    Ok(json!({
        "id": id,
        "mimeType": "image/jpeg",
        "data": STANDARD.encode(&bytes),
        "width": width,
        "height": height,
    }))
}

/// A screenshot's thumbnail: a small JPEG, as base64, with its size.
pub fn thumbnail(png: &[u8]) -> Result<Value> {
    let (bytes, width, height) = jpeg(png, THUMB)?;
    use base64::{Engine, engine::general_purpose::STANDARD};
    Ok(json!({
        "mimeType": "image/jpeg",
        "data": STANDARD.encode(&bytes),
        "width": width,
        "height": height,
    }))
}

/// The regular apps in `apps`, each with its windows from `windows` (front
/// to back): apps with a window first, in the order of their front one.
/// A window carries its number, title and bounds in desktop points.
pub fn open_apps(apps: &Value, windows: &Value) -> Result<Vec<Value>> {
    // Reserve the added field names and null defaults per entry before
    // copying anything. Each source entry can contribute to the output once.
    let count = apps.as_array().map_or(0, Vec::len) + windows.as_array().map_or(0, Vec::len);
    let too_large = || Error::new("apps", "The app list is too large to read");
    let overhead = count
        .checked_mul(128)
        .filter(|bytes| *bytes <= FRAME_LIMIT)
        .ok_or_else(too_large)?;
    let mut budget = ListBudget(FRAME_LIMIT - overhead);
    serde_json::to_writer(&mut budget, apps).map_err(|_| too_large())?;
    serde_json::to_writer(&mut budget, windows).map_err(|_| too_large())?;

    let mut pids = HashSet::new();
    for app in apps.as_array().into_iter().flatten() {
        if !pids.insert(
            app["pid"]
                .as_u64()
                .filter(|pid| *pid > 0)
                .ok_or_else(|| Error::new("apps", "The app list has an invalid process ID"))?,
        ) {
            return Err(Error::new("apps", "The app list repeats a process ID"));
        }
    }
    let windows = windows.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut grouped = HashMap::<u64, Vec<_>>::new();
    for (at, window) in windows.iter().enumerate() {
        if let Some(pid) = window["pid"].as_u64() {
            grouped.entry(pid).or_default().push((at, window));
        }
    }
    let mut listed = apps
        .as_array()
        .into_iter()
        .flatten()
        .filter(|app| app["policy"] == "regular")
        .map(|app| {
            let own = app["pid"]
                .as_u64()
                .and_then(|pid| grouped.remove(&pid))
                .unwrap_or_default();
            let front = own.first().map_or(usize::MAX, |(at, _)| *at);
            // The app the executable is in: the innermost, for a helper app
            // inside another.
            let bundle = app["executable"].as_str().and_then(|path| {
                path.rfind(".app/Contents/MacOS/")
                    .map(|end| &path[..end + 4])
            });
            let listed = json!({
                "name": app["name"],
                "pid": app["pid"],
                "active": app["active"] == true,
                "hidden": app["hidden"] == true,
                "bundle": bundle,
                "windows": own
                    .iter()
                    .map(|(_, window)| json!({
                        "id": window["number"],
                        "title": window["title"],
                        "x": window["bounds"]["x"],
                        "y": window["bounds"]["y"],
                        "width": window["bounds"]["width"],
                        "height": window["bounds"]["height"],
                    }))
                    .collect::<Vec<_>>(),
            });
            (
                front,
                app["name"].as_str().unwrap_or_default().to_lowercase(),
                listed,
            )
        })
        .collect::<Vec<_>>();

    listed.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(listed.into_iter().map(|(_, _, app)| app).collect())
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

/// The file name of a screenshot an agent's capture wrote: Atlas's own
/// timestamp name, never a path or a person's file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Capture(String);

impl Capture {
    /// `name` as a capture's, when [`capture_time`] reads it.
    pub fn parse(name: &str) -> Option<Self> {
        capture_time(name).map(|_| Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Capture {
    type Error = String;

    fn try_from(name: String) -> std::result::Result<Self, Self::Error> {
        Self::parse(&name).ok_or_else(|| format!("{name} is not a capture's name"))
    }
}

impl From<Capture> for String {
    fn from(capture: Capture) -> Self {
        capture.0
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
pub fn read_capture(output: &Path, name: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    let unknown = || Error::new("bad-request", "No screenshot has that name");
    if capture_time(name).is_none() {
        return Err(unknown());
    }

    let file = open_plain(&output.join(name)).ok_or_else(unknown)?;
    let mut png = Vec::new();
    file.take(FRAME_LIMIT as u64 + 1).read_to_end(&mut png)?;
    if png.len() > FRAME_LIMIT {
        return Err(Error::new("capture", "The screenshot is too large to read"));
    }

    Ok(png)
}

/// The size in pixels of the screenshot `name` in `output`, read from its
/// PNG header under [`read_capture`]'s rules.
pub fn capture_size(output: &Path, name: &str) -> Option<(u32, u32)> {
    use std::io::Read;

    capture_time(name)?;
    let file = open_plain(&output.join(name))?;
    let mut head = Vec::new();
    file.take(PNG_HEAD).read_to_end(&mut head).ok()?;
    crate::client::png_dimensions(&head).ok()
}

/// Whether `bundle` is an app installed where apps go: the system's and
/// the person's Applications folders, nowhere any app could write.
pub fn installed(bundle: &Path) -> bool {
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
    use base64::{Engine, engine::general_purpose::STANDARD};
    let png = crate::native::icons::png(bundle, ICON).ok()?;
    Some(STANDARD.encode(png))
}

#[cfg(not(target_os = "macos"))]
fn icon(_bundle: &Path) -> Option<String> {
    None
}

/// How many base64 characters `bytes` bytes take.
pub fn base64_length(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// `png` as a JPEG at most `limit` pixels on its longest side, at
/// `quality`, with its size.
#[cfg(target_os = "macos")]
pub fn jpeg(png: &[u8], (limit, quality): (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    crate::native::capture::jpeg(png, limit, quality)
}

#[cfg(not(target_os = "macos"))]
pub fn jpeg(_png: &[u8], _: (u32, f64)) -> Result<(Vec<u8>, u32, u32)> {
    Err(Error::new("platform", "Atlas runs on macOS only"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_a_jpeg_within_the_budget_named_by_its_content() {
        let png = crate::native::capture::sample_png(2560, 1440);
        let first = frame(&png).unwrap();
        assert_eq!(first["mimeType"], "image/jpeg");
        assert_eq!(
            (first["width"].clone(), first["height"].clone()),
            (json!(1024), json!(576))
        );
        assert!(first["data"].as_str().unwrap().len() <= FRAME_BUDGET);
        assert_eq!(first["id"].as_str().unwrap().len(), 16);
        assert_eq!(
            frame(&png).unwrap()["id"],
            first["id"],
            "the same screen, the same id"
        );
        assert_eq!(base64_length(4), 8);
    }

    #[test]
    fn apps_list_front_to_back_with_their_windows_and_bundles() {
        let apps = json!([
            {"name": "Finder", "pid": 1, "policy": "regular", "executable": "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"},
            {"name": "loginwindow", "pid": 2, "policy": "accessory", "executable": "/x"},
            {"name": "Opie", "pid": 3, "policy": "regular", "active": true, "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
            {"name": "Notes", "pid": 4, "policy": "regular", "executable": "/System/Applications/Notes.app/Contents/MacOS/Notes"},
        ]);
        let windows = json!([
            {"number": 11, "pid": 3, "title": "Starter", "bounds": {"x": 0.0, "y": 25.0, "width": 1080.0, "height": 590.0}},
            {"number": 12, "pid": 4, "title": "QA checklist", "bounds": {"x": 40.0, "y": 60.0, "width": 900.0, "height": 640.0}},
            {"number": 13, "pid": 3, "title": "Ring", "bounds": {"x": 700.0, "y": 80.0, "width": 520.0, "height": 860.0}},
        ]);

        let listed = open_apps(&apps, &windows).unwrap();
        let order = listed
            .iter()
            .map(|app| app["name"].clone())
            .collect::<Vec<_>>();
        assert_eq!(order, [json!("Opie"), json!("Notes"), json!("Finder")]);
        assert_eq!(listed[0]["active"], true);
        assert_eq!(listed[0]["bundle"], "/Applications/Opie.app");
        assert_eq!(
            listed[0]["windows"][1],
            json!({"id": 13, "title": "Ring", "x": 700.0, "y": 80.0, "width": 520.0, "height": 860.0})
        );
        assert_eq!(
            listed[2]["windows"],
            json!([]),
            "Finder, without a window, last"
        );
    }

    #[test]
    fn repeated_app_process_ids_are_refused_before_joining_windows() {
        let app = json!({"name": "Notes", "pid": 4, "policy": "regular"});
        let apps = json!([app, app]);
        let windows = json!([{"pid": 4, "number": 1, "title": "Notes"}]);
        assert_eq!(open_apps(&apps, &windows).unwrap_err().code, "apps");
    }

    #[test]
    fn app_lists_bound_total_entries_and_text_before_copying_them() {
        let apps = json!([{"name": "Notes", "pid": 4, "policy": "regular"}]);
        let windows = json!([
            {"pid": 4, "title": "x".repeat(FRAME_LIMIT / 2)},
            {"pid": 4, "title": "x".repeat(FRAME_LIMIT / 2)},
        ]);
        assert_eq!(open_apps(&apps, &windows).unwrap_err().code, "apps");
        let windows = Value::Array(vec![Value::Null; FRAME_LIMIT / 128 + 1]);
        assert_eq!(open_apps(&apps, &windows).unwrap_err().code, "apps");
        let apps = json!([{"pid": 4, "name": "x".repeat(FRAME_LIMIT), "policy": "regular"}]);
        assert_eq!(open_apps(&apps, &json!([])).unwrap_err().code, "apps");
    }

    #[test]
    fn only_files_an_agents_capture_wrote_are_screenshots() {
        assert_eq!(
            capture_time("1700000000000000000-capture.png"),
            Some(1_700_000_000_000_000_000)
        );
        for name in [
            "-capture.png",
            "../1-capture.png",
            "1-capture.png.partial",
            "a1-capture.png",
            "metadata.jsonl",
        ] {
            assert_eq!(capture_time(name), None, "{name}");
        }

        let dir = tempfile::tempdir().unwrap();
        let second = 1_000_000_000_u128;
        let named = |at: u128| format!("{}-capture.png", at * second);
        std::fs::write(dir.path().join(named(10)), b"x").unwrap();
        std::fs::write(dir.path().join(named(40)), b"x").unwrap();
        std::fs::write(dir.path().join("metadata.jsonl"), b"x").unwrap();
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join(named(50))).unwrap();

        let shots = captures(dir.path(), 30);
        assert_eq!(
            shots,
            [(40 * second, named(40)), (10 * second, named(10))],
            "no links, no other files, newest first"
        );
        assert_eq!(captures(dir.path(), 1).len(), 1);
    }

    #[test]
    fn a_screenshot_name_outside_the_capture_pattern_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let png = crate::native::capture::sample_png(64, 40);
        std::fs::write(dir.path().join("5-capture.png"), &png).unwrap();
        std::fs::write(dir.path().join("u.toml"), b"secret").unwrap();
        std::fs::write(dir.path().join("5-capture.png.partial"), &png).unwrap();

        assert_eq!(read_capture(dir.path(), "5-capture.png").unwrap(), png);
        assert_eq!(capture_size(dir.path(), "5-capture.png"), Some((64, 40)));
        for name in [
            "u.toml",
            "../u.toml",
            "5-capture.png.partial",
            "/etc/hosts",
            "x5-capture.png",
            "6-capture.png",
        ] {
            assert_eq!(
                read_capture(dir.path(), name).unwrap_err().code,
                "bad-request",
                "{name}"
            );
            assert_eq!(capture_size(dir.path(), name), None, "{name}");
        }
    }

    #[test]
    fn a_linked_screenshot_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let png = crate::native::capture::sample_png(64, 40);
        std::fs::write(dir.path().join("5-capture.png"), &png).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("5-capture.png"),
            dir.path().join("6-capture.png"),
        )
        .unwrap();

        assert_eq!(
            read_capture(dir.path(), "6-capture.png").unwrap_err().code,
            "bad-request"
        );
        assert_eq!(capture_size(dir.path(), "6-capture.png"), None);
    }

    #[test]
    fn a_screenshot_that_is_not_a_plain_file_is_refused_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("7-capture.png")).unwrap();
        nix::unistd::mkfifo(
            &dir.path().join("8-capture.png"),
            nix::sys::stat::Mode::S_IRWXU,
        )
        .unwrap();

        for name in ["7-capture.png", "8-capture.png"] {
            assert_eq!(
                read_capture(dir.path(), name).unwrap_err().code,
                "bad-request",
                "{name}"
            );
        }
        assert!(captures(dir.path(), 30).is_empty());
    }

    #[test]
    fn an_icon_is_drawn_at_the_size_asked() {
        let icon =
            crate::native::capture::icon_png(&crate::native::capture::sample_png(256, 256), 64)
                .unwrap();
        assert_eq!(crate::client::png_dimensions(&icon).unwrap(), (64, 64));
    }

    #[test]
    fn icons_come_only_from_apps_installed_where_apps_go() {
        assert!(installed(Path::new("/Applications/Opie.app")));
        assert!(installed(Path::new("/System/Applications/Notes.app")));
        assert!(installed(Path::new(
            "/System/Library/CoreServices/Finder.app"
        )));
        assert!(!installed(Path::new("/System/Library/Evil.app")));
        assert!(!installed(Path::new("/Users/Shared/Evil.app")));
        assert!(!installed(Path::new("/tmp/Evil.app")));
        assert!(!installed(Path::new("/Applications/../tmp/Evil.app")));
        assert!(!installed(Path::new("Applications/Opie.app")));

        let nested = json!([{"name": "Helper", "pid": 1, "policy": "regular",
            "executable": "/Applications/Big.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"}]);
        assert_eq!(
            open_apps(&nested, &json!([])).unwrap()[0]["bundle"],
            "/Applications/Big.app/Contents/Frameworks/Helper.app"
        );
    }
}
