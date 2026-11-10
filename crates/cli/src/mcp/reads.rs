//! What the model and the view read: the status, the helper's screen, and
//! the activity and its pictures the view lists.
use super::{
    Server,
    presentation::{add_points, own_session, said, summary, tool_result},
    resources::{FRAME_META, TILES_META},
};
use crate::frame_pipe::client::LONGEST;
use atlas_client::{
    activity_log::Whose,
    helper_info::{read_capture, thumbnail},
    host_session::{names::names, store::Store},
    screen_tiles::{Pixels, Tiled},
};
use atlas_core::{
    buffer::Buffer,
    error::{Code, Error, Result},
    host::Key,
    transport::refusal,
};
use atlas_native::native::capture::{RawShot, png_rgb, rgb_fitted};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// How long a status read serves the view's frames: each read asks the
/// helper to prove its session, so frames once a second share one.
const STATUS_REUSE: Duration = Duration::from_secs(2);
/// The most activity rows a view is sent: well inside what an agent app
/// hands a view, with each row's pictures and facts.
const ACTIVITY: usize = 100;
/// Longest a screenshot taken for the model may take, from its start.
const TAKING: Duration = Duration::from_secs(120);

/// The narrowest picture of the screen a view is sent: smaller saves little.
const NARROWEST: u32 = 640;
/// The steps a view's size is rounded up to, so a view resized a little
/// keeps building on the tiles it has.
const STEP: u32 = 320;

/// The longest side of the picture a view `drawn` pixels across is sent:
/// its size rounded up to a step, never more than the menu bar's service
/// gives, nor less than [`NARROWEST`]; the most when the view does not say.
pub(super) fn longest(drawn: Option<u32>) -> u32 {
    drawn.map_or(LONGEST, |drawn| {
        drawn
            .div_ceil(STEP)
            .saturating_mul(STEP)
            .clamp(NARROWEST, LONGEST)
    })
}

/// A screen as read: the service's pixels with their width, height and row
/// stride, or a picture file: the helper's PNG, or one of the activity's.
enum Shot {
    Streamed(Buffer, (usize, usize, usize)),
    Png(Vec<u8>),
}

/// A screen's pixels at the size a view is sent.
enum Drawn {
    Kept(Buffer, (usize, usize, usize)),
    Made(Arc<RawShot>),
}

/// The last picture file drawn at a view's size: its digest, that size,
/// and its pixels, so reads of a picture that has not changed draw it once.
pub(super) type Decoded = Option<([u8; 32], u32, Arc<RawShot>)>;

impl Shot {
    /// The pixels at most `longest` on their longest side: the service's as
    /// they are when they fit, else drawn again; a PNG drawn once while it
    /// stays the same, kept in `decoded`.
    fn fitted(self, longest: u32, decoded: &Mutex<Decoded>) -> Result<Drawn> {
        match self {
            Shot::Streamed(rgb, layout) if layout.0.max(layout.1) <= longest as usize => {
                Ok(Drawn::Kept(rgb, layout))
            }
            Shot::Streamed(rgb, layout) => {
                rgb_fitted(&rgb, layout, longest).map(|shot| Drawn::Made(Arc::new(shot)))
            }
            Shot::Png(png) => {
                let digest: [u8; 32] = Sha256::digest(&png).into();
                let mut kept = decoded
                    .lock()
                    .map_err(|_| Error::new(Code::Capture, "The screen could not be read"))?;
                if let Some((known, size, shot)) = kept.as_ref()
                    && *known == digest
                    && *size == longest
                {
                    return Ok(Drawn::Made(shot.clone()));
                }
                let shot = Arc::new(png_rgb(&png, longest)?);
                *kept = Some((digest, longest, shot.clone()));
                Ok(Drawn::Made(shot))
            }
        }
    }
}

impl Drawn {
    fn pixels(&self) -> Pixels<'_> {
        match self {
            Drawn::Kept(rgb, (width, height, stride)) => Pixels {
                rgb,
                width: *width,
                height: *height,
                stride: *stride,
            },
            Drawn::Made(shot) => Pixels {
                rgb: &shot.rgb,
                width: shot.width as usize,
                height: shot.height as usize,
                stride: shot.width as usize * 3,
            },
        }
    }
}

