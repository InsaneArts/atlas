//! The controller's side of the helper: one [`Client`] per configuration
//! that talks to the helper daemon and the input service over their
//! sockets, proves the helper is the hidden session it expects before every
//! action, holds controller exclusion while acting, and keeps the
//! screenshots and activity log in the private output folder.
use crate::{
    actions::{self, Expect},
    config::Config,
    control::{Beside, Controller, Ownership, SharedLease},
    daemon::protocol::{Crop, Region, SESSION_PIN, SessionPin},
    digest::file_sha256,
    doing::Doing,
    error::{Error, Holder, Result},
    host_session::HostSession,
    last_capture,
    session::{
        Display, Epoch, FOCUS_APP, FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW, HOLD, SHUTDOWN, Session,
        has_capability, input_current, input_settled, require_hidden,
    },
    task::Task,
    transport::{Transport, accepted, refusal, run_output},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use fs2::FileExt;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
struct Cropped {
    region: Region,
    /// The app shown, for an app's crop.
    app: Value,
    /// Other apps' windows in front of it, for an app's crop.
    overlapping: Value,
}

/// Whether `region` and the display share any area.
fn meets(region: &Region, display: &Display) -> bool {
    let (x, y) = (f64::from(region.x), f64::from(region.y));
    x < display.width
        && y < display.height
        && x + f64::from(region.width) > 0.0
        && y + f64::from(region.height) > 0.0
}

/// The crop a screenshot reply says it shows, checked against the one asked
/// for: an older helper that ignores a crop sends the whole display, which
/// must never be read as the crop.
fn cropped(reply: &Value, crop: Option<&Crop>, display: &Display) -> Result<Option<Cropped>> {
    let sent = reply.get("region").filter(|region| !region.is_null());
    let (crop, sent) = match (crop, sent) {
        (None, None) => return Ok(None),
        (None, Some(_)) => {
            return Err(Error::new(
                "protocol",
                "The helper sent a crop that was not asked for",
            ));
        }
        (Some(_), None) => {
            return Err(Error::new(
                "daemon",
                "The helper is running an older Atlas that cannot crop screenshots; run atlas \
                 restart, then try again",
            ));
        }
        (Some(crop), Some(sent)) => (crop, sent),
    };

    let malformed = |what: &str| Error::new("protocol", format!("The helper sent {what}"));
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
fn placement(display: &Display, width: u32, region: Option<&Region>) -> ((f64, f64), f64) {
    match region {
        Some(region) => (
            (f64::from(region.x), f64::from(region.y)),
            f64::from(region.width) / f64::from(width),
        ),
        None => ((0.0, 0.0), display.width / f64::from(width)),
    }
}

/// Identity (device, inode, size, mtime) of the installed executable and its hash.
type BuildCache = Arc<std::sync::Mutex<Option<((u64, u64, u64, i64, i64), String)>>>;

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
fn screenshot_request(size: Size) -> Value {
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

/// How long `run` lets a command take unless told otherwise.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a fixed, read-only inspection may take unless told otherwise.
pub(crate) const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest a command may run (the helper's own limit).
pub const RUN_TIMEOUT_MAX: Duration = Duration::from_secs(600);
/// Time beyond a command's timeout for the helper to stop what it left
/// running and send its last output.
const RUN_MARGIN: Duration = Duration::from_secs(10);
/// The longest the input service's grant can outlast a command killed
/// while holding it: the grant's own life, and a second to spare.
const GRANT_OUTLIVES: Duration = crate::native::GRANT_TTL.saturating_add(Duration::from_secs(1));

/// A helper session a command has just verified, and whether its daemon
/// refuses a read meant for another session. Only [`Client::verify`] makes
/// one, so a read given it was checked in that session.
#[derive(Debug, Clone)]
pub struct Verified {
    session: Session,
    pinned: bool,
}

impl Verified {
    /// The session as it was verified.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// What a read carries to be served only in this session; none when the
    /// daemon cannot be told, and the session is verified again instead.
    pub(crate) fn pin(&self) -> Option<SessionPin> {
        self.pinned.then_some(SessionPin {
            security_session_id: self.session.epoch.security_session_id,
            display_id: self.session.epoch.display_id,
        })
    }
}

/// How an agent's batch is checked against the app it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// Sent only with the check: an input service that cannot check is
    /// refused before anything is sent.
    Required(Expect),
    /// Checked when the input service can, and otherwise sent as before:
    /// for input whose own check already ran before it was sent.
    Offered(Expect),
}

/// Who recorded mutations are attributed to in the activity log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Agent,
    Viewer,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Agent => "agent",
            Actor::Viewer => "viewer",
        }
    }
}