impl Server {
    /// The helper's status as the view and the model read it.
    pub(super) async fn status(&self) -> Result<Value> {
        let began = Instant::now();
        let output = self.client.output_dir()?.to_path_buf();
        let (status, _) = crate::watch::read(&self.client, &output, self.port)
            .await
            .ok_or_else(|| Error::new(Code::Menubar, "Atlas's menu bar is busy; try again"))?;
        // The records and notes are read under their locks, off the runtime.
        let client = self.client.clone();
        let (recent, unrecorded) = tokio::task::spawn_blocking(move || {
            let recent = Store::at(&output).recent();
            let unrecorded = atlas_client::host_session::reading::first_command_running(
                client.output_dir().ok(),
                &client.controller,
                &recent,
            );
            (recent, unrecorded)
        })
        .await
        .map_err(|error| {
            Error::new(Code::State, "Atlas could not read its sessions").with_source(error)
        })?;
        let named = names(recent.iter().map(|seen| &seen.record));
        let parent = nix::unistd::getppid().as_raw();
        let own = own_session(self.host_session.as_ref(), &recent, parent, unrecorded);
        let status = summary(&status, &named, own.as_ref());

        let mut last = self.last.lock().await;
        if last.as_ref().is_none_or(|(at, _)| *at <= began) {
            *last = Some((began, status.clone()));
        }
        Ok(status)
    }

    /// The tool result for a fresh status read.
    pub(super) async fn status_result(&self) -> Result<Value> {
        self.status().await.map(|status| said(&status))
    }

    /// The status for a frame: the last read when it is recent.
    async fn recent_status(&self) -> Result<Value> {
        let last = self.last.lock().await.clone();
        match last {
            Some((at, status)) if at.elapsed() < STATUS_REUSE => Ok(status),
            _ => self.status().await,
        }
    }

    /// The status with the tiles of the helper's newest screen the view
    /// showing `since` lacks, at most `drawn` pixels on its longest side
    /// (the view's own size), from a recent status read. The tiles go in
    /// the result's `_meta`, which the view reads and the model does not;
    /// `streaming` says they came from the menu bar's frame service, which
    /// the view may read as often as it likes.
    pub(super) async fn screen(&self, since: Option<&str>, drawn: Option<u32>) -> Result<Value> {
        let status = self.recent_status().await?;
        let (shot, shared, streaming) = match self.streamed().await {
            Some(streamed) => (
                Some(Shot::Streamed(streamed.rgb, streamed.layout)),
                Some((streamed.key, streamed.display)),
                true,
            ),
            None => {
                let output = self.client.output_dir()?.to_path_buf();
                let screen = crate::watch::screen(&self.client, &output, self.port).await;
                let shared = screen.as_ref().and_then(|screen| screen.shared.clone());
                (screen.map(|screen| Shot::Png(screen.png)), shared, false)
            }
        };
        if let Some((key, _)) = &shared {
            self.saw_frame(key.clone()).await;
        }
        let tiled = match shot {
            Some(shot) => self.tiled(since, shot, longest(drawn)).await.ok(),
            None => None,
        };

        let mut result = said(&status);
        result["structuredContent"]["unchanged"] =
            json!(tiled.as_ref().is_some_and(Tiled::unchanged));
        result["structuredContent"]["screen"] = json!(tiled.is_some());
        result["structuredContent"]["streaming"] = json!(streaming);
        if let Some((_, display)) = &shared {
            add_points(&mut result, *display);
        }
        if let Some(tiled) = tiled {
            let mut tiles = json!(tiled);
            if let Some((key, _)) = shared {
                tiles["key"] = json!(key);
            }
            result["_meta"][TILES_META] = tiles;
        }
        Ok(result)
    }

    /// `shot` at most `longest` pixels on its longest side, as the tiles the
    /// view showing `since` lacks.
    async fn tiled(&self, since: Option<&str>, shot: Shot, longest: u32) -> Result<Tiled> {
        let unprepared = || Error::new(Code::Capture, "The helper's screen couldn't be prepared");
        let (tiles, decoded) = (self.tiles.clone(), self.decoded.clone());
        let since = since.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let drawn = shot.fitted(longest, &decoded)?;
            let pixels = drawn.pixels();
            let mut tiles = tiles.lock().map_err(|_| unprepared())?;
            tiles.next(since.as_deref(), pixels)
        })
        .await
        .map_err(|error| unprepared().with_source(error))?
        .map_err(|error| unprepared().with_source(error))
    }

    /// Everything done in the helper lately, newest first, as Atlas.app's
    /// Activity reads it: every session's rows, or the rows of the session
    /// `session` names by its key (an empty one is Terminal's), each with
    /// the pictures taken around it, and the sessions they name.
    pub(super) async fn activity(&self, session: Option<&Value>) -> Result<Value> {
        let whose = match session {
            None => None,
            Some(Value::String(key)) if key.is_empty() => Some(Whose::Terminal),
            Some(key) => Some(Whose::Host(key.as_str().and_then(Key::parse).ok_or_else(
                || Error::new(Code::BadRequest, "Name a session by its key"),
            )?)),
        };
        let mut reply = crate::viewer::activity_of(&self.client, whose, ACTIVITY).await?;
        if let Some(fields) = reply.as_object_mut() {
            fields.remove("ok");
        }
        for row in reply["activity"].as_array_mut().into_iter().flatten() {
            if let Some(facts) = row.get_mut("facts") {
                only_marks(facts);
            }
        }
        let count = reply["activity"].as_array().map_or(0, Vec::len);
        Ok(tool_result(&format!("{count} recent actions."), reply))
    }

    /// A screenshot the model takes in the helper, with the image itself:
    /// `atlas screenshot` takes it, so its guards and its record are the
    /// command's own. It runs as no session's: one server may serve several
    /// chats, so the session its environment names may not be the caller's,
    /// and a session's screenshot sets where its untargeted input goes. The
    /// image comes back even when the command says it is unchanged: the last
    /// screenshot of that part may have been another session's.
    pub(super) async fn take_screenshot(&self, arguments: &Value) -> Result<Value> {
        let argv = screenshot_argv(arguments)?;
        let program = std::env::current_exe().map_err(|error| {
            Error::new(Code::Capture, "Atlas could not find itself").with_source(error)
        })?;
        let mut command = tokio::process::Command::new(program);
        command
            .arg("--json")
            .arg("--config")
            .arg(&self.config)
            .args(argv)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        for variable in atlas_core::host::Host::session_variables() {
            command.env_remove(variable);
        }
        let output = tokio::time::timeout(TAKING, atlas_core::spawn::output_async(&mut command))
            .await
            .map_err(|_| Error::new(Code::Capture, "The screenshot took too long"))?
            .map_err(|error| {
                Error::new(Code::Capture, "The screenshot did not start").with_source(error)
            })?;
        // A command says what it did on its output, and why it failed on its
        // errors.
        let said = if output.status.success() {
            &output.stdout
        } else {
            &output.stderr
        };
        let reply: Value = serde_json::from_slice(said).map_err(|error| {
            Error::new(
                Code::Capture,
                "The screenshot said nothing Atlas could read",
            )
            .with_source(error)
        })?;
        if reply["ok"] != true {
            let refused = refusal(&reply);
            return Err(match reply["next"].as_str() {
                Some(next) => refused.with("next", json!(next)),
                None => refused,
            });
        }

        let text = reply.to_string();
        // Read where the command saved it, which its own configuration
        // chose: by its name alone, as a screenshot is read anywhere.
        let path = reply["path"].as_str().map(Path::new);
        let (output, name) = path
            .and_then(|path| Some((path.parent()?, path.file_name()?.to_str()?)))
            .map(|(output, name)| (output.to_path_buf(), name.to_owned()))
            .ok_or_else(|| Error::new(Code::Capture, "The screenshot named no file"))?;
        let png = tokio::task::spawn_blocking(move || read_capture(&output, &name))
            .await
            .map_err(|error| {
                Error::new(Code::Capture, "The screenshot could not be read").with_source(error)
            })??;

        let mut result = tool_result(&text, reply);
        result["content"]
            .as_array_mut()
            .ok_or_else(|| Error::new(Code::Capture, "The screenshot could not be said"))?
            .push(json!({"type": "image", "data": STANDARD.encode(png), "mimeType": "image/png"}));
        Ok(result)
    }

    /// One picture of the activity, by its name: a screenshot an agent
    /// took, or the screen just before an agent's step. As a thumbnail; or,
    /// with `full`, at the size the view's screen is sent, as the tiles the
    /// view holding `since` of it lacks, so it is as sharp as the screen.
    pub(super) async fn activity_picture(
        &self,
        name: &str,
        full: bool,
        since: Option<&str>,
    ) -> Result<Value> {
        let named = crate::viewer::Named::parse(name)
            .ok_or_else(|| Error::new(Code::BadRequest, "No picture has that name"))?;
        let output = self.client.output_dir()?.to_path_buf();
        let unread = || Error::new(Code::Capture, "The picture could not be read");
        let mut result = tool_result("A picture from the helper.", json!({"name": name}));
        if !full {
            let image = tokio::task::spawn_blocking(move || -> Result<Value> {
                thumbnail(&named.read(&output)?.0)
            })
            .await
            .map_err(|error| unread().with_source(error))??;
            result["_meta"][FRAME_META] = image;
            return Ok(result);
        }

        let (tiles, decoded) = (self.picture_tiles.clone(), self.picture_decoded.clone());
        let since = since.map(str::to_owned);
        let tiled = tokio::task::spawn_blocking(move || -> Result<Tiled> {
            // The picture's next answers draw it once.
            let drawn = Shot::Png(named.read(&output)?.0).fitted(LONGEST, &decoded)?;
            let mut tiles = tiles.lock().map_err(|_| unread())?;
            tiles.next(since.as_deref(), drawn.pixels())
        })
        .await
        .map_err(|error| unread().with_source(error))??;
        result["_meta"][TILES_META] = json!(tiled);
        Ok(result)
    }
}