#[derive(Clone)]
pub struct Client {
    pub config: Config,
    pub transport: Transport,
    pub controller: Controller,
    pub actor: Actor,
    /// What the command acting through this client does, for the log.
    pub doing: Option<Doing>,
    /// The host session the command acting through this client comes from,
    /// for the activity log and the session record; never for a decision.
    pub host_session: Option<HostSession>,
    /// The task of that session the command belongs to, for its rows and
    /// the task's record; never for a decision.
    pub task: Option<Task>,
    installed: BuildCache,
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

/// The folder holding the shared command: made (or repaired) so the helper
/// can pass through it, whatever this terminal's umask. Only a folder of
/// this account is changed; anyone else's is left for the checks to refuse.
fn shared_folder(folder: &Path) -> Result<()> {
    match fs::symlink_metadata(folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(folder)?;
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }

    let metadata = fs::symlink_metadata(folder)?;
    let me = nix::unistd::getuid().as_raw();
    // Any account can make folders in /Users/Shared: one another account
    // made, or that others can change, is never used.
    let foreign = metadata.uid() != me && metadata.uid() != 0;
    let open = metadata.uid() == 0 && metadata.mode() & 0o022 != 0;
    if !metadata.file_type().is_dir() || foreign || open {
        return Err(Error::new(
            "install",
            format!(
                "{} belongs to another account, or others can change it. Atlas is set up for \
                 one person per Mac; if no one else uses it, remove the folder (an \
                 administrator can) and try again.",
                folder.display()
            ),
        ));
    }

    if metadata.uid() == me && metadata.mode() & 0o777 != 0o755 {
        fs::set_permissions(folder, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl Client {
    pub fn install(&self) -> Result<Value> {
        self.install_probe(false)
    }

    /// Repair the probe under recovery exclusion without settling previous work.
    pub(crate) fn bootstrap_probe(&self) -> Result<Value> {
        self.install_probe(true)
    }

    fn install_probe(&self, recovery: bool) -> Result<Value> {
        let source = fs::canonicalize(std::env::current_exe()?)?;
        let target = &self.config.identity_probe;

        let shared = target
            .parent()
            .filter(|parent| Path::new(crate::handoff::ROOT).parent() == Some(*parent));
        let folder_ready = shared.is_none_or(|parent| {
            fs::symlink_metadata(parent).is_ok_and(|metadata| {
                metadata.is_dir()
                    && (metadata.uid() == nix::unistd::getuid().as_raw()
                        && metadata.mode() & 0o777 == 0o755
                        || metadata.uid() == 0 && metadata.mode() & 0o022 == 0)
            })
        });
        // An unchanged link and folder need no mutation or exclusion.
        // A copy is always replaced, using a fresh inode.
        if folder_ready && fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let _ownership = self.controller.acquire(recovery)?;
        if let Some(parent) = shared {
            shared_folder(parent)?;
        }
        if fs::canonicalize(target).ok().as_ref() == Some(&source) {
            return Ok(json!({ "ok": true, "path": target, "alreadyInstalled": true }));
        }

        let parent = target
            .parent()
            .ok_or_else(|| Error::new("config", "Shared executable needs a parent directory"))?;
        fs::create_dir_all(parent)?;

        // Atlas.app's executable is sealed with its bundle: a copy would
        // fail its signature, so the shared path links to it instead, and
        // follows the app through updates.
        if crate::update::app_of(&source).is_some() {
            if !crate::update::reachable_by_others(&source) {
                return Err(Error::new(
                    "install",
                    "The helper account cannot reach Atlas.app here; install it in /Applications",
                ));
            }

            let temporary = target.with_extension(format!("{}.new", crate::control::token()));
            let linked = std::os::unix::fs::symlink(&source, &temporary)
                .and_then(|()| fs::rename(&temporary, target));
            if linked.is_err() {
                let _ = fs::remove_file(&temporary);
            }

            linked?;
            return Ok(json!({ "ok": true, "path": target, "linkedTo": source }));
        }

        crate::config::write_beside(
            target,
            0o755,
            |file| {
                std::io::copy(&mut fs::File::open(&source)?, file)?;
                file.set_permissions(fs::Permissions::from_mode(0o755))?;
                Ok(())
            },
            // A running helper may still map the previous executable. Renaming
            // a fresh inode avoids modifying its code-signed pages in place.
            |temporary| Ok(fs::rename(temporary, target)?),
        )?;

        Ok(json!({ "ok": true, "path": target }))
    }

    /// Construction only validates; reads such as status never write; doctor
    /// only probes that Atlas's own folders can be written, making them when
    /// missing.
    pub fn new(config: Config) -> Result<Self> {
        crate::session::validate_uid(config.helper_uid)?;
        Ok(Self {
            controller: Controller::new(config.helper_uid)?,
            transport: Transport {
                path: config.socket_path.clone(),
                timeout: Duration::from_secs(30),
                peer_uid: config.helper_uid,
            },
            installed: BuildCache::default(),
            actor: Actor::Agent,
            doing: None,
            host_session: None,
            task: None,
            config,
        })
    }

    /// This client, recording what its command does in each activity row.
    pub fn doing(mut self, doing: Doing) -> Self {
        self.doing = Some(doing);
        self
    }

    /// This client, keeping no control's name in what it records.
    pub fn unlabelled(mut self) -> Self {
        self.doing = self.doing.map(Doing::unlabelled);
        self
    }

    /// The private output directory, created 0700 (with any missing parents)
    /// on first use. An existing directory must already be private; it is
    /// never re-permissioned. The umask can only remove mode bits.
    pub fn output_dir(&self) -> Result<&Path> {
        let path = &self.config.output_dir;
        if !path.exists() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(|error| Error::writing(path, error))?;
        }

        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(Error::new(
                "config",
                format!(
                    "Output directory {} must be a private (0700) directory you own",
                    path.display()
                ),
            ));
        }

        Ok(path)
    }

    pub async fn status(&self) -> Value {
        self.verified_status().await.unwrap_or_else(|e| json!(e))
    }

    async fn verified_status(&self) -> Result<Value> {
        let mut hello = self.transport.call(&json!({"op": "hello"})).await?;

        // Do not launch the probe into an unverified UID or the visible desktop.
        if hello["ok"] != true
            || hello["user"]["uid"].as_u64() != Some(self.config.helper_uid.into())
            || hello["session"]["onConsole"] != false
        {
            return Ok(hello);
        }

        hello["inputService"] = self
            .input_transport()
            .call(&json!({ "op": "hello" }))
            .await
            .unwrap_or_else(|error| json!(error));
        hello["installedBuild"] = json!(self.installed_build());
        hello["sessionProof"] = self
            .session_proof()
            .await
            .unwrap_or_else(|error| json!(error));

        Ok(hello)
    }

    async fn session_proof(&self) -> Result<Value> {
        let request = json!({
            "op": "run",
            "argv": [self.config.identity_probe, "session-info"],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true || reply["exitCode"] != 0 || reply["timedOut"] != false {
            return Err(Error::new(
                "identity",
                format!(
                    "The helper cannot run the shared Atlas command at {}. Run atlas \
                     install.",
                    self.config.identity_probe.display()
                ),
            ));
        }

        let output = run_output(&reply, 4096)
            .map_err(|_| Error::new("identity", "Malformed or oversized identity output"))?;

        Ok(serde_json::from_slice(&output)?)
    }

    pub async fn identity(&self) -> Result<Session> {
        require_hidden(&self.status().await, self.config.helper_uid)
    }

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
            return Err(Error::new("input", "Maximum dimension must be 1–8192"));
        }

        let before = self.identity().await?;
        if expected.is_some_and(|epoch| *epoch != before.epoch) {
            return Err(Error::new(
                "session-changed",
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
                        "usage",
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
                "capture",
                reply["error"].as_str().unwrap_or("Capture failed"),
            ));
        }
        let shows = cropped(&reply, crop, &before.epoch.display)?;

        let at = Instant::now();
        let encoded = reply["png"]
            .as_str()
            .ok_or_else(|| Error::new("capture", "Missing PNG"))?;
        let png = STANDARD
            .decode(encoded)
            .map_err(|_| Error::new("capture", "Invalid PNG encoding"))?;
        let (width, height) = png_dimensions(&png)?;
        if reply["width"].as_u64() != Some(width.into())
            || reply["height"].as_u64() != Some(height.into())
        {
            return Err(Error::new(
                "capture",
                "PNG dimensions disagree with metadata",
            ));
        }

        let after = self.identity().await?;
        if before.epoch != after.epoch {
            return Err(Error::new(
                "session-changed",
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
    pub async fn capture_in(
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
        let _ = self.record(&request, &reply, started.elapsed());

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

    pub fn input_transport(&self) -> Transport {
        Transport {
            path: self.config.input_socket.clone(),
            timeout: Duration::from_secs(5),
            peer_uid: self.config.helper_uid,
        }
    }

    pub async fn native_control(&self, operation: &str, owner: &str) -> Result<Value> {
        self.native_request(json!({ "op": operation, "owner": owner }))
            .await
    }

    pub async fn acquire_input(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
        // The person's viewer may take over while agents are stopped.
        self.pending_until_known(self.native_request(json!({
            "op": "acquire",
            "owner": owner,
            "epoch": epoch.input_epoch(),
            "person": self.actor == Actor::Viewer,
        })))
        .await
    }

    /// [`Client::acquire_input`], trying again while a grant another command
    /// left behind runs out: a definite busy from another agent is retried
    /// about ten times a second for what is left of the controller's wait,
    /// and no longer than a grant can outlive its holder. A busy from the
    /// person, and every other answer, is returned at once, and a saved
    /// Stop or a signal's stop ends the retries.
    async fn acquire_input_waiting(&self, owner: &str, epoch: &Epoch) -> Result<Value> {
        let mut until = None;
        loop {
            match self.acquire_input(owner, epoch).await {
                Err(error)
                    if error.code == "busy"
                        && error.holder == Some(Holder::Agent)
                        && !error.uncertain =>
                {
                    let until = *until.get_or_insert_with(|| {
                        Instant::now() + self.controller.remaining().min(GRANT_OUTLIVES)
                    });
                    let left = until.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(error);
                    }
                    tokio::time::sleep(crate::control::poll(left)).await;
                    // The service may not have heard of a Stop the person's
                    // side has saved.
                    if self.stop_marked() {
                        return Err(Error::new("stopped", crate::error::STOPPED));
                    }
                    self.controller.interrupt().check()?;
                }
                outcome => return outcome,
            }
        }
    }

    /// Stop agents in the helper (`on`), or let them go on. Stopping ends
    /// an agent's input under way and refuses its next actions; the person
    /// keeps the viewer. Only the person resumes, through a viewer. An input
    /// service too old to stop is refused.
    pub async fn hold(&self, on: bool) -> Result<Value> {
        let held = self.hold_input(on).await;
        if !on {
            return held;
        }

        // Background jobs end with a Stop too. The helper daemon also sees
        // the input service's saved stop, so one that is down ends them
        // itself once it starts; one that cannot be told is said.
        let mut reply = held?;
        if let Some(warning) = self.end_jobs_for_stop().await {
            match reply["warnings"].as_array_mut() {
                Some(warnings) => warnings.push(json!(warning)),
                None => reply["warnings"] = json!([warning]),
            }
        }
        Ok(reply)
    }

    /// Ask the helper daemon to end every background job for the person's
    /// Stop, within two seconds and never under control, so the person is
    /// never queued: why it could not be told, when it could not be. A
    /// daemon that is not running, or that keeps no jobs, has none to end.
    async fn end_jobs_for_stop(&self) -> Option<&'static str> {
        let daemon = Transport {
            path: self.config.socket_path.clone(),
            timeout: Duration::from_secs(2),
            peer_uid: self.transport.peer_uid,
        };
        let request = json!({"op": "job", "action": "stop-all", "reason": "person"});
        match daemon.call(&request).await {
            Ok(reply) if reply["ok"] == true || reply["code"] == "bad-request" => None,
            Err(error) if error.code == "io" && !self.config.socket_path.exists() => None,
            _ => Some("Atlas could not reach the helper to end its background jobs"),
        }
    }

    /// [`Client::hold`] in the input service and on the person's side.
    async fn hold_input(&self, on: bool) -> Result<Value> {
        if !on && self.actor != Actor::Viewer {
            return Err(Error::new(
                "stopped",
                "Only the person resumes agents, from the Atlas menu bar",
            ));
        }

        // The person's side keeps the Stop first, so it holds even when the
        // helper's input service is down or older: agents' commands read it.
        if on {
            self.controller.note_stop(true)?;
        }

        let unreachable = |why: &str| {
            if !on {
                if !matches!(
                    self.config
                        .input_socket
                        .with_extension("stopped")
                        .try_exists(),
                    Ok(false)
                ) {
                    return Err(Error::new(
                        "stopped",
                        format!("{why} Recover input before agents can resume."),
                    ));
                }
                self.controller.note_stop(false)?;
            }
            Ok(json!({
                "ok": true,
                "held": on,
                "kept": true,
                "warnings": [format!("{why} Agents are {} here all the same.", if on { "stopped" } else { "free to go on" })],
            }))
        };
        let Ok(hello) = self.input_transport().call(&json!({"op": "hello"})).await else {
            return unreachable("The helper's input service isn't running.");
        };
        if !has_capability(&hello, HOLD) {
            return unreachable("The helper's input service is from an older build.");
        }

        let mut reply = match self.native_request(json!({"op": "hold", "on": on})).await {
            Ok(reply) => reply,
            Err(error) if on => {
                let mut reply = if error.uncertain {
                    unreachable("The helper's input service may or may not have been told.")
                } else {
                    unreachable(&format!(
                        "The helper's input service reported a problem while stopping: {}.",
                        error.error.trim_end_matches('.')
                    ))
                }?;
                if error.uncertain {
                    reply["uncertain"] = json!(true);
                }
                return Ok(reply);
            }
            Err(error) => return Err(error),
        };
        if !on {
            self.controller.note_stop(false)?;
        }
        if reply["kept"] == false {
            reply["warnings"] = json!([
                "The helper could not save this, so it lasts only until its input restarts"
            ]);
        }

        Ok(reply)
    }

    /// Once the person stops agents: either side's saved marker appears.
    async fn stopped_while_running(&self) {
        let marker = self.config.input_socket.with_extension("stopped");
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if marker.exists() || self.controller.stop_noted() {
                return;
            }
        }
    }

    /// Refuse an agent's command that would change the helper while the
    /// person has stopped agents. Input is also refused by the input service
    /// itself; commands run through the helper daemon, which knows nothing
    /// of a stop. The stop's saved marker counts too, so a stopped helper
    /// whose input service is down still refuses.
    pub async fn require_going(&self) -> Result<()> {
        if self.actor == Actor::Viewer {
            return Ok(());
        }

        let marked = self.stop_marked();
        let hello = self.input_transport().call(&json!({"op": "hello"})).await;
        if marked || hello.is_ok_and(|hello| hello["held"] == true) {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }

        Ok(())
    }

    /// Whether a Stop is saved on either side: the input service's own
    /// marker, or the person's note.
    pub(crate) fn stop_marked(&self) -> bool {
        self.config.input_socket.with_extension("stopped").exists() || self.controller.stop_noted()
    }

    async fn native_request(&self, request: Value) -> Result<Value> {
        let result = self.native_call(&request).await?;
        if result["ok"] != true {
            let error = Error::new(
                match result["code"].as_str() {
                    Some("busy") => "busy",
                    Some("lease-lost") => "lease-lost",
                    Some("cancelled") => "cancelled",
                    Some("stopped") => "stopped",
                    Some("identity") => "identity",
                    Some("session-changed") => "session-changed",
                    Some("permission") => "permission",
                    Some("focus-changed") => "focus-changed",
                    _ => "native",
                },
                result["error"]
                    .as_str()
                    .unwrap_or("Native control request failed"),
            );

            // Who holds control, when the service says.
            let error = match result["holder"].as_str().and_then(Holder::parse) {
                Some(holder) => error.held_by(holder),
                None => error,
            };

            // The service reports when a failure may already have changed state.
            return Err(if result["uncertain"] == true {
                error.uncertain()
            } else {
                error
            });
        }

        Ok(result)
    }

    /// Send `request` to the input service and read its answer. It is
    /// never dropped half sent; an answer still missing when a signal's stop
    /// cuts the step under way leaves its outcome unknown, so the claim the
    /// caller holds for it stays pending.
    async fn native_call(&self, request: &Value) -> Result<Value> {
        let interrupt = self.controller.interrupt();
        let connection = self.input_transport().connect().await?;
        let sent = {
            let _sending = interrupt.sending();
            connection.dispatch(request).await?
        };

        tokio::select! {
            biased;
            reply = sent.finish() => reply,
            signal = interrupt.cut() => Err(interrupt.cut_short(signal)),
        }
    }

    pub async fn input(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
    ) -> Result<Value> {
        let _ownership = if lease.is_none() {
            Some(self.controller.acquire_beside_commands()?)
        } else {
            None
        };

        self.input_inner(actions, expected, lease, None).await
    }

    /// Send one validated batch under controller ownership the caller already
    /// holds, refusing unless the helper still has exactly `epoch`. With a
    /// `check`, the helper sends each action only while it would reach that
    /// app.
    pub async fn input_owned(
        &self,
        _ownership: &Ownership,
        actions: &Value,
        epoch: &Epoch,
        check: Option<Check>,
    ) -> Result<Value> {
        self.input_inner(actions, Some((epoch, Instant::now())), None, check)
            .await
    }

    /// The input service, still running in this same session, was pinned
    /// to an earlier display (switching accounts can renumber it): pin it to
    /// the current verified display, as start-input does. When the display
    /// changed size it is re-pinned too, but the action is refused so the
    /// caller looks at the screen again before acting on it.
    async fn repin(&self, session: Session) -> Result<Session> {
        let unavailable = || {
            Error::new(
                "startup",
                "The native input service is unavailable; run atlas start-input",
            )
        };
        let status = self.status().await;
        require_accessibility(&status)?;
        if !crate::session::pinned_to_session(&status, &session) {
            return Err(unavailable());
        }

        let epoch = &session.epoch;
        let pinned = &status["inputService"]["enforcedEpoch"];
        let same_size = pinned["width"].as_f64() == Some(epoch.display.width)
            && pinned["height"].as_f64() == Some(epoch.display.height);

        self.pending_until_known(
            self.native_request(json!({"op": "reset", "epoch": epoch.input_epoch()})),
        )
        .await?;
        let current = self.identity().await?;
        if !current.input {
            return Err(unavailable());
        }

        if !same_size {
            return Err(Error::new(
                "session-changed",
                "The helper's display changed size. Take a new screenshot, then try again.",
            ));
        }

        Ok(current)
    }

    async fn input_inner(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Check>,
    ) -> Result<Value> {
        // An agent's input holds control taken beside commands; the person's
        // holds the lease, which no command can start under.
        self.controller.require_known_beside(Beside::Commands)?;

        // A viewer's fresh observation is already tied to its acquired lease.
        // The native process validates that exact epoch again before every
        // event. The person's own input is never checked.
        if let (Some((epoch, _)), Some(_)) = (expected, lease) {
            debug_assert!(check.is_none(), "the person's input is never checked");
            let session = Session {
                epoch: epoch.clone(),
                capture: true,
                input: true,
            };
            actions::validate(actions, &session.epoch.display)?;
            return self
                .input_in_session(actions, expected, lease, None, &session)
                .await;
        }

        let status = self.status().await;
        self.input_with_status(&status, actions, expected, lease, check)
            .await
    }

    /// [`Client::input_inner`] once `status` is read. The epoch and whether
    /// the input service checks an agent's app come from this one status,
    /// so both belong to the very process the input goes to.
    async fn input_with_status(
        &self,
        status: &Value,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        check: Option<Check>,
    ) -> Result<Value> {
        let session = require_hidden(status, self.config.helper_uid)?;
        let expect = expectation(&status["inputService"], check)?;

        if expected.is_some_and(|(epoch, _)| *epoch != session.epoch) {
            return Err(Error::new(
                "session-changed",
                "The helper changed since this frame",
            ));
        }

        let input_pid = session.epoch.input_pid;
        let session = if session.input || lease.is_some() {
            session
        } else {
            self.repin(session).await?
        };

        // The service whose capability was read is the one that checks.
        if expect.is_some() && session.epoch.input_pid != input_pid {
            return Err(Error::new(
                "session-changed",
                "The helper's input service changed meanwhile",
            ));
        }

        actions::validate(actions, &session.epoch.display)?;

        self.input_in_session(actions, expected, lease, expect, &session)
            .await
    }

    /// Carry out validated input in the verified session, retaining the
    /// standalone action's claim until native cleanup is acknowledged.
    async fn input_in_session(
        &self,
        actions: &Value,
        expected: Option<(&Epoch, Instant)>,
        lease: Option<(&SharedLease, &str)>,
        expect: Option<Expect>,
        session: &Session,
    ) -> Result<Value> {
        self.controller.interrupt().check()?;
        let standalone = crate::control::token();
        let owner = lease.map(|(_, token)| token).unwrap_or(&standalone);
        if lease.is_none()
            && let Err(error) = self.acquire_input_waiting(owner, &session.epoch).await
        {
            if error.uncertain {
                // The service may now hold input for `owner`: release it, or
                // leave the outcome pending for recovery. The unacknowledged
                // acquire stays pending even if this cleanup succeeds.
                let _ = self
                    .pending_until_known(self.native_control("release", owner))
                    .await;
            }

            return Err(error);
        }

        let generation = lease
            .is_none()
            .then(|| self.controller.quarantine())
            .transpose()?;
        let mut request = json!({
            "op": "input",
            "actions": actions,
            "owner": owner,
            "epoch": session.epoch.input_epoch(),
            // An agent's command names the app it acted in.
            "app": self.doing.is_some(),
        });
        if let Some(expect) = expect {
            request["expect"] = json!(expect);
        }
        let result = self
            .mutate(request, lease, expected.map(|(_, at)| at), None)
            .await;

        if lease.is_none() {
            let cleanup = self.native_control("release", owner).await;
            let unknown = result
                .as_ref()
                .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
                || cleanup.as_ref().is_err_and(|error| error.uncertain);
            if cleanup.is_ok()
                && !unknown
                && let Some(generation) = &generation
            {
                self.controller.clear_if(generation)?;
            }
            if let Err(cleanup) = cleanup {
                // A release the stop cut short says so, as its input may.
                if cleanup.code == "interrupted" {
                    return Err(cleanup);
                }
                let error = match result {
                    Ok(_) => Error::new(
                        "uncertain",
                        "Input finished but native release was not confirmed",
                    ),
                    Err(error) => Error::new(
                        "uncertain",
                        format!("{} Native release was not confirmed either.", error.error),
                    ),
                };
                return Err(error.uncertain());
            }
        }

        result
    }

    pub async fn run(&self, argv: Vec<String>) -> Result<Value> {
        self.run_within(argv, RUN_TIMEOUT).await
    }

    /// [`Client::run`], stopping the command after `timeout` (at most ten
    /// minutes). Control stays held until it ends.
    pub async fn run_within(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        let ownership = self.controller.acquire(false)?;
        self.run_owned_within(&ownership, argv, timeout).await
    }

    /// Run argv in the verified hidden helper as an agent's own command,
    /// stopping it after `timeout`. Control is held only until the request
    /// reaches the helper; while the command runs, its claim keeps
    /// recovery, restart, update and log-out out, and other agents may act.
    /// Its outcome is pending until it ends.
    pub async fn run_beside(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        let request = run_request(argv, timeout)?;
        let ownership = self.controller.acquire_beside_commands()?;
        self.identity().await?;

        self.mutate(request, None, None, Some(ownership)).await
    }

    /// Execute argv in the verified hidden helper as a recorded mutation under
    /// controller ownership the caller already holds.
    pub async fn run_owned(&self, ownership: &Ownership, argv: Vec<String>) -> Result<Value> {
        self.run_owned_within(ownership, argv, RUN_TIMEOUT).await
    }

    /// [`Client::run_owned`], stopping the command after `timeout`.
    pub async fn run_owned_within(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
        timeout: Duration,
    ) -> Result<Value> {
        let request = run_request(argv, timeout)?;
        self.identity().await?;

        self.mutate(request, None, None, None).await
    }

    /// Run a fixed, read-only inspection command (such as the window list) in
    /// the verified hidden helper. It records no pending mutation, so callers
    /// must only pass commands that cannot change the helper.
    pub(crate) async fn inspect(&self, argv: Vec<String>) -> Result<Value> {
        self.inspect_within(argv, INSPECT_TIMEOUT).await
    }

    /// [`Client::inspect`] bounded by `limit`. A read changes nothing, so
    /// its failure is never uncertain.
    pub(crate) async fn inspect_within(&self, argv: Vec<String>, limit: Duration) -> Result<Value> {
        self.run_pending(argv, limit).await.map_err(certain)
    }

    /// [`Client::inspect_within`] in the helper session `verified` names,
    /// which the caller verified moments before. A daemon that can be told
    /// the session refuses the run in any other, so it is not verified
    /// again; an older one is verified again first, as for any inspection.
    pub(crate) async fn inspect_in(
        &self,
        verified: &Verified,
        argv: Vec<String>,
        limit: Duration,
    ) -> Result<Value> {
        let pin = verified.pin();
        if pin.is_none() {
            self.identity().await?;
        }

        self.run_verified(argv, limit, pin).await.map_err(certain)
    }

    /// [`Client::inspect_within`] with a run's usual limit, under controller
    /// ownership the caller already holds. Only for commands that cannot
    /// change the helper: nothing is left pending when it fails.
    pub(crate) async fn inspect_owned(
        &self,
        _ownership: &Ownership,
        argv: Vec<String>,
    ) -> Result<Value> {
        self.inspect_within(argv, RUN_TIMEOUT).await
    }

    /// Execute argv in the verified hidden helper as part of a mutation the
    /// caller holds ownership for and has already recorded as pending. The
    /// caller settles that record once the outcome is established. A
    /// refusal is an error with the helper's code, uncertain when the
    /// command may have started.
    pub async fn run_pending(&self, argv: Vec<String>, timeout: Duration) -> Result<Value> {
        self.identity().await?;
        self.run_verified(argv, timeout, None).await
    }

    /// [`Client::run_pending`] once the helper is verified, served only in
    /// the session `pin` names when there is one.
    async fn run_verified(
        &self,
        argv: Vec<String>,
        timeout: Duration,
        pin: Option<SessionPin>,
    ) -> Result<Value> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let mut request = json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms});
        if let Some(pin) = pin {
            request["session"] = json!(pin);
        }

        // The reply comes after the command ends, as for any other run.
        let mut transport = self.transport.clone();
        transport.timeout = transport.timeout.max(timeout + RUN_MARGIN);

        let reply = transport.call(&request).await?;
        validate_completion(&request, &reply)?;

        accepted(reply)
    }

    /// The helper's running apps, or why they could not be read. Only a
    /// reply the helper accepted, in the shape [`app_list`] checks, counts:
    /// a refusal keeps its code (busy, apps), and a reply of any other shape
    /// is never read as "nothing is running". Reading changes nothing in the
    /// helper, so a failure here is never uncertain.
    pub async fn apps(&self) -> Result<Value> {
        let read = async {
            self.identity().await?;
            self.listed_apps(json!({"op": "apps"})).await
        };
        read.await.map_err(certain)
    }

    /// [`Client::apps`] in the helper session `verified` names, as
    /// [`Client::inspect_in`] runs: a daemon that can be told the session
    /// refuses the listing in any other, and an older one is verified again.
    pub(crate) async fn apps_in(&self, verified: &Verified) -> Result<Value> {
        let Some(pin) = verified.pin() else {
            return self.apps().await;
        };

        self.listed_apps(json!({"op": "apps", "session": pin}))
            .await
            .map_err(certain)
    }

    /// The app list `request` asks the helper daemon for, once the helper
    /// is verified.
    async fn listed_apps(&self, request: Value) -> Result<Value> {
        app_list(self.transport.call(&request).await?)
    }

    /// The helper session as the daemon reports it now, and whether that
    /// daemon refuses a read meant for another session: what reads in this
    /// session are checked by. It verifies the helper as
    /// [`Client::identity`] does.
    pub async fn verify(&self) -> Result<Verified> {
        let status = self.status().await;
        let session = require_hidden(&status, self.config.helper_uid)?;
        let pinned = has_capability(&status["daemon"], SESSION_PIN);
        Ok(Verified { session, pinned })
    }

    /// [`Client::verify`], refusing a helper daemon that does not offer
    /// `capability` with `daemon` and `older`, which says what it cannot do
    /// and how to bring it up to date. Nothing is sent to an older daemon.
    pub(crate) async fn verify_offering(
        &self,
        capability: &str,
        older: &'static str,
    ) -> Result<Verified> {
        let status = self.status().await;
        let session = require_hidden(&status, self.config.helper_uid)?;
        if !has_capability(&status["daemon"], capability) {
            return Err(Error::new("daemon", older));
        }
        let pinned = has_capability(&status["daemon"], SESSION_PIN);
        Ok(Verified { session, pinned })
    }

    pub async fn start_input(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(false)?;
        self.identity().await?;
        self.launch_input().await
    }

    /// SHA-256 of the installed shared executable, rehashed only when its
    /// device, inode, size or modification time changes.
    pub fn installed_build(&self) -> Option<String> {
        let path = &self.config.identity_probe;
        // The executable itself (through a link to the app), not the link.
        let metadata = fs::metadata(path).ok()?;
        let key = (
            metadata.dev(),
            metadata.ino(),
            metadata.size(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        );
        if let Some((cached, build)) = self.installed.lock().ok()?.as_ref()
            && *cached == key
        {
            return Some(build.clone());
        }

        // Hashed without the lock, so other callers are not held up. A
        // failed read is not cached: permissions may be repaired without
        // changing the identity key.
        let build = crate::digest::probe_executable(path).and_then(|path| file_sha256(&path))?;

        if let Ok(mut cache) = self.installed.lock() {
            *cache = Some((key, build.clone()));
        }

        Some(build)
    }

    /// start-input: replace a stale service, otherwise make one ready.
    async fn launch_input(&self) -> Result<Value> {
        let status = self.status().await;
        require_hidden(&status, self.config.helper_uid)?;

        let native = &status["inputService"];
        let current = crate::session::input_build_current(&status);
        if native["ok"] == true && !current {
            return self.upgrade_input(&status).await;
        }

        let (pid, outcome) = self.ensure_input(&status).await?;
        Ok(json!({ "ok": true, outcome: true, "pid": pid }))
    }

    /// Make a ready input service exist without replacing a running one:
    /// reuse it, reset it, or start the installed build.
    async fn ensure_input(&self, status: &Value) -> Result<(u32, &'static str)> {
        let session = require_hidden(status, self.config.helper_uid)?;
        if session.input {
            return Ok((session.epoch.input_pid, "alreadyRunning"));
        }

        if status["inputService"]["ok"] == true {
            let reset = self.native_request(json!({
                "op": "reset",
                "epoch": session.epoch.input_epoch(),
            }));
            self.pending_until_known(reset).await?;

            if !self.identity().await?.input {
                return Err(Error::new(
                    "startup",
                    "Native input did not become ready after reset",
                ));
            }

            return Ok((session.epoch.input_pid, "recovered"));
        }

        // Without Accessibility a new service can never become ready, and a
        // start that cannot be confirmed would stay pending; refuse plainly.
        require_accessibility(status)?;
        let pid = self.pending_until_known(self.spawn_input()).await?;

        Ok((pid, "started"))
    }

    /// Carry out a change to the input service as a pending mutation: it
    /// stays pending when its outcome is unknown, and clears once it is
    /// known, done or refused.
    async fn pending_until_known<T>(&self, change: impl Future<Output = Result<T>>) -> Result<T> {
        let generation = self.controller.quarantine()?;
        let outcome = change.await;
        if outcome
            .as_ref()
            .map_or_else(|error| !error.uncertain, |_| true)
        {
            self.controller.clear_if(&generation)?;
        }

        outcome
    }

    /// Replace a running service whose build differs from the installed
    /// executable through its authenticated shutdown. The replacement stays
    /// a pending mutation until it is ready, current and holds nothing.
    async fn upgrade_input(&self, status: &Value) -> Result<Value> {
        let native = &status["inputService"];

        if !status["installedBuild"].is_string() {
            return Err(Error::new(
                "startup",
                "The installed executable cannot be read; refusing to replace the running service",
            ));
        }

        if !has_capability(native, SHUTDOWN) {
            return Err(Error::new(
                "startup",
                "The running input service predates authenticated shutdown and cannot be \
                 replaced automatically. Stop that process in the helper account (or log the \
                 helper out and in), then run atlas start-input.",
            ));
        }

        if native["controlActive"] != false {
            return Err(Error::new(
                "busy",
                "The input service has an active controller; release it before upgrading",
            ));
        }

        let previous = native["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 1)
            .ok_or_else(|| Error::new("startup", "The running input service has no valid PID"))?;
        let generation = self.controller.quarantine()?;

        if let Err(error) = self.stop_input(previous).await {
            if !error.uncertain {
                self.controller.clear_if(&generation)?;
            }
            return Err(error);
        }

        let pid = self.spawn_input().await?;
        let state = self.status().await;
        if !input_current(&state, self.config.helper_uid, pid) {
            return Err(Error::new(
                "uncertain",
                "The replacement input service is not confirmed current with nothing held",
            )
            .uncertain());
        }

        self.controller.clear_if(&generation)?;

        Ok(json!({
            "ok": true,
            "upgraded": true,
            "previousPid": previous,
            "pid": pid,
            "build": state["inputService"]["build"],
        }))
    }

    /// Ask exactly `pid` to shut down over its authenticated socket, then wait
    /// until its endpoint is closed and the process is gone. Anything short of
    /// that is uncertain; a definitive refusal changed nothing.
    pub async fn stop_input(&self, pid: u32) -> Result<()> {
        self.native_request(json!({ "op": "shutdown", "pid": pid }))
            .await
            .map_err(|error| {
                if error.uncertain {
                    error
                } else {
                    Error::new(
                        error.code,
                        format!("The running input service refused to stop: {}", error.error),
                    )
                }
            })?;

        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            let closed = matches!(
                tokio::net::UnixStream::connect(&self.config.input_socket).await,
                Err(error) if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                )
            );
            if closed && !process_exists(pid) {
                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    "uncertain",
                    "The previous input service did not confirm exit",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Start the installed executable as the helper's input service. The caller
    /// records the launch as a pending mutation and settles it on success.
    pub async fn spawn_input(&self) -> Result<u32> {
        // Arguments stay separate from shell syntax, including configured paths.
        let script = concat!(
            "nohup \"$1\" input-service --uid \"$2\" --controller-uid \"$3\" --socket \"$4\" ",
            ">\"$HOME/Library/Logs/Atlas-input.log\" 2>&1 </dev/null & echo $!",
        );
        let controller_uid = nix::unistd::getuid().as_raw();
        let request = json!({
            "op": "run",
            "argv": [
                "/bin/sh", "-c", script, "sh",
                self.config.identity_probe,
                self.config.helper_uid.to_string(),
                controller_uid.to_string(),
                self.config.input_socket,
            ],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;

        // The launch prints the child PID; only that process may satisfy readiness.
        let child = (reply["ok"] == true && reply["exitCode"] == 0)
            .then(|| run_output(&reply, 32).ok())
            .flatten()
            .and_then(|output| String::from_utf8(output).ok())
            .and_then(|text| text.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 1)
            .ok_or_else(|| {
                Error::new(
                    "startup",
                    "The helper could not launch the Rust input service",
                )
                .uncertain()
            })?;

        let deadline = Instant::now() + Duration::from_secs(6);

        loop {
            let state = self.status().await;
            if require_hidden(&state, self.config.helper_uid)
                .is_ok_and(|session| session.input && session.epoch.input_pid == child)
            {
                return Ok(child);
            }

            if Instant::now() >= deadline {
                return Err(Error::new(
                    "startup",
                    "Input service did not become ready. Check ~/Library/Logs/Atlas-input.log \
                     in the helper account and confirm that identity_probe points to the current Rust binary.",
                )
                .uncertain());
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn restart(&self) -> Result<Value> {
        let _ownership = self.controller.acquire(true)?;
        let before = self.identity().await?;

        // Recovery goes through the input service; with none running and no
        // Accessibility to start one, it cannot finish, so nothing is marked.
        let current = self.status().await;
        if current["inputService"]["ok"] != true {
            require_accessibility(&current)?;
        }

        // Held through verification, so the restart reads as in progress.
        // A refusal that changed nothing clears it, as for a log-out.
        let claim = self.controller.quarantine()?;
        let outcome = self.restart_claimed(&before).await;

        self.settle_restart(&claim, outcome)
    }

    /// Settle this restart after verified recovery or a definite refusal.
    /// Other live mutations retain their own claims.
    fn settle_restart(&self, claim: &str, outcome: Result<Value>) -> Result<Value> {
        let refused = match &outcome {
            Ok(reply) => reply["ok"] != true && reply["uncertain"] != true,
            Err(error) => !error.uncertain,
        };
        if refused || outcome.as_ref().is_ok_and(|reply| reply["ok"] == true) {
            self.controller.clear_if(claim).map_err(Error::uncertain)?;
        }

        outcome
    }

    /// The restart itself, under the claim [`Client::restart`] holds: input
    /// cleared first, then the helper restarted and confirmed new.
    async fn restart_claimed(&self, before: &Session) -> Result<Value> {
        // Input has its own process. Clear held state there before restarting
        // the capture/command helper; a new capture PID alone proves nothing
        // about an uncertain mouse or keyboard action.
        let initial = self.status().await;
        // A service whose file was replaced (an app update) cannot become
        // ready under the restarted helper; it is replaced there instead.
        let stale = !crate::session::input_build_current(&initial);
        let (input_pid, _) = self.ensure_input(&initial).await?;
        let session = self.identity().await?;
        if session.epoch.input_pid != input_pid {
            return Err(
                Error::new("uncertain", "The input service changed while recovering").uncertain(),
            );
        }

        let cleanup = self
            .native_request(json!({ "op": "reset", "epoch": session.epoch.input_epoch() }))
            .await?;
        if cleanup["released"] != true {
            return Err(
                Error::new("uncertain", "Native input cleanup was not confirmed").uncertain(),
            );
        }

        let reply = self
            .mutate(json!({"op": "restart"}), None, None, None)
            .await?;
        if reply["ok"] != true {
            return Ok(reply);
        }

        // The helper was told to restart: from here on, every failure leaves
        // the outcome unknown, so the claim stays until recovery.
        let recovery = async {
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let status = self.status().await;

                // Settle only with a new capture process and the same input
                // service whose cleanup was acknowledged, idle and empty.
                let Ok(session) = require_hidden(&status, self.config.helper_uid) else {
                    continue;
                };

                if session.epoch.capture_pid == before.epoch.capture_pid {
                    continue;
                }

                if input_settled(&status, self.config.helper_uid, input_pid) {
                    self.controller.clear().map_err(Error::uncertain)?;
                    return Ok(json!({"ok": true, "restarted": true, "session": status}));
                }

                // Its cleanup was confirmed above, so nothing is held: a stale
                // service that is not ready under the new helper is replaced.
                if stale
                    && status["inputService"]["pid"] == input_pid
                    && status["inputService"]["ready"] == false
                {
                    self.upgrade_input(&status)
                        .await
                        .map_err(Error::uncertain)?;
                    self.controller.clear().map_err(Error::uncertain)?;
                    let status = self.status().await;

                    return Ok(json!({
                        "ok": true,
                        "restarted": true,
                        "inputReplaced": true,
                        "session": status,
                    }));
                }
            }
        };

        tokio::time::timeout(Duration::from_secs(20), recovery)
            .await
            .map_err(|_| {
                Error::new(
                    "restart",
                    "Restart acknowledged, but no new hidden helper was confirmed",
                )
                .uncertain()
            })?
    }

    /// Log the helper out (`atlas logout`) while it is in the background
    /// (`identity` refuses it on screen). Its apps quit as at any log-out.
    /// Like a restart, it holds its own claim until the outcome is known:
    /// cleared once the desktop is gone or macOS refused, kept pending when
    /// it may still happen (an app holding it up, an unconfirmed request).
    /// `force` ends the session without asking the apps, and may run with
    /// outcomes still unknown: it is the person's way out of one.
    pub async fn logout(&self, force: bool) -> Result<Value> {
        let ownership = self.controller.acquire(force)?;
        self.logout_held(&ownership, force).await
    }

    /// [`Client::logout`] under control already held (`_ownership`), for a
    /// caller that keeps it across more than the log-out, such as uninstall.
    pub async fn logout_held(&self, _ownership: &Ownership, force: bool) -> Result<Value> {
        let status = self.status().await;
        require_hidden(&status, self.config.helper_uid)?;
        let claim = self.controller.quarantine()?;

        // Background jobs are stopped first, so none outlives the login it
        // ran in unsupervised.
        let jobs = if has_capability(&status["daemon"], crate::daemon::protocol::JOBS) {
            Some(crate::jobs::stop_every(self, crate::daemon::protocol::StopReason::Stopped).await)
        } else {
            None
        };
        let with_jobs = |mut reply: Value| {
            if let Some(jobs) = &jobs {
                jobs.say(&mut reply);
            }
            reply
        };

        match self
            .mutate(json!({"op": "logout", "force": force}), None, None, None)
            .await
        {
            Ok(reply) if reply["ok"] == true => {}
            // Refused before anything happened: nothing is pending.
            Ok(reply) if reply["uncertain"] != true => {
                self.controller.clear_if(&claim)?;
                return Ok(reply);
            }
            Ok(reply) => return Ok(reply),
            Err(error) => {
                if !error.uncertain {
                    self.controller.clear_if(&claim)?;
                }
                return Err(error);
            }
        }

        if self.desktop_gone(if force { 15 } else { 30 }).await {
            self.controller.clear_if(&claim)?;
            return Ok(with_jobs(json!({"ok": true, "loggedOut": true})));
        }

        if force {
            return Err(Error::new(
                "logout",
                "macOS took the request but the helper is still logged in",
            )
            .uncertain());
        }

        let open = self
            .apps()
            .await
            .map(|reply| crate::app_selection::left_open(&reply))
            .unwrap_or_default();
        let still = if open.is_empty() {
            String::new()
        } else {
            format!(" Still open: {}.", open.join(", "))
        };

        Err(Error::new(
            "logout-held",
            format!("The helper has not logged out yet; an app may be asking to save.{still}"),
        )
        .uncertain())
    }

    /// Whether the helper's login goes away within `seconds`: no
    /// loginwindow and no Dock of it, seen twice in a row half a second
    /// apart (a Dock alone can restart in a login that lives on), and its
    /// daemon no longer answering. A look that fails counts as present.
    async fn desktop_gone(&self, seconds: u64) -> bool {
        let uid = self.config.helper_uid;
        let looked = move || async move {
            tokio::task::spawn_blocking(move || crate::session::logged_out(uid))
                .await
                .ok()
                .flatten()
                == Some(true)
        };
        let answers = || async { self.transport.call(&json!({"op": "hello"})).await.is_ok() };

        gone_within(
            looked,
            answers,
            Duration::from_secs(seconds),
            Duration::from_millis(500),
        )
        .await
    }

    async fn mutate(
        &self,
        request: Value,
        lease: Option<(&SharedLease, &str)>,
        observed_at: Option<Instant>,
        // Control to let go of once the request has reached the helper; its
        // claim is then marked as a running command.
        release: Option<Ownership>,
    ) -> Result<Value> {
        debug_assert!(
            release.is_none() || request["op"] == "run",
            "only a command started with run lets go of control while it runs"
        );
        let started = Instant::now();
        let interrupt = self.controller.interrupt();
        interrupt.check()?;
        let command = release.is_some();

        // An agent's command that changes the helper is refused, before it
        // starts, once the person has stopped agents; reads and Atlas's own
        // steps (no `doing`) go on.
        let cuttable = request["op"] == "run" && self.actor == Actor::Agent && self.doing.is_some();
        let job_change = request["op"] == "job"
            && matches!(request["action"].as_str(), Some("start" | "stop"))
            && self.actor == Actor::Agent
            && self.doing.is_some();
        if (cuttable || job_change)
            && (self.config.input_socket.with_extension("stopped").exists()
                || self.controller.stop_noted())
        {
            return Err(Error::new("stopped", crate::error::STOPPED));
        }

        // Connect without holding the lease. Release can cancel slow preflight.
        let mut transport = if request["op"] == "input" {
            self.input_transport()
        } else {
            self.transport.clone()
        };

        // A long command's reply comes after it ends, and after the helper
        // has stopped whatever it left running.
        if let Some(run) = request["timeoutMs"]
            .as_u64()
            .filter(|_| request["op"] == "run")
        {
            transport.timeout = transport
                .timeout
                .max(Duration::from_millis(run) + RUN_MARGIN);
        }

        let connection = transport.connect().await?;

        // Serialize only final authorization and bounded byte submission.
        let mut authorization = match lease {
            Some((state, _)) => Some(state.lock().await),
            None => None,
        };

        if let (Some(state), Some((_, owner))) = (&mut authorization, lease) {
            state.require(owner)?;
        }

        if observed_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(8)) {
            return Err(Error::new(
                "stale-frame",
                "The observation expired before dispatch",
            ));
        }

        // Restart, log-out and standalone input run under the claim their
        // caller holds through verification or cleanup.
        let mut generation = (request["op"] != "restart"
            && request["op"] != "logout"
            && !(request["op"] == "input" && lease.is_none()))
        .then(|| {
            if command {
                self.controller.quarantine_command()
            } else {
                self.controller.quarantine()
            }
        })
        .transpose()?;

        // The last moment a stop refuses the request: it is sent whole, or
        // not at all, so its claim is settled whenever it is not sent.
        let sent = {
            let _sending = interrupt.sending();
            match interrupt.check() {
                Ok(()) => connection.dispatch(&request).await,
                Err(stop) => Err(stop),
            }
        };
        let pending = match sent {
            Ok(pending) => pending,
            Err(error) => {
                if let Some(generation) = &generation
                    && !error.uncertain
                {
                    self.controller.clear_if(generation)?;
                }
                return Err(error);
            }
        };

        drop(authorization);
        // A command an agent started lets go of control once its request is
        // sent: from here its claim, marked as a running command, keeps
        // recovery out until it ends, and other agents may act beside it.
        drop(release);

        // An agent's command runs until it ends, unless the person stops
        // agents: then the connection is dropped, which ends the command in
        // the helper. What it did by then is unknown, so it stays pending.
        // A command that started ends with a row too, however it ends. A
        // signal's stop cuts any request still unanswered after its grace
        // the same way.
        let finishing = pending.finish();
        let cut = interrupt.cut();
        let mut reply = if cuttable {
            let _ = self.record_start(&request);
            let ended = tokio::select! {
                biased;
                reply = finishing => reply,
                () = self.stopped_while_running() => {
                    Err(Error::new("stopped", crate::error::STOPPED).uncertain())
                }
                signal = cut => Err(interrupt.cut_short(signal)),
            };
            match ended {
                Ok(reply) => reply,
                Err(error) => {
                    // Its outcome is unknown from here: it stops counting as
                    // a running command before its row is written, which may
                    // wait for another command's.
                    let _recording = interrupt.step();
                    if command && let Some(claim) = generation.take() {
                        self.controller.abandon_command(claim);
                    }
                    let failed = json!({"ok": false, "code": recorded_code(&error)});
                    let _ = self.record(&request, &failed, started.elapsed());
                    return Err(error);
                }
            }
        } else {
            tokio::select! {
                biased;
                reply = finishing => reply?,
                signal = cut => return Err(interrupt.cut_short(signal)),
            }
        };
        validate_completion(&request, &reply)?;

        // A command whose helper could not say what it did is unknown from
        // here, before its row is written.
        let unknown = command && reply["uncertain"] == true;
        let _recording = unknown.then(|| interrupt.step());
        if unknown && let Some(claim) = generation.take() {
            self.controller.abandon_command(claim);
        }

        let mut warnings = Vec::new();
        if let Some(generation) = &generation
            && reply["uncertain"] != true
            && let Err(error) = self.controller.clear_if(generation)
        {
            warnings.push(format!(
                "Operation completed, but its pending marker could not be cleared: {error}"
            ));
        }

        if self.record(&request, &reply, started.elapsed()).is_err() {
            warnings
                .push("Operation result is valid; local metadata could not be recorded".to_owned());
        }

        if !warnings.is_empty() {
            reply["warnings"] = json!(warnings);
        }

        Ok(reply)
    }

    /// Send `request`, a change to the helper, as a recorded mutation under
    /// control the caller holds: it stays pending until its reply proves
    /// what it did.
    pub(crate) async fn change(&self, request: Value) -> Result<Value> {
        self.mutate(request, None, None, None).await
    }

    /// Whether this shell may keep Atlas's own files: the controller lock
    /// and the screenshots. An agent's sandbox may forbid both while the
    /// helper itself is fine.
    pub fn writable(&self) -> Result<()> {
        self.controller.writable()?;
        crate::control::probe_write(self.output_dir()?)?;

        // Builds and environment plans are staged beside the shared Atlas
        // command for the helper to read; a missing folder is setup's
        // concern, not this check's.
        if let Some(shared) = self.config.identity_probe.parent()
            && shared.is_dir()
        {
            crate::control::probe_write(shared)?;
        }

        Ok(())
    }

    /// Record that a command began, so what it does shows while it runs;
    /// the row it ends with stands for it after.
    fn record_start(&self, request: &Value) -> Result<()> {
        self.write_row(request, &json!({"ok": null}), Duration::ZERO, true)
    }

    /// Write the activity row of a command that ended as `reply` says.
    pub(crate) fn record(&self, request: &Value, reply: &Value, elapsed: Duration) -> Result<()> {
        self.write_row(request, reply, elapsed, false)
    }

    /// [`Client::record`] a finished command's row, for tests elsewhere in
    /// the crate.
    #[cfg(test)]
    pub(crate) fn record_for_test(&self, request: &Value, reply: &Value) {
        self.record(request, reply, Duration::ZERO).unwrap();
    }

    fn write_row(
        &self,
        request: &Value,
        reply: &Value,
        elapsed: Duration,
        start: bool,
    ) -> Result<()> {
        let directory = self.output_dir()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join("metadata.lock"))?;
        if !lock.metadata()?.is_file() {
            return Err(Error::new("state", "The activity lock is not a plain file"));
        }
        FileExt::lock_exclusive(&lock)?;

        let path = directory.join("metadata.jsonl");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() > 1024 * 1024) {
            fs::rename(&path, directory.join("metadata.previous.jsonl"))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::new("state", "The activity log is not a plain file"));
        }

        // No input text, argv, coordinates, window titles, output or image
        // payloads in metadata: only what `Doing` names.
        let ok = worked(reply);
        let mut row = json!({
            "at": timestamp().to_string(),
            "operation": request["op"],
            "ok": if start { Value::Null } else { json!(ok) },
            "elapsedMs": elapsed.as_secs_f64() * 1000.0,
            "actionCount": request["actions"].as_array().map(Vec::len),
            "actor": self.actor.as_str(),
        });

        // A failure keeps its fixed code (never its message), so the app can
        // say why in plain words. A refusal for another app in front says
        // whether some of the command was sent before it, and why it was
        // refused.
        if !start
            && !ok
            && let (Some(code), Some(fields)) = (failure_code(reply), row.as_object_mut())
        {
            if code == "focus-changed" {
                let sent = reply["performed"]
                    .as_u64()
                    .is_some_and(|performed| performed > 0);
                fields.insert("partial".into(), json!(sent || reply["partial"] == true));
                if let Some(reason) = focus_reason(reply) {
                    fields.insert("reason".into(), json!(reason));
                }
            }
            fields.insert("code".into(), json!(code));
        }

        // Only an agent's own command names its host session; the person's,
        // through a viewer, never does.
        if self.actor == Actor::Agent
            && let (Some(session), Some(fields)) = (&self.host_session, row.as_object_mut())
        {
            fields.insert("session".into(), json!(session.key()));
        }

        // A task ATLAS_TASK split from its session names its key, never its
        // name; a session's default task adds nothing.
        if self.actor == Actor::Agent
            && let (Some(task), Some(fields)) = (&self.task, row.as_object_mut())
            && task.name().is_some()
        {
            fields.insert("task".into(), json!(task.key()));
        }

        // A collection says how many files and bytes it kept, never which.
        if request["op"] == "collect"
            && let Some(fields) = row.as_object_mut()
        {
            for field in ["count", "bytes"] {
                if let Some(number) = reply[field].as_u64() {
                    fields.insert(field.into(), json!(number));
                }
            }
        }

        // A screenshot names the file it gave: Atlas's own timestamp name,
        // never a path or a person's file.
        if request["op"] == "screenshot"
            && let (Some(capture), Some(fields)) = (reply["capture"].as_str(), row.as_object_mut())
            && crate::helper_info::capture_time(capture).is_some()
        {
            fields.insert("capture".into(), json!(capture));
        }

        if let (Some(doing), Some(fields)) = (&self.doing, row.as_object_mut()) {
            doing.fields(fields, ok);
            if start {
                fields.insert("started".into(), json!(true));
            }
            // Input names the app it landed in, as the helper saw it.
            if !fields.contains_key("app")
                && let Some(app) = reply["app"].as_str()
            {
                fields.insert("app".into(), json!(crate::doing::bounded(app)));
            }
        }

        writeln!(file, "{row}")?;
        Ok(())
    }
}

/// What the helper is asked to expect of an agent's batch, given what its
/// input service `native` says it can check. An input service that answered
/// without the whole check, the Dock's part included and the window's and
/// field's when the batch names them, is refused when the check is
/// required: it would post the input unchecked, or let a click on a Dock
/// icon through, and input cannot be taken back once it is sent. One that
/// did not answer is refused later, as unavailable.
fn expectation(native: &Value, check: Option<Check>) -> Result<Option<Expect>> {
    let offered = has_capability(native, FOCUS_APP);
    let answered = native["ok"] == true;
    match check {
        None => Ok(None),
        Some(Check::Offered(expect)) => Ok(offered.then_some(expect)),
        Some(Check::Required(expect)) if !answered || checks_all(native, &expect) => {
            Ok(Some(expect))
        }
        Some(Check::Required(_)) => Err(Error::new(
            "startup",
            "The helper's input service is from an older Atlas: run atlas start-input, then try again",
        )),
    }
}

/// Whether input service `native` checks all of `expect`: the app, with
/// the Dock, and the window and field when it names them.
fn checks_all(native: &Value, expect: &Expect) -> bool {
    has_capability(native, FOCUS_APP)
        && has_capability(native, FOCUS_DOCK)
        && (expect.window().is_none() || has_capability(native, FOCUS_WINDOW))
        && (expect.field().is_none() || has_capability(native, FOCUS_FIELD))
}

/// The file name of the capture a screenshot's reply `saved` names.
fn capture_name(saved: &Value) -> Option<&str> {
    Path::new(saved["path"].as_str()?).file_name()?.to_str()
}

/// The code a failure is recorded with: `uncertain` when it may have
/// finished in the helper all the same, so the activity never says it
/// didn't happen.
fn recorded_code(error: &Error) -> &'static str {
    if error.uncertain {
        "uncertain"
    } else {
        error.code
    }
}

/// Why a mutation failed, as a fixed code: the helper's error code, or
/// `timeout` and `exit` for a command that ran too long or failed. Only a
/// short lowercase code is kept, so no message text reaches the log.
fn failure_code(reply: &Value) -> Option<String> {
    if let Some(code) = reply["code"].as_str() {
        let fixed = !code.is_empty()
            && code.len() <= 32
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-');
        return fixed.then(|| code.to_owned());
    }
    if reply["timedOut"] == true {
        return Some("timeout".into());
    }
    (!reply["signal"].is_null() || reply["exitCode"].as_i64().is_some_and(|code| code != 0))
        .then(|| "exit".into())
}

/// Why a focus refusal in `reply` was refused, when it is one of the input
/// service's fixed words: another app had keyboard focus (`app`), another
/// app's window was at the point (`point`), another window of the app had
/// focus or was there (`window`), a sheet over it had focus (`sheet`),
/// another field had focus (`field`) or the field was disabled
/// (`disabled`), or Atlas couldn't tell (`unreadable`).
fn focus_reason(reply: &Value) -> Option<&'static str> {
    match reply["reason"].as_str()? {
        "app" => Some("app"),
        "point" => Some("point"),
        "window" => Some("window"),
        "sheet" => Some("sheet"),
        "field" => Some("field"),
        "disabled" => Some("disabled"),
        "unreadable" => Some("unreadable"),
        _ => None,
    }
}

/// Whether a mutation did what it was for: the helper accepted it and, for
/// a command, the command ran to its end and succeeded.
fn worked(reply: &Value) -> bool {
    reply["ok"] == true
        && reply["exitCode"].as_i64().is_none_or(|code| code == 0)
        && reply["timedOut"] != true
        && reply["signal"].is_null()
}

pub fn validate_completion(request: &Value, reply: &Value) -> Result<()> {
    if reply["ok"] != true {
        return Ok(());
    }

    let valid = match request["op"].as_str() {
        // A checked batch is confirmed only with its expectation echoed: a
        // service that ignored it posted the input unchecked.
        Some("input") => {
            reply["performed"].as_u64() == request["actions"].as_array().map(|v| v.len() as u64)
                && (request["expect"].is_null() || reply["expect"] == request["expect"])
        }
        Some("job") => match request["action"].as_str() {
            Some("start") => job_started(request, reply),
            Some("stop") => {
                reply["job"]["id"] == request["id"]
                    && (reply["signalled"] == "SIGTERM"
                        || reply["alreadyEnded"] == true
                        || reply["alreadyStopping"] == true)
            }
            _ => true,
        },
        Some("run") => {
            reply["timedOut"].is_boolean()
                && reply.get("exitCode").is_some()
                && (reply["exitCode"].is_i64()
                    || (reply["exitCode"].is_null()
                        && (reply["timedOut"] == true
                            || reply["signal"]
                                .as_str()
                                .is_some_and(|s| s.starts_with("SIG") && s.len() <= 32))))
        }
        _ => true,
    };

    if valid {
        Ok(())
    } else {
        Err(Error::new("protocol", "Helper did not confirm operation completion").uncertain())
    }
}

/// Whether a job start's `reply` proves the job it started: an ID, a
/// leader that is a group of its own, an exact start time, the boot it
/// started in, the helper account the request was for, and a state. Else
/// a job may run that the person's side knows nothing of.
fn job_started(request: &Value, reply: &Value) -> bool {
    let job = &reply["job"];
    let pid = job["pid"].as_i64();
    let start = job["start"].as_array().map(|start| {
        start.len() == 2
            && start[0].as_u64().is_some_and(|seconds| seconds > 0)
            && start[1].as_u64().is_some_and(|micros| micros < 1_000_000)
    });
    job["id"]
        .as_str()
        .is_some_and(|id| crate::daemon::protocol::JobId::parse(id).is_some())
        && pid.is_some_and(|pid| pid > 1)
        && job["pgid"].as_i64() == pid
        && start == Some(true)
        && job["boot"].as_str().is_some_and(|boot| !boot.is_empty())
        && job["uid"] == request["uid"]
        && matches!(job["state"].as_str(), Some("running" | "ended"))
}

/// The request that runs `argv` in the helper, stopping it after `timeout`:
/// a program and its arguments, none holding a NUL, for 1 second to 10
/// minutes.
fn run_request(argv: Vec<String>, timeout: Duration) -> Result<Value> {
    if argv.is_empty() || argv[0].is_empty() || argv.iter().any(|a| a.contains('\0')) {
        return Err(Error::new(
            "input",
            "Provide a command and arguments after run --",
        ));
    }

    if timeout.is_zero() || timeout > RUN_TIMEOUT_MAX {
        return Err(Error::new(
            "usage",
            "A command may run for 1 second to 10 minutes",
        ));
    }

    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    Ok(json!({"op": "run", "argv": argv, "timeoutMs": timeout_ms}))
}

/// `error` from something that only read: whatever it says, nothing in the
/// helper may have changed.
fn certain(error: Error) -> Error {
    Error {
        uncertain: false,
        ..error
    }
}

/// `reply` as an app list: accepted, with `apps` an array of objects, each
/// with a process ID from 1 to 2³¹−1, `name`, `bundleId` and `executable`
/// strings or null, `active` and `hidden` booleans and `policy` a string
/// when present. A refusal keeps its code; anything else is a protocol
/// failure, never an empty list.
pub(crate) fn app_list(reply: Value) -> Result<Value> {
    if reply["ok"] == false {
        return Err(refusal(&reply));
    }
    let valid = reply["ok"] == true
        && reply["apps"]
            .as_array()
            .is_some_and(|apps| apps.iter().all(listed_app));
    if valid {
        Ok(reply)
    } else {
        Err(Error::new(
            "protocol",
            "The helper's list of running apps was malformed, so Atlas cannot tell what is running",
        ))
    }
}

/// Whether `app` is one entry of an app list, as [`app_list`] says.
fn listed_app(app: &Value) -> bool {
    let Some(fields) = app.as_object() else {
        return false;
    };
    let pid = app["pid"]
        .as_u64()
        .is_some_and(|pid| pid > 0 && i32::try_from(pid).is_ok());
    let text = |key: &str| {
        fields
            .get(key)
            .is_none_or(|value| value.is_null() || value.is_string())
    };
    let flag = |key: &str| fields.get(key).is_none_or(Value::is_boolean);

    pid && ["name", "bundleId", "executable"].into_iter().all(text)
        && ["active", "hidden"].into_iter().all(flag)
        && fields.get("policy").is_none_or(Value::is_string)
}

pub fn png_dimensions(png: &[u8]) -> Result<(u32, u32)> {
    let invalid = || Error::new("capture", "Invalid PNG header or dimensions");

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

/// Whether a process exists, whoever owns it (EPERM still means it exists).
fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };

    matches!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Ok(()) | Err(nix::errno::Errno::EPERM)
    )
}

/// Refuse input work while the helper's Atlas lacks Accessibility; an
/// unknown answer is not a refusal.
fn require_accessibility(status: &Value) -> Result<()> {
    if status["permissions"]["accessibility"] == false
        || status["inputService"]["accessibility"] == false
    {
        return Err(Error::new(
            "permission",
            "The helper does not allow Atlas Accessibility yet, which input needs. In the \
             helper's System Settings › Privacy & Security › Accessibility, turn Atlas on \
             (atlas init guides this).",
        ));
    }

    Ok(())
}

/// Whether a login is seen gone within `limit`: `looked` (no loginwindow
/// and no Dock) true twice in a row, `pause` apart, and then `answers` (its
/// daemon says hello) false. The processes are looked at all the while: a
/// login seen again, even while its daemon still answers, starts the count
/// over. Only silence after two looks that found it gone, and a third look
/// after that silence (the daemon's answer can take a while, and the login
/// can come back meanwhile), ends the wait.
async fn gone_within<L, A>(
    mut looked: impl FnMut() -> L,
    mut answers: impl FnMut() -> A,
    limit: Duration,
    pause: Duration,
) -> bool
where
    L: std::future::Future<Output = bool>,
    A: std::future::Future<Output = bool>,
{
    let gone = async {
        let mut absent = 0;
        loop {
            tokio::time::sleep(pause).await;
            if !looked().await {
                absent = 0;
                continue;
            }

            absent += 1;
            if absent >= 2 && !answers().await {
                if looked().await {
                    return;
                }
                absent = 0;
            }
        }
    };

    tokio::time::timeout(limit, gone).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interrupt::Interrupt;
    use nix::sys::signal::Signal;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::{UnixListener, UnixStream},
    };

    #[test]
    fn pruning_removes_only_old_capture_files() {
        let dir = tempfile::tempdir().unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
        let kept = [
            "notes-capture.png",
            "-capture.png",
            "340282366920938463463374607431768211456-capture.png",
            ".1-capture.png.partial",
        ];
        for name in kept.into_iter().chain(["1-capture.png"]) {
            let file = fs::File::create(dir.path().join(name)).unwrap();
            file.set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        fs::write(dir.path().join("2-capture.png"), b"recent").unwrap();
        std::os::unix::fs::symlink("notes-capture.png", dir.path().join("3-capture.png")).unwrap();
        fs::create_dir(dir.path().join("4-capture.png")).unwrap();
        fs::File::open(dir.path().join("4-capture.png"))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();

        prune_captures(dir.path());

        assert!(!dir.path().join("1-capture.png").exists());
        for name in kept
            .into_iter()
            .chain(["2-capture.png", "3-capture.png", "4-capture.png"])
        {
            assert!(
                fs::symlink_metadata(dir.path().join(name)).is_ok(),
                "{name}"
            );
        }
    }

    /// A native socket and controller confined to a short temporary path.
    fn native_client() -> (tempfile::TempDir, Client, UnixListener, Session) {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        client.config.helper_uid = nix::unistd::getuid().as_raw();
        client.transport = client.input_transport();
        let listener = UnixListener::bind(&client.config.input_socket).unwrap();
        let session = Session {
            epoch: Epoch {
                uid: client.config.helper_uid,
                capture_pid: 123,
                display: Display {
                    width: 100.0,
                    height: 100.0,
                    scale: 1.0,
                },
                security_session_id: 42,
                display_id: 7,
                input_pid: 456,
            },
            capture: true,
            input: true,
        };
        (dir, client, listener, session)
    }

    /// Drive the caller until the stand-in has received a complete request,
    /// leaving its reply under the test's control.
    async fn dispatched<T: std::fmt::Debug>(
        caller: &mut (impl Future<Output = Result<T>> + Unpin),
        listener: &UnixListener,
        operation: &str,
    ) -> BufReader<UnixStream> {
        received(caller, listener, operation).await.0
    }

    /// [`dispatched`], with the request the stand-in received.
    async fn received<T: std::fmt::Debug>(
        caller: &mut (impl Future<Output = Result<T>> + Unpin),
        listener: &UnixListener,
        operation: &str,
    ) -> (BufReader<UnixStream>, Value) {
        tokio::select! {
            outcome = caller => panic!("caller ended before {operation}: {outcome:?}"),
            peer = tokio::time::timeout(Duration::from_secs(2), async {
                let (peer, _) = listener.accept().await.unwrap();
                let mut peer = BufReader::new(peer);
                let mut line = String::new();
                peer.read_line(&mut line).await.unwrap();
                let request = serde_json::from_str::<Value>(&line).unwrap();
                assert_eq!(request["op"], operation);
                (peer, request)
            }) => peer.unwrap(),
        }
    }

    async fn answer(peer: &mut BufReader<UnixStream>, reply: Value) {
        peer.get_mut()
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
    }

    /// Exercise the transaction after session verification, with no native
    /// events: the stand-in acknowledges a press and move without a release.
    async fn standalone_input(client: &Client, session: &Session) -> Result<Value> {
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([
            {"type": "pointer", "phase": "down", "x": 1, "y": 1},
            {"type": "pointer", "phase": "move", "x": 2, "y": 2},
        ]);
        actions::validate(&actions, &session.epoch.display)?;
        client
            .input_in_session(&actions, None, None, None, session)
            .await
    }

    /// One key press, checked against `expect` when given, after session
    /// verification.
    async fn checked_input(
        client: &Client,
        session: &Session,
        expect: Option<Expect>,
    ) -> Result<Value> {
        let _ownership = client.controller.acquire(false)?;
        let actions = json!([{"type": "key", "key": "a"}]);
        client
            .input_in_session(&actions, None, None, expect, session)
            .await
    }

    fn textedit() -> Option<Expect> {
        Some(Expect::new(812).unwrap())
    }

    #[tokio::test]
    async fn the_expectation_is_sent_only_when_asked() {
        for (expect, sent) in [(None, Value::Null), (textedit(), json!({"pid": 812}))] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(checked_input(&client, &session, expect));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(&mut acquire, json!({"ok": true})).await;
            let (mut input, request) = received(&mut caller, &listener, "input").await;
            assert_eq!(request.get("expect").cloned().unwrap_or(Value::Null), sent);
            let mut reply = json!({"ok": true, "performed": 1});
            reply["expect"] = sent;
            answer(&mut input, reply).await;
            let mut release = dispatched(&mut caller, &listener, "release").await;
            answer(&mut release, json!({"ok": true})).await;
            assert_eq!(caller.await.unwrap()["ok"], true);
            assert!(client.controller.pending().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn a_checked_reply_without_its_echo_is_uncertain() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(checked_input(&client, &session, textedit()));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        // A service that ignored the expectation posted the key unchecked.
        answer(&mut input, json!({"ok": true, "performed": 1})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "protocol");
        assert!(error.uncertain);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn a_focus_refusal_releases_and_leaves_no_marker() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(checked_input(&client, &session, textedit()));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        let refusal = json!({"ok": false, "code": "focus-changed", "reason": "app",
            "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
            "performed": 0, "partial": false, "uncertain": false});
        answer(&mut input, refusal.clone()).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        let reply = caller.await.unwrap();
        for field in ["ok", "code", "reason", "found", "performed", "uncertain"] {
            assert_eq!(reply[field], refusal[field], "{field}");
        }
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    /// A verified status of helper account 7001 whose input service lists
    /// `capabilities`: enough for input to go ahead without asking again.
    fn verified_status(capabilities: &[&str]) -> Value {
        let proof = json!({"ok": true, "uid": 7001, "hasGraphicAccess": true, "parentPID": 123,
            "onConsole": false, "securitySessionID": 42, "displayID": 7, "width": 100,
            "height": 100});
        json!({
            "ok": true,
            "user": {"uid": 7001},
            "daemon": {"pid": 123},
            "session": {"managerName": "Aqua", "onConsole": false},
            "display": {"width": 100, "height": 100, "scale": 1},
            "permissions": {"screenCapture": true, "accessibility": true},
            "sessionProof": proof,
            "inputService": {"ok": true, "protocolVersion": 1, "pid": 456, "ready": true,
                "accessibility": true, "session": proof, "capabilities": capabilities},
        })
    }

    /// Send one key under `check` given `status`, and whether the input
    /// service's socket was connected to at all.
    ///
    /// The helper account must differ from the one running the test, so
    /// the stand-in socket (served by the test's own account) fails the
    /// peer check of any connection: a connection is the evidence that
    /// something would have been sent, and none is the evidence that
    /// nothing was.
    async fn connected(status: &Value, check: Option<Check>) -> (Result<Value>, bool) {
        let (_dir, mut client, listener, _) = native_client();
        client.config.helper_uid = 7001;
        let actions = json!([{"type": "key", "key": "a"}]);
        let _ownership = client.controller.acquire(false).unwrap();
        let (outcome, accepted) = tokio::join!(
            client.input_with_status(status, &actions, None, None, check),
            tokio::time::timeout(Duration::from_millis(500), listener.accept()),
        );
        assert!(
            client.controller.pending().unwrap().is_empty(),
            "no marker is left"
        );
        (outcome, accepted.is_ok())
    }

    #[tokio::test]
    async fn an_older_input_service_is_refused_before_anything_is_sent() {
        let older = verified_status(&[SHUTDOWN, HOLD]);
        let (outcome, sent) = connected(&older, textedit().map(Check::Required)).await;
        let refused = outcome.unwrap_err();
        assert_eq!(refused.code, "startup");
        assert!(!refused.uncertain);
        assert!(
            refused.error.contains("atlas start-input"),
            "{}",
            refused.error
        );
        assert!(!sent, "neither acquire nor input reached the service");

        // The same status goes on to the service when nothing needs the
        // check, or when the service can make it.
        let current = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK]);
        for (status, check) in [
            (&older, None),
            (&older, textedit().map(Check::Offered)),
            (&current, textedit().map(Check::Required)),
        ] {
            let (_, sent) = connected(status, check.clone()).await;
            assert!(sent, "{check:?}");
        }
    }

    /// A service from before the Dock counted as covering checks the app
    /// but lets a click on a Dock icon through: it is given no input that
    /// needs the check.
    #[tokio::test]
    async fn an_input_service_without_the_dock_check_receives_no_checked_input() {
        let undocked = verified_status(&[SHUTDOWN, HOLD, FOCUS_APP]);
        let (outcome, sent) = connected(&undocked, textedit().map(Check::Required)).await;
        let refused = outcome.unwrap_err();
        assert_eq!(refused.code, "startup");
        assert!(!refused.uncertain);
        assert!(refused.error.contains("older Atlas"), "{}", refused.error);
        assert!(
            refused.error.contains("atlas start-input"),
            "{}",
            refused.error
        );
        assert!(!sent, "neither acquire nor input reached the service");

        // A named click is judged against the Dock before it is sent, and
        // input that needs no check goes on as before.
        for check in [None, textedit().map(Check::Offered)] {
            let (_, sent) = connected(&undocked, check.clone()).await;
            assert!(sent, "{check:?}");
        }
    }

    #[test]
    fn a_named_click_is_checked_only_by_a_service_that_can() {
        let older = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD]});
        let current = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK]});
        let undocked = json!({"ok": true, "capabilities": [SHUTDOWN, HOLD, FOCUS_APP]});
        assert_eq!(
            expectation(&undocked, Some(Check::Required(textedit().unwrap())))
                .unwrap_err()
                .code,
            "startup"
        );
        let unanswered = json!({"ok": false, "code": "transport"});
        let pid = textedit().unwrap();

        assert_eq!(
            expectation(&older, Some(Check::Offered(pid.clone()))).unwrap(),
            None
        );
        assert_eq!(
            expectation(&unanswered, Some(Check::Offered(pid.clone()))).unwrap(),
            None
        );
        assert_eq!(
            expectation(&current, Some(Check::Offered(pid.clone()))).unwrap(),
            Some(pid.clone())
        );

        assert_eq!(
            expectation(&older, Some(Check::Required(pid.clone())))
                .unwrap_err()
                .code,
            "startup"
        );
        assert_eq!(
            expectation(&current, Some(Check::Required(pid.clone()))).unwrap(),
            Some(pid.clone())
        );
        // A service that did not answer is refused as unavailable later.
        assert_eq!(
            expectation(&unanswered, Some(Check::Required(pid.clone()))).unwrap(),
            Some(pid.clone())
        );
        assert_eq!(expectation(&older, None).unwrap(), None);
    }

    #[tokio::test]
    async fn a_focus_refusal_keeps_its_code_on_every_native_path() {
        let (_dir, client, listener, _) = native_client();
        let mut caller = Box::pin(client.native_control("input", "owner"));
        let mut request = dispatched(&mut caller, &listener, "input").await;
        answer(
            &mut request,
            json!({"ok": false, "code": "focus-changed", "error": "Calculator has keyboard focus"}),
        )
        .await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "focus-changed");
        assert!(!error.uncertain);
    }

    #[test]
    fn a_checked_input_is_confirmed_only_with_its_expectation_echoed() {
        let unchecked = json!({"op": "input", "actions": [{}, {}]});
        let mut checked = unchecked.clone();
        checked["expect"] = json!({"pid": 812});

        for (request, reply, confirmed) in [
            (&unchecked, json!({"ok": true, "performed": 2}), true),
            (&unchecked, json!({"ok": true, "performed": 1}), false),
            (
                &checked,
                json!({"ok": true, "performed": 2, "expect": {"pid": 812}}),
                true,
            ),
            (&checked, json!({"ok": true, "performed": 2}), false),
            (
                &checked,
                json!({"ok": true, "performed": 2, "expect": {"pid": 9}}),
                false,
            ),
            (
                &checked,
                json!({"ok": false, "code": "focus-changed"}),
                true,
            ),
        ] {
            let outcome = validate_completion(request, &reply);
            assert_eq!(outcome.is_ok(), confirmed, "{request} {reply}");
            if let Err(error) = outcome {
                assert_eq!(error.code, "protocol");
                assert!(error.uncertain);
            }
        }
    }

    /// `acquire` once the lock is free: a child another test is starting
    /// may hold the lock's file a moment past its release, until it runs
    /// its own program. Any other refusal is the answer.
    fn once_free(acquire: impl Fn() -> Result<Ownership>) -> Result<Ownership> {
        let held = |error: &Error| {
            error.code == "busy"
                && error.holder == Some(crate::error::Holder::Agent)
                && !error.details.contains_key("running")
        };
        for _ in 0..50 {
            match acquire() {
                Err(error) if held(&error) => std::thread::sleep(Duration::from_millis(20)),
                taken => return taken,
            }
        }
        acquire()
    }

    fn recovery_settles_the_abandoned_claim(client: &Client) {
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        assert_eq!(client.controller.unresolved().unwrap().len(), 1);
        assert_eq!(
            once_free(|| client.controller.acquire(false))
                .unwrap_err()
                .code,
            "uncertain"
        );
        let ownership = once_free(|| client.controller.acquire(true)).unwrap();
        client.controller.clear().unwrap();
        drop(ownership);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(once_free(|| client.controller.acquire(false)).is_ok());
    }

    #[tokio::test]
    async fn a_release_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let _release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap(), generation);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn a_successful_release_settles_the_standalone_input_claim() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap(), generation);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn standalone_input_with_an_uncertain_outcome_stays_pending_until_recovery() {
        // Recovery takes the controller lock, which a spawn on another test
        // thread can hold in its child until exec.
        if !crate::transaction_test::isolated(
            "client::tests::standalone_input_with_an_uncertain_outcome_stays_pending_until_recovery",
        ) {
            return;
        }
        for (input_reply, release_reply) in [
            (json!({"ok": true, "performed": 2}), None),
            (
                json!({"ok": false, "uncertain": true, "error": "unknown"}),
                Some(json!({"ok": true})),
            ),
            (
                json!({"ok": true, "performed": 2}),
                Some(json!({"ok": false, "uncertain": true, "error": "unknown"})),
            ),
        ] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(standalone_input(&client, &session));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(&mut acquire, json!({"ok": true})).await;
            let mut input = dispatched(&mut caller, &listener, "input").await;
            answer(&mut input, input_reply).await;
            let mut release = dispatched(&mut caller, &listener, "release").await;
            assert_eq!(client.controller.pending().unwrap().len(), 1);
            if let Some(reply) = release_reply {
                answer(&mut release, reply).await;
            }
            drop(release);
            let outcome = caller.await;
            assert!(
                outcome
                    .as_ref()
                    .map_or_else(|error| error.uncertain, |reply| reply["uncertain"] == true)
            );
            recovery_settles_the_abandoned_claim(&client);
        }
    }

    /// A grace the tests wait out.
    const GRACE: Duration = Duration::from_millis(500);

    /// Well within [`GRACE`].
    const SOON: Duration = Duration::from_millis(50);

    /// `client`'s controller stopped by a stop of its own, which the test
    /// asks.
    fn stoppable(client: &mut Client) -> Interrupt {
        let interrupt = Interrupt::new(GRACE);
        client.controller = client.controller.clone().interrupted_by(interrupt.clone());
        interrupt
    }

    /// `client` with a controller of its own on the same state, as another
    /// agent's command has.
    fn another(client: &Client, dir: &Path) -> Client {
        let mut other = client.clone();
        other.controller = Controller::at(60001, &dir.join("state"));
        other
    }

    #[tokio::test]
    async fn a_stop_before_native_acquisition_starts_nothing() {
        let (_dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let _ownership = client.controller.acquire(false).unwrap();
        interrupt.ask(Signal::SIGTERM);

        let actions = json!([{"type": "key", "key": "a"}]);
        let error = client
            .input_in_session(&actions, None, None, None, &session)
            .await
            .unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(!error.uncertain);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err(),
            "the input service heard nothing"
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_stop_while_acquisition_is_unanswered_waits_for_it_then_posts_nothing() {
        let (_dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        interrupt.ask(Signal::SIGTERM);
        answer(&mut acquire, json!({"ok": true})).await;

        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(!error.uncertain, "nothing was posted");
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_stop_while_input_is_under_way_lets_the_batch_finish_and_settles_it() {
        let (_dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        interrupt.ask(Signal::SIGTERM);
        answer(&mut input, json!({"ok": true, "performed": 2})).await;

        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn input_still_unanswered_after_the_grace_stays_pending_for_every_agent() {
        // Recovery takes the controller lock, which a spawn on another test
        // thread can hold in its child until exec.
        if !crate::transaction_test::isolated(
            "client::tests::input_still_unanswered_after_the_grace_stays_pending_for_every_agent",
        ) {
            return;
        }
        let (dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        interrupt.ask(Signal::SIGTERM);

        // Past the grace, its release is still sent whole, but not waited
        // for.
        let error = tokio::time::timeout(GRACE * 4, caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(error.uncertain);
        let mut line = String::new();
        assert_eq!(
            input.read_line(&mut line).await.unwrap(),
            0,
            "the input service sees the batch cut off"
        );
        let (release, _) = listener.accept().await.unwrap();
        BufReader::new(release).read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["op"],
            "release"
        );
        recovery_settles_the_abandoned_claim(&another(&client, dir.path()));
    }

    #[tokio::test]
    async fn a_stop_while_release_is_unanswered_waits_for_it() {
        let (_dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        interrupt.ask(Signal::SIGTERM);
        tokio::time::sleep(SOON).await;

        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_release_still_unanswered_after_the_grace_stays_pending() {
        if !crate::transaction_test::isolated(
            "client::tests::a_release_still_unanswered_after_the_grace_stays_pending",
        ) {
            return;
        }
        let (dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let _release = dispatched(&mut caller, &listener, "release").await;
        interrupt.ask(Signal::SIGTERM);
        let asked = Instant::now();

        let error = tokio::time::timeout(GRACE * 4, caller)
            .await
            .unwrap()
            .unwrap_err();
        assert!(asked.elapsed() >= GRACE, "cut at the end of the grace");
        assert_eq!(error.code, "interrupted");
        assert!(error.uncertain);
        recovery_settles_the_abandoned_claim(&another(&client, dir.path()));
    }

    #[tokio::test]
    async fn an_acquisition_still_unanswered_after_the_grace_stays_pending() {
        if !crate::transaction_test::isolated(
            "client::tests::an_acquisition_still_unanswered_after_the_grace_stays_pending",
        ) {
            return;
        }
        let (dir, mut client, listener, session) = native_client();
        let interrupt = stoppable(&mut client);
        let mut caller = Box::pin(standalone_input(&client, &session));
        let _acquire = dispatched(&mut caller, &listener, "acquire").await;
        interrupt.ask(Signal::SIGTERM);

        // The release that follows an unknown acquisition is cut at once,
        // so it stays unknown too.
        let error = tokio::time::timeout(GRACE * 4, caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(error.uncertain);
        let other = another(&client, dir.path());
        assert!(!other.controller.pending().unwrap().is_empty());
        assert_eq!(
            other.controller.acquire(false).unwrap_err().code,
            "uncertain"
        );
        let recovery = other.controller.acquire(true).unwrap();
        other.controller.clear().unwrap();
        drop(recovery);
        assert!(other.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn a_stop_while_a_request_waits_to_be_sent_sends_nothing_and_leaves_nothing() {
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, interrupt) = running(dir.path(), client);
        let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(crate::control::Lease::new(
            Duration::from_secs(10),
        )));
        let token = lease.lock().await.acquire(&acting.controller).unwrap();
        let authorizing = lease.lock().await;
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
        let mut caller = Box::pin(acting.mutate(run, Some((&lease, &token)), None, None));
        assert!(
            tokio::time::timeout(SOON, &mut caller).await.is_err(),
            "it waits for the lease"
        );
        interrupt.ask(Signal::SIGTERM);
        drop(authorizing);

        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(!error.uncertain, "nothing was sent");
        let (peer, _) = helper.accept().await.unwrap();
        let mut line = String::new();
        assert_eq!(
            BufReader::new(peer).read_line(&mut line).await.unwrap(),
            0,
            "the helper got no request"
        );
        assert!(acting.controller.pending().unwrap().is_empty());
    }

    /// An agent's `run` against a helper stand-in kept in `dir`, its
    /// listener, and the stop its controller heeds.
    fn running(dir: &Path, mut client: Client) -> (Client, UnixListener, Interrupt) {
        client.transport.path = dir.join("helper.sock");
        let helper = UnixListener::bind(&client.transport.path).unwrap();
        let interrupt = stoppable(&mut client);
        (client.doing(Doing::new("run")), helper, interrupt)
    }

    #[tokio::test]
    async fn a_run_that_ends_within_the_grace_is_settled() {
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, interrupt) = running(dir.path(), client);
        let _ownership = acting.controller.acquire(false).unwrap();
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
        let mut caller = Box::pin(acting.mutate(run, None, None, None));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        interrupt.ask(Signal::SIGTERM);
        tokio::time::sleep(SOON).await;

        answer(
            &mut peer,
            json!({"ok": true, "exitCode": 0, "timedOut": false}),
        )
        .await;
        assert_eq!(caller.await.unwrap()["exitCode"], 0);
        assert!(acting.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_run_still_going_after_the_grace_is_cut_and_stays_pending() {
        if !crate::transaction_test::isolated(
            "client::tests::a_run_still_going_after_the_grace_is_cut_and_stays_pending",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, interrupt) = running(dir.path(), client);
        let ownership = acting.controller.acquire(false).unwrap();
        let run = json!({"op": "run", "argv": ["sleep", "30"], "timeoutMs": 60_000});
        let mut caller = Box::pin(acting.mutate(run, None, None, None));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        interrupt.ask(Signal::SIGTERM);

        let error = tokio::time::timeout(GRACE * 5, caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(error.uncertain);
        let mut line = String::new();
        assert_eq!(
            peer.read_line(&mut line).await.unwrap(),
            0,
            "the helper sees the connection close, which ends the command"
        );
        let log = fs::read_to_string(acting.output_dir().unwrap().join("metadata.jsonl")).unwrap();
        let ended: Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        assert_eq!(ended["ok"], false);
        assert_eq!(ended["code"], "uncertain", "never recorded as not done");
        drop(ownership);
        recovery_settles_the_abandoned_claim(&another(&acting, dir.path()));
    }

    #[tokio::test]
    async fn a_restart_stopped_before_its_request_settles_only_its_own_claim() {
        let (_dir, mut client, _input, _session) = native_client();
        let interrupt = stoppable(&mut client);
        let earlier = client.controller.quarantine().unwrap();
        let unknown = vec![earlier.generation().to_owned()];
        drop(earlier);
        let claim = client.controller.quarantine().unwrap();
        interrupt.ask(Signal::SIGTERM);

        let outcome = client
            .mutate(json!({"op": "restart"}), None, None, None)
            .await;
        let error = client.settle_restart(&claim, outcome).unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(!error.uncertain, "the restart was never sent");
        drop(claim);
        assert_eq!(client.controller.pending().unwrap(), unknown);
    }

    #[tokio::test]
    async fn a_grant_another_command_left_is_waited_out_within_the_wait() {
        let (_dir, mut client, listener, session) = native_client();
        client.controller = client.controller.waiting(Duration::from_secs(2));
        let mut caller = Box::pin(standalone_input(&client, &session));
        for _ in 0..2 {
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(
                &mut acquire,
                json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
            )
            .await;
        }
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        answer(&mut release, json!({"ok": true})).await;
        assert_eq!(caller.await.unwrap()["ok"], true);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_grant_the_person_holds_is_never_waited_for() {
        let (_dir, mut client, listener, session) = native_client();
        client.controller = client.controller.waiting(Duration::from_secs(2));
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(
            &mut acquire,
            json!({"ok": false, "code": "busy", "holder": "person", "error": "held"}),
        )
        .await;
        let error = tokio::time::timeout(Duration::from_secs(1), caller)
            .await
            .expect("answered after one try")
            .unwrap_err();
        assert_eq!(error.code, "busy");
        assert_eq!(error.holder, Some(Holder::Person));
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_controller_that_does_not_wait_tries_the_grant_once() {
        for wait in [None, Some(Duration::from_millis(1))] {
            let (_dir, mut client, listener, session) = native_client();
            if let Some(wait) = wait {
                // A wait already spent on the lock leaves none for the grant.
                client.controller = client.controller.waiting(wait);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let mut caller = Box::pin(standalone_input(&client, &session));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(
                &mut acquire,
                json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
            )
            .await;
            let error = tokio::time::timeout(Duration::from_secs(1), caller)
                .await
                .expect("answered after one try")
                .unwrap_err();
            assert_eq!(error.code, "busy");
            assert_eq!(error.holder, Some(Holder::Agent));
            assert!(client.controller.pending().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn a_grant_refusal_that_may_have_happened_or_names_no_agent_is_never_retried() {
        for reply in [
            json!({"ok": false, "code": "busy", "holder": "agent", "uncertain": true, "error": "?"}),
            json!({"ok": false, "code": "busy", "error": "held"}),
            json!({"ok": false, "code": "lease-lost", "error": "lost"}),
        ] {
            let (_dir, mut client, listener, session) = native_client();
            client.controller = client.controller.waiting(Duration::from_secs(2));
            let mut caller = Box::pin(standalone_input(&client, &session));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            answer(&mut acquire, reply.clone()).await;
            if reply["uncertain"] == true {
                // The grant may be held: it is released, not tried again.
                let mut release = dispatched(&mut caller, &listener, "release").await;
                answer(&mut release, json!({"ok": true})).await;
            }
            let error = tokio::time::timeout(Duration::from_secs(1), caller)
                .await
                .expect("answered after one try")
                .unwrap_err();
            assert_eq!(error.code, reply["code"], "{reply}");
        }
    }

    #[tokio::test]
    async fn a_saved_stop_ends_the_wait_for_a_grant_before_another_try() {
        let (_dir, mut client, listener, session) = native_client();
        client.controller = client.controller.waiting(Duration::from_secs(2));
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(
            &mut acquire,
            json!({"ok": false, "code": "busy", "holder": "agent", "error": "held"}),
        )
        .await;
        // The person stops agents; the service has not heard of it.
        client.controller.note_stop(true).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(1), caller)
            .await
            .expect("no other try")
            .unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(!error.uncertain);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "no acquire or input after the Stop"
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_input_whose_native_release_is_definitely_refused_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
        answer(&mut acquire, json!({"ok": true})).await;
        let mut input = dispatched(&mut caller, &listener, "input").await;
        answer(&mut input, json!({"ok": true, "performed": 2})).await;
        let mut release = dispatched(&mut caller, &listener, "release").await;
        let generation = client.controller.pending().unwrap();
        assert_eq!(generation.len(), 1);
        answer(
            &mut release,
            json!({"ok": false, "code": "lease-lost", "error": "refused"}),
        )
        .await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "uncertain");
        assert!(error.uncertain);
        assert_eq!(client.controller.pending().unwrap(), generation);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn releasing_after_a_lost_acquisition_reply_keeps_the_acquisition_pending() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(standalone_input(&client, &session));
        let acquire = dispatched(&mut caller, &listener, "acquire").await;
        drop(acquire);
        let mut release = dispatched(&mut caller, &listener, "release").await;
        assert_eq!(client.controller.pending().unwrap().len(), 2);
        assert_eq!(client.controller.unresolved().unwrap().len(), 1);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        answer(&mut release, json!({"ok": true})).await;
        assert!(caller.await.unwrap_err().uncertain);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acquisition_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
        let _acquire = dispatched(&mut caller, &listener, "acquire").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acquisition_settles_only_an_acknowledged_outcome() {
        for reply in [
            json!({"ok": true}),
            json!({"ok": false, "code": "busy", "error": "refused"}),
            json!({"ok": false, "uncertain": true, "error": "unknown"}),
        ] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            assert_eq!(client.controller.pending().unwrap().len(), 1);
            answer(&mut acquire, reply.clone()).await;
            assert_eq!(caller.await.is_ok(), reply["ok"] == true);
            if reply["uncertain"] == true {
                recovery_settles_the_abandoned_claim(&client);
            } else {
                assert!(client.controller.pending().unwrap().is_empty());
                assert!(client.controller.acquire(false).is_ok());
            }
        }
    }

    #[tokio::test]
    async fn a_busy_input_service_says_who_holds_control() {
        for (holder, expected) in [
            (json!("person"), Some(Holder::Person)),
            (json!("agent"), Some(Holder::Agent)),
            (json!("someone"), None),
            (Value::Null, None),
        ] {
            let (_dir, client, listener, session) = native_client();
            let mut caller = Box::pin(client.acquire_input("owner", &session.epoch));
            let mut acquire = dispatched(&mut caller, &listener, "acquire").await;
            let reply = json!({"ok": false, "code": "busy", "error": "refused", "holder": holder});
            answer(&mut acquire, reply).await;
            let error = caller.await.unwrap_err();
            assert_eq!(error.code, "busy");
            assert_eq!(error.holder, expected, "{holder}");
        }
    }

    /// A status reply sufficient for repinning preflight. It deliberately
    /// lacks the independent proof required by later session verification.
    fn repinnable_status(session: &Session) -> Value {
        json!({"ok": true, "inputService": {
            "ok": true, "protocolVersion": 1, "accessibility": true,
            "enforcedEpoch": session.epoch.input_epoch(),
        }})
    }

    #[tokio::test]
    async fn missing_input_with_permissions_granted_reports_a_startup_failure() {
        let (_dir, client, listener, session) = native_client();
        let status = json!({
            "ok": true,
            "permissions": {"screenCapture": true, "accessibility": true},
            "inputService": {"ok": false, "accessibility": true},
        });
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status).await;
        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "startup");
        assert!(!error.uncertain);
        assert!(client.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn repinning_with_accessibility_revoked_reports_a_permission_failure() {
        for source in ["permissions", "inputService"] {
            let (_dir, client, listener, session) = native_client();
            let mut status = repinnable_status(&session);
            status[source]["accessibility"] = json!(false);
            let mut caller = Box::pin(client.repin(session));
            let mut hello = dispatched(&mut caller, &listener, "hello").await;
            answer(&mut hello, status).await;
            assert_eq!(caller.await.unwrap_err().code, "permission", "{source}");
            assert!(client.controller.pending().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn a_display_reset_interrupted_after_dispatch_stays_pending_until_recovery() {
        let (_dir, client, listener, session) = native_client();
        let status = repinnable_status(&session);
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status).await;
        let _reset = dispatched(&mut caller, &listener, "reset").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        drop(caller);
        recovery_settles_the_abandoned_claim(&client);
    }

    #[tokio::test]
    async fn an_acknowledged_display_reset_is_settled_before_session_verification() {
        let (_dir, client, listener, session) = native_client();
        let status = repinnable_status(&session);
        let mut caller = Box::pin(client.repin(session));
        let mut hello = dispatched(&mut caller, &listener, "hello").await;
        answer(&mut hello, status.clone()).await;
        let mut reset = dispatched(&mut caller, &listener, "reset").await;
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        answer(&mut reset, json!({"ok": true, "released": true})).await;
        let mut verifying = dispatched(&mut caller, &listener, "hello").await;
        assert!(client.controller.pending().unwrap().is_empty());
        answer(&mut verifying, status).await;
        assert_eq!(caller.await.unwrap_err().code, "identity");
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());
    }

    #[test]
    fn an_interrupted_setup_can_repair_its_probe_without_settling_pending_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        let previous = client.controller.quarantine().unwrap();
        let generation = previous.generation().to_owned();
        drop(previous);
        assert!(!client.config.identity_probe.exists());
        assert!(client.install().is_err());
        let competing = client.controller.acquire(true).unwrap();
        assert!(client.bootstrap_probe().is_err());
        assert!(!client.config.identity_probe.exists());
        drop(competing);
        client.bootstrap_probe().unwrap();
        assert!(client.config.identity_probe.is_file());
        assert_eq!(
            file_sha256(&client.config.identity_probe).unwrap(),
            file_sha256(&std::env::current_exe().unwrap()).unwrap()
        );
        assert_eq!(
            client.controller.pending().unwrap(),
            std::slice::from_ref(&generation)
        );
        assert!(client.controller.acquire(false).is_err());
        fs::write(&client.config.identity_probe, b"interrupted copy").unwrap();
        client.bootstrap_probe().unwrap();
        assert_eq!(
            file_sha256(&client.config.identity_probe).unwrap(),
            file_sha256(&std::env::current_exe().unwrap()).unwrap()
        );
        assert_eq!(client.controller.pending().unwrap(), [generation]);
        let owned = client.controller.acquire(true).unwrap();
        client.controller.clear().unwrap();
        drop(owned);
        assert!(client.controller.acquire(false).is_ok());
    }

    #[test]
    fn a_successful_recovery_allows_the_next_normal_acquisition() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = Controller::at(60001, &dir.path().join("state"));
        let ownership = client.controller.acquire(true).unwrap();
        drop(client.controller.quarantine().unwrap());
        let claim = client.controller.quarantine().unwrap();
        // The verified recovery clears abandoned work while retaining its
        // own live claim, which the restart success path must then settle.
        client.controller.clear().unwrap();
        client
            .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
            .unwrap();
        drop(claim);
        drop(ownership);
        assert!(client.controller.pending().unwrap().is_empty());
        assert!(client.controller.acquire(false).is_ok());

        let other = client.controller.quarantine().unwrap();
        let claim = client.controller.quarantine().unwrap();
        client.controller.clear().unwrap();
        client
            .settle_restart(&claim, Ok(json!({"ok": true, "restarted": true})))
            .unwrap();
        drop(claim);
        assert_eq!(client.controller.pending().unwrap(), [other.generation()]);
        assert!(client.controller.acquire(false).is_err());
    }

    /// A client for a configuration kept in `dir`.
    fn client_in(dir: &Path) -> Client {
        let text = "helper_uid = 60001\nsocket_path = \"c.sock\"\noutput_dir = \"out\"\n\
                    identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
        let config = Config::parse(text.as_bytes(), &dir.join("u.toml")).unwrap();
        Client::new(config).unwrap()
    }

    /// The rows `client` wrote to its activity log, oldest first.
    fn rows(client: &Client) -> Vec<Value> {
        fs::read_to_string(client.output_dir().unwrap().join("metadata.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn labelled(client: &Client) -> Client {
        let mut labelled = client.clone().doing(Doing::new("click"));
        labelled.host_session = crate::host_session::HostSession::new(
            crate::host_session::Host::Claude,
            "d9031471-5e2a",
        );
        labelled
    }

    #[test]
    fn an_agent_row_carries_its_host_session_key() {
        let dir = tempfile::tempdir().unwrap();
        let client = labelled(&client_in(dir.path()));
        let key = client.host_session.as_ref().unwrap().key().clone();
        client.record_for_test(
            &json!({"op": "input", "actions": [{}]}),
            &json!({"ok": true}),
        );
        assert_eq!(rows(&client)[0]["session"], key.as_str());

        let mut viewer = client.clone();
        viewer.actor = Actor::Viewer;
        viewer.record_for_test(
            &json!({"op": "input", "actions": [{}]}),
            &json!({"ok": true}),
        );
        assert!(
            rows(&client)[1].get("session").is_none(),
            "the person's never does"
        );
    }

    #[test]
    fn a_default_task_adds_nothing_to_a_row_and_a_named_one_adds_only_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let session = labelled(&client_in(dir.path()));
        let key = session.host_session.as_ref().unwrap().key().clone();
        let mut default = session.clone();
        default.task = Some(Task::of(&key, None).unwrap());
        let mut named = session.clone();
        named.task = Some(Task::of(&key, Some("settings-sheet")).unwrap());
        let request = json!({"op": "input", "actions": [{}]});
        for client in [&session, &default, &named] {
            client.record_for_test(&request, &json!({"ok": true}));
        }

        let [plain, by_default, by_name] = <[Value; 3]>::try_from(rows(&session)).unwrap();
        let fields = |row: &Value| row.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(fields(&by_default), fields(&plain));
        let task = named.task.as_ref().unwrap().key();
        assert_eq!(by_name["task"], task.as_str());
        assert_ne!(task, &key);
        let line =
            fs::read_to_string(session.output_dir().unwrap().join("metadata.jsonl")).unwrap();
        assert!(!line.contains("settings-sheet"), "never the name");
    }

    #[test]
    fn a_row_with_no_host_session_is_as_before() {
        let dir = tempfile::tempdir().unwrap();
        let client = labelled(&client_in(dir.path()));
        let mut plain = client.clone();
        plain.host_session = None;
        let request = json!({"op": "input", "actions": [{}]});
        client.record_for_test(&request, &json!({"ok": true}));
        plain.record_for_test(&request, &json!({"ok": true}));

        let [mut labelled, unlabelled] = <[Value; 2]>::try_from(rows(&client)).unwrap();
        labelled.as_object_mut().unwrap().remove("session");
        let fields = |row: &Value| row.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(fields(&labelled), fields(&unlabelled));
        assert_eq!(
            fields(&unlabelled),
            [
                "actionCount",
                "actor",
                "at",
                "command",
                "elapsedMs",
                "ok",
                "operation",
                "verb"
            ]
        );
    }

    #[test]
    fn a_screenshot_row_names_its_capture_file_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let client = labelled(&client_in(dir.path()));
        let name = "1761660429000123456-capture.png";
        let saved =
            json!({"ok": true, "path": format!("/Users/me/runs/{name}"), "unchanged": true});
        assert_eq!(
            capture_name(&saved),
            Some(name),
            "an unchanged capture names the earlier file"
        );

        let request = json!({"op": "screenshot"});
        for capture in [
            json!(name),
            json!("/Users/me/runs/1-capture.png"),
            json!("notes.png"),
        ] {
            client.record_for_test(&request, &json!({"ok": true, "capture": capture}));
        }
        let rows = rows(&client);
        assert_eq!(rows[0]["capture"], name);
        assert!(rows[1].get("capture").is_none() && rows[2].get("capture").is_none());
        assert!(!rows.iter().any(|row| row.to_string().contains("/Users")));
    }

    #[test]
    fn a_refusal_for_another_app_in_front_says_whether_some_was_sent() {
        let dir = tempfile::tempdir().unwrap();
        let client = labelled(&client_in(dir.path()));
        let request = json!({"op": "input", "actions": [{}, {}]});
        for reply in [
            json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": false}),
            json!({"ok": false, "code": "focus-changed", "performed": 1, "partial": false}),
            json!({"ok": false, "code": "focus-changed", "performed": 0, "partial": true}),
            json!({"ok": false, "code": "busy"}),
        ] {
            client.record_for_test(&request, &reply);
        }
        let partial: Vec<Value> = rows(&client)
            .iter()
            .map(|row| row["partial"].clone())
            .collect();
        assert_eq!(
            partial,
            [json!(false), json!(true), json!(true), Value::Null]
        );
    }

    #[test]
    fn a_focus_refusals_row_keeps_why_it_was_refused_in_the_services_own_words_only() {
        let dir = tempfile::tempdir().unwrap();
        let client = labelled(&client_in(dir.path()));
        let request = json!({"op": "input", "actions": [{}]});
        for reason in [
            json!("app"),
            json!("point"),
            json!("unreadable"),
            json!("window"),
            json!("sheet"),
            json!("field"),
            json!("disabled"),
            json!("Calculator has keyboard focus"),
            Value::Null,
        ] {
            client.record_for_test(
                &request,
                &json!({"ok": false, "code": "focus-changed", "reason": reason, "performed": 0}),
            );
        }
        client.record_for_test(
            &request,
            &json!({"ok": false, "code": "busy", "reason": "app"}),
        );
        let reasons: Vec<Value> = rows(&client)
            .iter()
            .map(|row| row["reason"].clone())
            .collect();
        assert_eq!(
            reasons,
            [
                json!("app"),
                json!("point"),
                json!("unreadable"),
                json!("window"),
                json!("sheet"),
                json!("field"),
                json!("disabled"),
                Value::Null,
                Value::Null,
                Value::Null,
            ]
        );
    }

    #[test]
    fn an_expectation_is_sent_only_to_a_service_that_names_the_capability() {
        let app = Expect::new(812).unwrap();
        let window = app.clone().in_window(4120).unwrap();
        let field = app.clone().in_field("Email").unwrap();
        let both = window.clone().in_field("Email").unwrap();
        let service = |extra: &[&str]| {
            let mut capabilities = vec![SHUTDOWN, HOLD, FOCUS_APP, FOCUS_DOCK];
            capabilities.extend_from_slice(extra);
            json!({"ok": true, "capabilities": capabilities})
        };
        let sent = |native: &Value, expect: &Expect| {
            expectation(native, Some(Check::Required(expect.clone())))
                .map(|sent| sent == Some(expect.clone()))
        };

        let app_only = service(&[]);
        assert!(sent(&app_only, &app).unwrap());
        for refused in [&window, &field, &both] {
            assert_eq!(
                sent(&app_only, refused).unwrap_err().code,
                "startup",
                "{refused:?}"
            );
        }

        let windows = service(&[FOCUS_WINDOW]);
        assert!(sent(&windows, &window).unwrap());
        assert_eq!(sent(&windows, &field).unwrap_err().code, "startup");
        assert_eq!(sent(&windows, &both).unwrap_err().code, "startup");

        let all = service(&[FOCUS_WINDOW, FOCUS_FIELD]);
        for expect in [&app, &window, &field, &both] {
            assert!(sent(&all, expect).unwrap(), "{expect:?}");
        }
    }

    /// A login is gone only once it is seen gone twice in a row and its
    /// daemon stopped answering; a look in between that sees it again
    /// starts the count over.
    #[tokio::test]
    async fn a_login_is_gone_only_when_seen_gone_twice_and_silent() {
        use std::{cell::RefCell, collections::VecDeque};

        let pause = Duration::from_millis(1);
        let limit = Duration::from_secs(5);
        let run = |looks: &[bool], hellos: &[bool]| {
            let looks = RefCell::new(looks.iter().copied().collect::<VecDeque<_>>());
            let hellos = RefCell::new(hellos.iter().copied().collect::<VecDeque<_>>());
            let seen = RefCell::new(0);
            async move {
                let gone = gone_within(
                    || {
                        *seen.borrow_mut() += 1;
                        let look = looks.borrow_mut().pop_front().unwrap_or(false);
                        async move { look }
                    },
                    || {
                        let hello = hellos.borrow_mut().pop_front().unwrap_or(false);
                        async move { hello }
                    },
                    limit,
                    pause,
                )
                .await;
                (gone, seen.into_inner())
            }
        };

        assert_eq!(run(&[true, true, true], &[]).await, (true, 3));
        assert_eq!(
            run(&[true, false, true, true, true], &[]).await,
            (true, 5),
            "a login seen again starts the count over"
        );
        assert_eq!(
            run(&[true, true, true, true, true], &[true, true]).await,
            (true, 5),
            "waits for the daemon to go quiet, looking all the while"
        );
        assert_eq!(
            run(&[true, true, false, true, true, true], &[true, false]).await,
            (true, 6),
            "a login back after the second look starts the count over"
        );
        assert_eq!(
            run(&[true, true, false, true, true, true], &[false, false]).await,
            (true, 6),
            "a login back while the daemon was asked starts the count over"
        );

        // Back after the second look, then its daemon goes quiet: still
        // logged in, never gone.
        let back = Duration::from_millis(50);
        let looks = std::cell::Cell::new(0);
        let hellos = std::cell::Cell::new(0);
        let returned = gone_within(
            || {
                looks.set(looks.get() + 1);
                let look = looks.get() <= 2;
                async move { look }
            },
            || {
                hellos.set(hellos.get() + 1);
                let hello = hellos.get() == 1;
                async move { hello }
            },
            back,
            pause,
        )
        .await;
        assert!(!returned, "a login that came back is not gone");

        // It logs in again while the daemon is still being asked, and the
        // daemon then says nothing: still logged in.
        let again = std::cell::Cell::new(false);
        let returned = gone_within(
            || {
                let look = !again.get();
                async move { look }
            },
            || {
                again.set(true);
                async move {
                    tokio::time::sleep(pause).await;
                    false
                }
            },
            back,
            pause,
        )
        .await;
        assert!(!returned, "back while its daemon was asked");

        let short = Duration::from_millis(50);
        let stays = gone_within(|| async { false }, || async { false }, short, pause).await;
        assert!(!stays, "a login that stays is not gone");
        let answering = gone_within(|| async { true }, || async { true }, short, pause).await;
        assert!(!answering, "a daemon that keeps answering is not gone");
    }

    #[tokio::test]
    async fn an_input_change_stays_pending_only_while_its_outcome_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        assert_eq!(
            client.pending_until_known(async { Ok(1) }).await.unwrap(),
            1
        );

        let refused = Error::new("native", "refused");
        assert!(
            client
                .pending_until_known(async { Err::<(), _>(refused) })
                .await
                .is_err()
        );
        assert!(
            client.controller.pending().unwrap().is_empty(),
            "known outcomes clear"
        );

        let lost = Error::new("transport", "reply lost").uncertain();
        assert!(
            client
                .pending_until_known(async { Err::<(), _>(lost) })
                .await
                .is_err()
        );
        assert_eq!(
            client.controller.unresolved().unwrap().len(),
            1,
            "an unknown one stays"
        );
    }

    #[test]
    fn process_existence_ignores_ownership() {
        assert!(process_exists(std::process::id()));
        assert!(process_exists(1), "launchd exists though owned by root");
        assert!(!process_exists(u32::MAX));
    }

    #[tokio::test]
    async fn only_the_person_resumes_agents() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        assert_eq!(client.hold(false).await.unwrap_err().code, "stopped");
    }

    #[tokio::test]
    async fn a_stop_holds_on_the_persons_side_while_the_helpers_input_is_down() {
        let dir = tempfile::tempdir().unwrap();
        let mut person = client_in(dir.path());
        person.controller = crate::control::Controller::at(60001, &dir.path().join("state"));
        person.actor = Actor::Viewer;
        let mut agent = client_in(dir.path());
        agent.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        let stopped = person.hold(true).await.unwrap();
        assert_eq!(stopped["held"], true);
        assert!(
            stopped["warnings"][0]
                .as_str()
                .unwrap()
                .contains("isn't running")
        );
        assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");
        assert!(
            tokio::time::timeout(Duration::from_secs(2), agent.stopped_while_running())
                .await
                .is_ok()
        );
        let acting = agent.clone().doing(Doing::new("run"));
        let refused = acting
            .mutate(json!({"op": "run", "argv": ["true"]}), None, None, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(!refused.uncertain, "nothing started");

        let marker = person.config.input_socket.with_extension("stopped");
        fs::write(&marker, b"").unwrap();
        let refused = person.hold(false).await.unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(person.controller.stop_noted());
        assert!(marker.exists());

        fs::remove_file(marker).unwrap();
        person.hold(false).await.unwrap();
        assert!(agent.require_going().await.is_ok(), "resumed");
    }

    #[tokio::test]
    async fn a_running_command_hears_of_a_stop_from_its_saved_marker() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        let marker = client.config.input_socket.with_extension("stopped");

        let waiting =
            tokio::time::timeout(Duration::from_millis(500), client.stopped_while_running());
        assert!(waiting.await.is_err(), "no stop, no end");

        std::fs::write(&marker, b"").unwrap();
        let heard = tokio::time::timeout(Duration::from_secs(2), client.stopped_while_running());
        assert!(heard.await.is_ok(), "the stop is heard within a moment");
    }

    #[tokio::test]
    async fn a_local_stop_cancels_a_running_command_and_refuses_its_next_dispatch() {
        let (dir, mut client, input, _session) = native_client();
        drop(input);
        client.transport.path = dir.path().join("helper.sock");
        let helper = UnixListener::bind(&client.transport.path).unwrap();
        let mut person = client.clone();
        person.actor = Actor::Viewer;
        let acting = client.doing(Doing::new("run"));
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});
        let ownership = acting.controller.acquire(false).unwrap();
        let mut caller = Box::pin(acting.mutate(run.clone(), None, None, None));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        let pending = acting.controller.pending().unwrap();
        assert_eq!(pending.len(), 1);

        let stopped = person.hold(true).await.unwrap();
        assert_eq!(stopped["ok"], true);
        assert_eq!(stopped["held"], true);
        assert!(
            !acting
                .config
                .input_socket
                .with_extension("stopped")
                .exists()
        );
        let error = tokio::time::timeout(Duration::from_secs(2), caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(error.uncertain);
        assert_eq!(acting.controller.pending().unwrap(), pending);
        assert_eq!(acting.controller.acquire(true).unwrap_err().code, "busy");
        let mut line = String::new();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut line))
                .await
                .unwrap()
                .unwrap(),
            0,
            "the helper sees the cancelled connection"
        );

        let error = acting.mutate(run, None, None, None).await.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(!error.uncertain, "the next command never starts");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), helper.accept())
                .await
                .is_err()
        );
        drop(ownership);
        recovery_settles_the_abandoned_claim(&acting);
    }

    /// Drive `caller` for a moment past sending its request, as it waits
    /// for the reply.
    async fn waits<T: std::fmt::Debug>(caller: &mut (impl Future<Output = Result<T>> + Unpin)) {
        assert!(
            tokio::time::timeout(SOON, caller).await.is_err(),
            "it still waits for its reply"
        );
    }

    /// A command an agent started with `run`, which the stand-in leaves
    /// unanswered until the test answers it.
    fn command() -> Value {
        json!({"op": "run", "argv": ["sleep", "30"], "timeoutMs": 60_000})
    }

    #[tokio::test]
    async fn a_run_lets_go_of_control_once_its_request_is_sent() {
        if !crate::transaction_test::isolated(
            "client::tests::a_run_lets_go_of_control_once_its_request_is_sent",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        waits(&mut caller).await;

        drop(other.controller.acquire_beside_commands().unwrap());
        drop(other.controller.acquire_for_person().unwrap());
        let refused = other.controller.acquire(true).unwrap_err();
        assert_eq!(refused.code, "busy");
        assert_eq!(refused.holder, Some(Holder::Agent));
        assert_eq!(other.controller.acquire(false).unwrap_err().code, "busy");
        assert!(other.controller.pending_causes().unwrap()[0].command);

        answer(
            &mut peer,
            json!({"ok": true, "exitCode": 0, "timedOut": false}),
        )
        .await;
        assert_eq!(caller.await.unwrap()["exitCode"], 0);
        assert!(other.controller.pending().unwrap().is_empty());
        assert!(other.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn a_run_refused_before_it_is_sent_settles_under_control() {
        if !crate::transaction_test::isolated(
            "client::tests::a_run_refused_before_it_is_sent_settles_under_control",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        // A lease of a helper of its own holds the request back before it
        // is sent, while the run keeps its control.
        let lease: SharedLease = Arc::new(tokio::sync::Mutex::new(crate::control::Lease::new(
            Duration::from_secs(10),
        )));
        let elsewhere = Controller::at(60001, &dir.path().join("elsewhere"));
        let token = lease.lock().await.acquire(&elsewhere).unwrap();
        let authorizing = lease.lock().await;
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller =
            Box::pin(acting.mutate(command(), Some((&lease, &token)), None, Some(ownership)));
        assert!(tokio::time::timeout(SOON, &mut caller).await.is_err());
        assert_eq!(
            other.controller.acquire_beside_commands().unwrap_err().code,
            "busy",
            "control is still held"
        );
        interrupt.ask(Signal::SIGTERM);
        drop(authorizing);

        let error = caller.await.unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(!error.uncertain, "nothing was sent");
        let (peer, _) = helper.accept().await.unwrap();
        let mut line = String::new();
        assert_eq!(
            BufReader::new(peer).read_line(&mut line).await.unwrap(),
            0,
            "the helper got no request"
        );
        assert!(other.controller.pending().unwrap().is_empty());
        assert!(other.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn a_stopped_run_stays_pending_beside_other_agents() {
        if !crate::transaction_test::isolated(
            "client::tests::a_stopped_run_stays_pending_beside_other_agents",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let mut person = acting.clone();
        person.actor = Actor::Viewer;
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        waits(&mut caller).await;
        let pending = other.controller.pending().unwrap();

        person.hold(true).await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(error.uncertain);
        let mut line = String::new();
        assert_eq!(
            peer.read_line(&mut line).await.unwrap(),
            0,
            "the helper sees the connection close, which ends the command"
        );
        assert_eq!(other.controller.pending().unwrap(), pending);
        assert_eq!(other.controller.unresolved().unwrap(), pending);
        assert_eq!(
            other.controller.acquire_beside_commands().unwrap_err().code,
            "uncertain"
        );
        recovery_settles_the_abandoned_claim(&other);
    }

    #[tokio::test]
    async fn a_lost_run_reply_blocks_every_agent() {
        if !crate::transaction_test::isolated("client::tests::a_lost_run_reply_blocks_every_agent")
        {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
        let peer = dispatched(&mut caller, &helper, "run").await;
        waits(&mut caller).await;

        drop(peer);
        let error = caller.await.unwrap_err();
        assert!(error.uncertain, "{error:?}");
        for refused in [
            other.controller.acquire_beside_commands(),
            other.controller.acquire_for_person(),
            other.controller.acquire(false),
        ] {
            assert_eq!(refused.unwrap_err().code, "uncertain");
        }
        recovery_settles_the_abandoned_claim(&other);
    }

    #[tokio::test]
    async fn a_lost_or_uncertain_run_reply_is_unknown_before_its_row_is_written() {
        if !crate::transaction_test::isolated(
            "client::tests::a_lost_or_uncertain_run_reply_is_unknown_before_its_row_is_written",
        ) {
            return;
        }
        for lost in [true, false] {
            lost_before_its_row(lost).await;
        }
    }

    /// A run that let go of control ends with its reply `lost`, or with
    /// one saying its outcome is unknown, while another command writes its
    /// row: everyone hears it is unknown before its own row is written.
    async fn lost_before_its_row(lost: bool) {
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let output = acting.output_dir().unwrap().to_owned();
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        // Writing a row blocks its thread, so the run has one of its own.
        let caller = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(acting.mutate(command(), None, None, Some(ownership)))
        });
        let (peer, _) = tokio::time::timeout(Duration::from_secs(2), helper.accept())
            .await
            .unwrap()
            .unwrap();
        let mut peer = BufReader::new(peer);
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let started = Instant::now();
        while !fs::read_to_string(output.join("metadata.jsonl")).is_ok_and(|log| !log.is_empty()) {
            assert!(started.elapsed() < Duration::from_secs(2), "it began");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let activity = fs::File::create(output.join("metadata.lock")).unwrap();
        FileExt::lock_exclusive(&activity).unwrap();
        if lost {
            drop(peer);
        } else {
            answer(
                &mut peer,
                json!({"ok": false, "code": "restart", "error": "The helper stopped", "uncertain": true}),
            )
            .await;
        }
        let started = Instant::now();
        loop {
            match other.controller.acquire_beside_commands() {
                Err(error) if error.code == "uncertain" => break,
                _ => {
                    assert!(started.elapsed() < Duration::from_secs(2), "never unknown");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
        assert!(!caller.is_finished(), "its row still waits");
        FileExt::unlock(&activity).unwrap();

        match caller.join().unwrap() {
            Err(error) => assert!(lost && error.uncertain, "{error:?}"),
            Ok(reply) => assert!(!lost && reply["uncertain"] == true, "{reply}"),
        }
        assert_eq!(rows(&other).pop().unwrap()["ok"], false);
        recovery_settles_the_abandoned_claim(&other);
    }

    #[tokio::test]
    async fn a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone() {
        if !crate::transaction_test::isolated(
            "client::tests::a_run_that_let_go_and_is_cut_after_the_grace_stays_pending_for_everyone",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller = Box::pin(acting.mutate(command(), None, None, Some(ownership)));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        waits(&mut caller).await;
        interrupt.ask(Signal::SIGTERM);

        let error = tokio::time::timeout(GRACE * 5, caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "interrupted");
        assert!(error.uncertain);
        let mut line = String::new();
        assert_eq!(
            peer.read_line(&mut line).await.unwrap(),
            0,
            "the helper sees the connection close, which ends the command"
        );
        for refused in [
            other.controller.acquire_beside_commands(),
            other.controller.acquire_for_person(),
        ] {
            assert_eq!(refused.unwrap_err().code, "uncertain");
        }
        recovery_settles_the_abandoned_claim(&other);
    }

    #[tokio::test]
    async fn two_runs_go_on_beside_each_other() {
        if !crate::transaction_test::isolated("client::tests::two_runs_go_on_beside_each_other") {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let second = another(&acting, dir.path());
        let other = another(&acting, dir.path());

        let first_control = acting.controller.acquire_beside_commands().unwrap();
        let mut first = Box::pin(acting.mutate(command(), None, None, Some(first_control)));
        let mut first_peer = dispatched(&mut first, &helper, "run").await;
        waits(&mut first).await;
        let second_control = second.controller.acquire_beside_commands().unwrap();
        let mut beside = Box::pin(second.mutate(command(), None, None, Some(second_control)));
        let mut second_peer = dispatched(&mut beside, &helper, "run").await;
        waits(&mut beside).await;

        let causes = other.controller.pending_causes().unwrap();
        assert_eq!(causes.len(), 2);
        assert!(causes.iter().all(|cause| cause.command));
        drop(other.controller.acquire_beside_commands().unwrap());
        assert_eq!(other.controller.acquire(true).unwrap_err().code, "busy");

        let ended = json!({"ok": true, "exitCode": 0, "timedOut": false});
        answer(&mut second_peer, ended.clone()).await;
        beside.await.unwrap();
        assert_eq!(other.controller.pending().unwrap().len(), 1);
        answer(&mut first_peer, ended).await;
        first.await.unwrap();
        assert!(other.controller.pending().unwrap().is_empty());
        assert!(other.controller.acquire(false).is_ok());
    }

    #[tokio::test]
    async fn input_goes_on_beside_a_running_command() {
        if !crate::transaction_test::isolated(
            "client::tests::input_goes_on_beside_a_running_command",
        ) {
            return;
        }
        let (dir, client, input, _session) = native_client();
        drop(input);
        let running = another(&client, dir.path());

        let command = running.controller.quarantine_command().unwrap();
        let actions = json!([{"type": "key", "key": "a"}]);
        let beside = client.input(&actions, None, None).await.unwrap_err();
        assert!(
            !["busy", "uncertain"].contains(&beside.code),
            "only the helper refuses it: {beside:?}"
        );
        running.controller.clear_if(&command).unwrap();
        drop(command);

        let change = running.controller.quarantine().unwrap();
        let refused = client.input(&actions, None, None).await.unwrap_err();
        assert_eq!(refused.code, "uncertain", "an unmarked change still blocks");
        running.controller.clear_if(&change).unwrap();
    }

    #[tokio::test]
    async fn other_commands_keep_control_for_their_whole_run() {
        let (dir, client, input, _session) = native_client();
        drop(input);
        let (acting, helper, _interrupt) = running(dir.path(), client);
        let other = another(&acting, dir.path());
        let ownership = acting.controller.acquire_beside_commands().unwrap();
        let mut caller = Box::pin(acting.mutate(command(), None, None, None));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        waits(&mut caller).await;

        assert_eq!(
            other.controller.acquire_beside_commands().unwrap_err().code,
            "busy",
            "the caller keeps control"
        );
        assert!(!other.controller.pending_causes().unwrap()[0].command);

        answer(
            &mut peer,
            json!({"ok": true, "exitCode": 0, "timedOut": false}),
        )
        .await;
        caller.await.unwrap();
        drop(ownership);
        assert!(other.controller.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_local_stop_succeeds_and_cancels_a_running_command_when_native_hold_fails() {
        let (dir, mut client, input, _session) = native_client();
        client.transport.path = dir.path().join("helper.sock");
        let helper = UnixListener::bind(&client.transport.path).unwrap();
        let mut person = client.clone();
        person.actor = Actor::Viewer;
        let acting = client.doing(Doing::new("run"));
        let ownership = acting.controller.acquire(false).unwrap();
        let mut caller = Box::pin(acting.mutate(
            json!({"op": "run", "argv": ["true"], "timeoutMs": 1000}),
            None,
            None,
            None,
        ));
        let mut peer = dispatched(&mut caller, &helper, "run").await;
        let pending = acting.controller.pending().unwrap();
        assert_eq!(pending.len(), 1);

        let mut stopping = Box::pin(person.hold(true));
        let mut hello = dispatched(&mut stopping, &input, "hello").await;
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut stopping, &input, "hold").await;
        answer(
            &mut hold,
            json!({"ok": false, "error": "Cleanup failed", "uncertain": true}),
        )
        .await;
        assert_eq!(
            stopping.await.unwrap(),
            json!({
                "ok": true,
                "held": true,
                "kept": true,
                "uncertain": true,
                "warnings": ["The helper's input service may or may not have been told. Agents are stopped here all the same."],
            })
        );
        assert!(person.controller.stop_noted());
        let error = tokio::time::timeout(Duration::from_secs(2), caller)
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(error.uncertain);
        assert_eq!(acting.controller.pending().unwrap(), pending);
        assert_eq!(acting.controller.acquire(true).unwrap_err().code, "busy");
        let mut line = String::new();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), peer.read_line(&mut line))
                .await
                .unwrap()
                .unwrap(),
            0,
            "the helper sees the cancelled connection"
        );
        drop(ownership);
        recovery_settles_the_abandoned_claim(&acting);
    }

    #[tokio::test]
    async fn a_stop_returns_ok_with_the_native_problem_when_native_hold_fails() {
        let (_dir, mut person, input, _session) = native_client();
        person.actor = Actor::Viewer;
        let mut stopping = Box::pin(person.hold(true));
        let mut hello = dispatched(&mut stopping, &input, "hello").await;
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut stopping, &input, "hold").await;
        answer(
            &mut hold,
            json!({"ok": false, "error": "Cleanup failed", "uncertain": false}),
        )
        .await;

        assert_eq!(
            stopping.await.unwrap(),
            json!({
                "ok": true,
                "held": true,
                "kept": true,
                "warnings": ["The helper's input service reported a problem while stopping: Cleanup failed. Agents are stopped here all the same."],
            })
        );
        assert!(person.controller.stop_noted());
    }

    #[tokio::test]
    async fn a_stop_returns_ok_with_uncertainty_and_a_warning_when_native_hold_is_uncertain() {
        let (_dir, mut person, input, _session) = native_client();
        person.actor = Actor::Viewer;
        let mut stopping = Box::pin(person.hold(true));
        let mut hello = dispatched(&mut stopping, &input, "hello").await;
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut stopping, &input, "hold").await;
        answer(
            &mut hold,
            json!({"ok": false, "error": "Cleanup failed", "uncertain": true}),
        )
        .await;

        assert_eq!(
            stopping.await.unwrap(),
            json!({
                "ok": true,
                "held": true,
                "kept": true,
                "uncertain": true,
                "warnings": ["The helper's input service may or may not have been told. Agents are stopped here all the same."],
            })
        );
        assert!(person.controller.stop_noted());
    }

    #[tokio::test]
    async fn resume_keeps_the_local_stop_until_the_helpers_stop_can_be_cleared() {
        let (_dir, client, listener, _session) = native_client();
        let mut person = client;
        person.actor = Actor::Viewer;
        let marker = person.config.input_socket.with_extension("stopped");
        let mut stopping = Box::pin(person.hold(true));
        let mut hello = dispatched(&mut stopping, &listener, "hello").await;
        answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
        let mut hold = dispatched(&mut stopping, &listener, "hold").await;
        fs::write(&marker, b"").unwrap();
        answer(&mut hold, json!({"ok": true, "held": true, "kept": true})).await;
        assert_eq!(stopping.await.unwrap()["held"], true);
        drop(listener);

        let error = person.hold(false).await.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(
            error
                .error
                .contains("Recover input before agents can resume")
        );
        assert!(person.controller.stop_noted());
        assert!(marker.exists());
        let mut agent = person.clone();
        agent.actor = Actor::Agent;
        assert_eq!(agent.require_going().await.unwrap_err().code, "stopped");

        fs::remove_file(marker).unwrap();
        assert_eq!(person.hold(false).await.unwrap()["held"], false);
        assert!(!person.controller.stop_noted());
    }

    #[tokio::test]
    async fn resume_clears_the_local_stop_only_after_native_acknowledgment() {
        for acknowledged in [false, true] {
            let (_dir, client, listener, _session) = native_client();
            let mut person = client;
            person.actor = Actor::Viewer;
            person.controller.note_stop(true).unwrap();
            let mut resuming = Box::pin(person.hold(false));
            let mut hello = dispatched(&mut resuming, &listener, "hello").await;
            assert!(person.controller.stop_noted());
            answer(&mut hello, json!({"ok": true, "capabilities": ["hold"]})).await;
            let mut hold = dispatched(&mut resuming, &listener, "hold").await;
            assert!(person.controller.stop_noted());
            answer(
                &mut hold,
                json!({"ok": acknowledged, "code": "native", "error": "refused", "held": false}),
            )
            .await;
            assert_eq!(resuming.await.is_ok(), acknowledged);
            assert_eq!(person.controller.stop_noted(), !acknowledged);
        }
    }

    #[tokio::test]
    async fn a_stopped_agents_command_is_refused_before_it_starts_and_a_read_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path());
        std::fs::write(client.config.input_socket.with_extension("stopped"), b"").unwrap();
        let run = json!({"op": "run", "argv": ["true"], "timeoutMs": 1000});

        let acting = client.clone().doing(Doing::new("run"));
        let refused = acting
            .mutate(run.clone(), None, None, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code, "stopped");
        assert!(!refused.uncertain, "nothing started");

        let reading = client.mutate(run, None, None, None).await.unwrap_err();
        assert_ne!(
            reading.code, "stopped",
            "a read goes on (and finds no helper here)"
        );
    }

    #[test]
    fn input_names_the_app_it_landed_in_and_a_command_its_start() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("type"));
        let typed = json!({"op": "input", "actions": [{"type": "type", "text": "x"}]});
        client
            .record(
                &typed,
                &json!({"ok": true, "app": "TextEdit"}),
                Duration::ZERO,
            )
            .unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(crate::doing::says(&rows[0]), "typed in TextEdit");

        let client = client.doing(Doing::new("open").app(Some("Calculator")));
        let run = json!({"op": "run", "argv": ["open", "-a", "Calculator"]});
        client.record_start(&run).unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(crate::doing::says(&rows[0]), "opening Calculator…");
        client
            .record(&run, &json!({"ok": true, "exitCode": 0}), Duration::ZERO)
            .unwrap();
        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(
            crate::doing::says(&rows[0]),
            "opened Calculator",
            "the end stands for the start"
        );
        assert_eq!(rows.len(), 2, "one row per command");
    }

    #[test]
    fn a_command_that_failed_or_timed_out_is_recorded_as_failed() {
        assert!(worked(&json!({"ok": true})));
        assert!(worked(
            &json!({"ok": true, "exitCode": 0, "timedOut": false})
        ));
        assert!(!worked(&json!({"ok": true, "exitCode": 1})));
        assert!(!worked(
            &json!({"ok": true, "exitCode": 0, "timedOut": true})
        ));
        assert!(!worked(&json!({"ok": false})));
    }

    #[test]
    fn a_command_terminated_by_a_signal_is_recorded_as_an_exit_failure() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("run"));
        let request = json!({"op": "run", "argv": ["tool"]});
        let reply = json!({"ok": true, "exitCode": null, "timedOut": false, "signal": "SIGTERM"});
        validate_completion(&request, &reply).unwrap();
        client.record_start(&request).unwrap();
        client.record(&request, &reply, Duration::ZERO).unwrap();

        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["ok"], false);
        assert_eq!(rows[0]["code"], "exit");
    }

    fn display() -> Display {
        Display {
            width: 1280.0,
            height: 720.0,
            scale: 2.0,
        }
    }

    fn region(x: i32, y: i32, width: i32, height: i32) -> Region {
        Region {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_crop_maps_pixels_to_points_from_its_origin() {
        let shown = region(300, 200, 800, 600);
        let ((x, y), scale) = placement(&display(), 400, Some(&shown));

        assert_eq!(((x, y), scale), ((300.0, 200.0), 2.0));
        assert_eq!((x + 10.0 * scale, y + 20.0 * scale), (320.0, 240.0));
        assert_eq!(
            placement(&display(), 1600, Some(&shown)),
            ((300.0, 200.0), 0.5)
        );
    }

    #[test]
    fn a_full_screenshot_starts_at_the_corner_with_todays_scale() {
        assert_eq!(placement(&display(), 2560, None), ((0.0, 0.0), 0.5));
        assert_eq!(placement(&display(), 640, None), ((0.0, 0.0), 2.0));
        assert_eq!(
            cropped(&json!({"ok": true}), None, &display()).unwrap(),
            None
        );
    }

    #[test]
    fn an_older_helper_that_ignores_the_crop_is_refused() {
        let whole = json!({"ok": true, "png": "", "width": 2560, "height": 1440, "scale": 2.0, "offDisplayWindows": []});

        for crop in [Crop::App { pid: 42 }, Crop::Region(region(0, 0, 400, 300))] {
            let refused = cropped(&whole, Some(&crop), &display()).unwrap_err();
            assert_eq!(refused.code, "daemon", "{crop:?}");
            assert!(refused.error.contains("atlas restart"), "{crop:?}");
        }

        let mut null = whole.clone();
        null["region"] = Value::Null;
        assert_eq!(
            cropped(&null, Some(&Crop::App { pid: 42 }), &display())
                .unwrap_err()
                .code,
            "daemon"
        );
    }

    #[test]
    fn a_crop_other_than_the_one_asked_for_is_refused() {
        let reply = |region: Value| json!({"ok": true, "region": region});
        let asked = Crop::Region(region(100, 100, 400, 300));

        let fits = reply(json!({"x": 100, "y": 100, "width": 400, "height": 300}));
        assert_eq!(
            cropped(&fits, Some(&asked), &display()).unwrap(),
            Some(Cropped {
                region: region(100, 100, 400, 300),
                app: Value::Null,
                overlapping: Value::Null,
            })
        );
        let clamped = reply(json!({"x": 100, "y": 100, "width": 100, "height": 50}));
        assert!(cropped(&clamped, Some(&asked), &display()).is_ok());

        for sent in [
            json!({"x": 90, "y": 100, "width": 400, "height": 300}),
            json!({"x": 100, "y": 100, "width": 401, "height": 300}),
            json!({"x": -1, "y": 0, "width": 10, "height": 10}),
            json!({"x": 1200, "y": 0, "width": 100, "height": 10}),
            json!({"x": 0, "y": 0, "width": 0, "height": 10}),
            json!({"x": 100.5, "y": 100, "width": 10, "height": 10}),
            json!({"x": 100, "y": 100}),
            json!("100,100,400,300"),
        ] {
            let refused = cropped(&reply(sent.clone()), Some(&asked), &display()).unwrap_err();
            assert_eq!(refused.code, "protocol", "{sent}");
        }

        let app = Crop::App { pid: 42 };
        let shown = |pid: i64, overlapping: Value| {
            json!({
                "ok": true,
                "region": {"x": 0, "y": 0, "width": 10, "height": 10},
                "app": {"pid": pid, "name": "TextEdit"},
                "overlapping": overlapping,
            })
        };
        let found = cropped(&shown(42, json!([])), Some(&app), &display())
            .unwrap()
            .unwrap();
        assert_eq!(found.app, json!({"pid": 42, "name": "TextEdit"}));
        assert_eq!(found.overlapping, json!([]));
        for (pid, overlapping) in [(43, json!([])), (42, Value::Null), (42, json!({}))] {
            let refused = cropped(&shown(pid, overlapping), Some(&app), &display()).unwrap_err();
            assert_eq!(refused.code, "protocol", "{pid}");
        }
    }

    #[test]
    fn only_the_standard_size_asks_for_one_pixel_per_point() {
        assert_eq!(
            screenshot_request(Size::Standard),
            json!({"op": "screenshot", "maxDimension": SCREENSHOT_SIDE, "perPoint": true})
        );
        assert_eq!(
            screenshot_request(Size::Longest(SCREENSHOT_SIDE)),
            json!({"op": "screenshot", "maxDimension": SCREENSHOT_SIDE})
        );
        assert_eq!(
            screenshot_request(Size::Longest(800)),
            json!({"op": "screenshot", "maxDimension": 800})
        );
        assert_eq!(screenshot_request(Size::Full), json!({"op": "screenshot"}));
    }

    #[test]
    fn a_crop_nobody_asked_for_is_refused() {
        let reply = json!({"ok": true, "region": {"x": 0, "y": 0, "width": 10, "height": 10}});
        let refused = cropped(&reply, None, &display()).unwrap_err();

        assert_eq!(
            (refused.code, refused.error.as_str()),
            ("protocol", "The helper sent a crop that was not asked for")
        );
    }

    #[test]
    fn a_region_must_meet_the_display() {
        assert!(meets(&region(-50, -50, 200, 200), &display()));
        assert!(meets(&region(1279, 719, 10, 10), &display()));
        assert!(!meets(&region(5000, 5000, 10, 10), &display()));
        assert!(!meets(&region(1280, 0, 10, 10), &display()));
        assert!(!meets(&region(-10, 0, 10, 10), &display()));
    }

    #[tokio::test]
    async fn a_screenshot_is_recorded_as_the_agent_looking_and_a_failed_one_as_failed() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("screenshot"));
        assert!(
            client.capture(Size::Standard, None, false).await.is_err(),
            "no helper answers"
        );
        let region = Crop::Region(Region {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        });
        assert!(
            client
                .capture(Size::Longest(400), Some(&region), true)
                .await
                .is_err()
        );

        let rows = crate::overview::activity(client.output_dir().unwrap(), 5);
        assert_eq!(
            (&rows[0]["operation"], &rows[0]["ok"], &rows[0]["actor"]),
            (&json!("screenshot"), &json!(false), &json!("agent"))
        );
        assert_eq!(crate::doing::says(&rows[0]), "couldn't take a screenshot");
        let log = fs::read_to_string(client.output_dir().unwrap().join("metadata.jsonl")).unwrap();
        let logged: Vec<Value> = log
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(logged.len(), 2);
        for row in &logged {
            assert_eq!(
                (&row["operation"], &row["ok"], &row["request"]),
                (&json!("screenshot"), &json!(false), &Value::Null),
                "a crop is logged as a plain screenshot"
            );
        }
        assert!(!log.contains("region"), "{log}");
    }

    #[tokio::test]
    async fn a_status_read_without_a_viewer_says_no_viewer_has_control() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path());
        client.controller = crate::control::Controller::at(60001, &dir.path().join("state"));

        let status = crate::overview::read(&client).await;
        assert_eq!(status["controlActive"], false);
        assert_eq!(status["overview"]["state"], "notReady", "no helper answers");
        assert_eq!(status["overview"]["label"], "Not ready");
        assert!(
            !status["overview"]["headline"]
                .as_str()
                .unwrap()
                .ends_with('.')
        );
    }

    #[test]
    fn a_failure_keeps_its_fixed_code_and_reads_as_plain_words_but_never_its_message() {
        let dir = tempfile::tempdir().unwrap();
        let click = json!({"op": "input", "actions": [{"type": "click"}]});
        let refused =
            json!({"ok": false, "code": "elements", "error": "No element is named secret"});
        let odd = json!({"ok": false, "code": "Not A Code: secret"});
        let failed = json!({"ok": true, "exitCode": 2});

        // One client per action: rows of one command read as its newest step.
        for reply in [&refused, &odd, &failed] {
            client_in(dir.path())
                .doing(Doing::new("click").app(Some("Opie")))
                .record(&click, reply, Duration::ZERO)
                .unwrap();
        }

        let client = client_in(dir.path());
        let directory = client.output_dir().unwrap();
        let log = std::fs::read_to_string(directory.join("metadata.jsonl")).unwrap();
        assert!(!log.contains("secret"), "{log}");
        let rows = crate::overview::activity(directory, 5);
        let reasons = rows
            .iter()
            .map(|row| crate::overview::shown(row, 0)["reason"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            reasons,
            [
                json!("The command failed"),
                Value::Null,
                json!("Nothing on screen matched")
            ]
        );
    }

    #[test]
    fn recorded_rows_name_the_actor_and_what_was_done_but_never_typed_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = client_in(dir.path()).doing(Doing::new("type").app(Some("TextEdit")));
        let typed = json!({"op": "input", "actions": [{"type": "type", "text": "secret"}]});
        let ok = json!({"ok": true});

        client
            .record(&typed, &ok, Duration::from_millis(5))
            .unwrap();
        client.doing = None;
        client.actor = Actor::Viewer;
        client
            .record(&json!({"op": "restart"}), &ok, Duration::ZERO)
            .unwrap();

        let directory = client.output_dir().unwrap();
        let rows = crate::overview::activity(directory, 5);
        assert_eq!(
            (&rows[0]["actor"], &rows[0]["operation"]),
            (&json!("viewer"), &json!("restart"))
        );
        assert_eq!(
            (&rows[1]["actor"], &rows[1]["actionCount"]),
            (&json!("agent"), &json!(1))
        );
        assert_eq!(
            (&rows[1]["verb"], &rows[1]["app"]),
            (&json!("type"), &json!("TextEdit"))
        );
        assert!(
            rows[0].get("verb").is_none(),
            "a row says only what it knows"
        );

        let log = fs::read_to_string(directory.join("metadata.jsonl")).unwrap();
        assert!(!log.contains("secret"), "no typed text is recorded");
    }

    #[test]
    fn recording_refuses_non_files_without_waiting_for_a_reader() {
        const WORKER: &str = "ATLAS_TEST_METADATA_DIRECTORY";
        if let Some(directory) = std::env::var_os(WORKER) {
            let client = client_in(Path::new(&directory));
            assert!(client.record_start(&json!({"op": "run"})).is_err());
            return;
        }

        let mut refused = Vec::new();
        for name in ["metadata.lock", "metadata.jsonl"] {
            for kind in [
                "fifo",
                "fifo with reader",
                "directory",
                "link",
                "large link",
            ] {
                let dir = tempfile::tempdir().unwrap();
                let client = client_in(dir.path());
                let output = client.output_dir().unwrap();
                let path = output.join(name);
                let target = output.join("personal-file");
                let contents = if kind == "large link" {
                    vec![b'x'; 1024 * 1024 + 1]
                } else {
                    b"kept".to_vec()
                };
                fs::write(&target, &contents).unwrap();
                match kind {
                    "fifo" | "fifo with reader" => nix::unistd::mkfifo(
                        &path,
                        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                    )
                    .unwrap(),
                    "directory" => fs::create_dir(&path).unwrap(),
                    "link" | "large link" => std::os::unix::fs::symlink(&target, &path).unwrap(),
                    _ => unreachable!(),
                }
                let _reader = (kind == "fifo with reader").then(|| {
                    OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NONBLOCK)
                        .open(&path)
                        .unwrap()
                });

                // The synchronous open cannot be interrupted by a Tokio timeout.
                // Confine it to a child we always reap, even on the old blocking path.
                let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "client::tests::recording_refuses_non_files_without_waiting_for_a_reader",
                    ])
                    .env(WORKER, dir.path())
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                let success = loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        break status.success();
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        child.wait().unwrap();
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                assert_eq!(fs::read(&target).unwrap(), contents);
                refused.push((name, kind, success));
            }
        }
        assert!(
            refused.iter().all(|(_, _, success)| *success),
            "{refused:?}"
        );
    }

    #[test]
    fn recording_appends_privately_and_rotates_a_full_log() {
        let dir = tempfile::tempdir().unwrap();
        let client = client_in(dir.path()).doing(Doing::new("run"));
        let request = json!({"op": "run"});
        client.record_start(&request).unwrap();
        client
            .record(&request, &json!({"ok": true}), Duration::ZERO)
            .unwrap();
        let output = client.output_dir().unwrap();
        let path = output.join("metadata.jsonl");
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 2);
        for name in ["metadata.lock", "metadata.jsonl"] {
            assert_eq!(
                fs::metadata(output.join(name)).unwrap().mode() & 0o777,
                0o600
            );
        }

        let full = vec![b'x'; 1024 * 1024 + 1];
        fs::write(&path, &full).unwrap();
        client.record_start(&request).unwrap();
        assert_eq!(
            fs::read(output.join("metadata.previous.jsonl")).unwrap(),
            full
        );
        let row: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(row["operation"], "run");
        assert_eq!(row["started"], true);
    }

    #[test]
    fn a_refused_app_list_keeps_its_code() {
        let busy = json!({"ok": false, "code": "busy", "uncertain": false,
            "error": "The helper is serving too many connections"});
        let failed = json!({"ok": false, "code": "apps", "uncertain": false,
            "error": "Running applications could not be listed"});

        for (reply, code) in [(busy, "busy"), (failed, "apps")] {
            let error = app_list(reply).unwrap_err();
            assert_eq!((error.code, error.uncertain), (code, false));
        }
    }

    #[test]
    fn a_reply_without_an_app_array_is_never_an_empty_list() {
        let malformed = [
            json!({"ok": true}),
            json!({"ok": true, "apps": null}),
            json!({"ok": true, "apps": {}}),
            json!({"ok": true, "apps": [{"name": "TextEdit"}]}),
            json!({"ok": true, "apps": [{"pid": 0}]}),
            json!({"ok": true, "apps": [{"pid": -1}]}),
            json!({"ok": true, "apps": [{"pid": "5"}]}),
            json!({"ok": true, "apps": [{"pid": 2_147_483_648_u64}]}),
            json!({"ok": true, "apps": [{"pid": 5, "name": 3}]}),
            json!({"ok": true, "apps": [{"pid": 5, "active": "yes"}]}),
            json!({"ok": true, "apps": ["TextEdit"]}),
            json!({"apps": []}),
        ];

        for reply in malformed {
            let error = app_list(reply.clone()).unwrap_err();
            assert_eq!(
                (error.code, error.uncertain),
                ("protocol", false),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_truly_empty_list_is_an_empty_list() {
        let empty = json!({"ok": true, "apps": [], "elapsedMs": 1.5});
        assert_eq!(app_list(empty.clone()).unwrap(), empty);

        let listed = json!({"ok": true, "apps": [{"pid": 812, "name": null, "bundleId": null,
            "executable": "/x", "active": true, "hidden": false, "policy": "regular"}]});
        assert_eq!(app_list(listed.clone()).unwrap(), listed);
    }

    #[tokio::test]
    async fn an_app_list_read_is_never_uncertain() {
        use crate::helper_test::{Helper, UID};

        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);

        // The stand-in takes the request and hangs up without a reply.
        let error = helper.serve(client.apps(), |_| None).await.unwrap_err();

        assert_eq!((error.code, error.uncertain), ("protocol", false));
    }
}