/// The facts kept beside a step's picture, cut to what the view marks the
/// picture with: the kind of input, the display, where it acted and how,
/// the keys, the scroll, and where the element it went to stands. Nothing
/// typed, no element's name or role and no task goes into an agent app.
pub(super) fn only_marks(facts: &mut Value) {
    const MARKS: [&str; 9] = [
        "action", "display", "point", "to", "clicks", "button", "keys", "scroll", "element",
    ];
    if let Some(fields) = facts.as_object_mut() {
        fields.retain(|key, _| MARKS.contains(&key.as_str()));
        if let Some(element) = fields.get_mut("element").and_then(Value::as_object_mut) {
            element.retain(|key, _| key == "frame");
        }
    }
}

/// The `atlas screenshot` command line for a `take_screenshot` call's
/// `arguments`: an object of `app`, `window` and `region`, each optional.
fn screenshot_argv(arguments: &Value) -> Result<Vec<String>> {
    let bad = |said: &str| Error::new(Code::BadRequest, said);
    let given = match arguments {
        Value::Null => return Ok(vec!["screenshot".to_owned()]),
        Value::Object(given) => given,
        _ => {
            return Err(bad(
                "take_screenshot takes an object of app, window and region",
            ));
        }
    };
    // A view's authority, which a read does not need, is no argument of it.
    if let Some(other) = given
        .keys()
        .find(|key| !["app", "window", "region", super::guarded::ARGUMENT].contains(&key.as_str()))
    {
        return Err(bad(&format!("take_screenshot takes no {other}")));
    }

    let mut argv = vec!["screenshot".to_owned()];
    for (key, flag) in [("app", "--app"), ("region", "--region")] {
        match given.get(key) {
            None => {}
            Some(Value::String(text)) => argv.push(format!("{flag}={text}")),
            Some(_) => return Err(bad(&format!("{key} is a string"))),
        }
    }
    match given.get("window") {
        None => {}
        Some(window) => {
            let number = window
                .as_i64()
                .ok_or_else(|| bad("window is a window's number"))?;
            argv.push(format!("--window={number}"));
        }
    }
    Ok(argv)
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;
