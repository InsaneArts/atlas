//! What the model and the view read: the status, the helper's screen, and
//! the activity, apps and screenshots the view lists.
use super::{
    Server,
    presentation::{
        add_points, only_title_and_size, own_session, said, session_name, summary, tool_result,
    },
    resources::FRAME_META,
};
use atlas_client::{
    helper_info::{ICONS, capture_time, captures, frame, open_apps, read_capture, thumbnail},
    host_session::{names::names, store::Store},
};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

/// How long a status read serves the view's frames: each read asks the
/// helper to prove its session, so frames once a second share one.
const STATUS_REUSE: Duration = Duration::from_secs(2);
/// The most activity rows and screenshots listed.
const ACTIVITY: usize = 100;
const SHOTS: usize = 30;

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

    /// The status with the helper's newest screen, unless it is still the
    /// one the view has (`since`), from a recent status read. The screen
    /// goes in the result's `_meta`, which the view reads and the model
    /// does not; `streaming` says it came from the menu bar's frame
    /// service, which the view may read as often as it likes.
    pub(super) async fn screen(&self, since: Option<&str>) -> Result<Value> {
        let status = self.recent_status().await?;
        let (frame, shared, streaming) = match self.streamed().await {
            Some(streamed) => (
                Some(streamed.frame),
                Some((streamed.key, streamed.display)),
                true,
            ),
            None => {
                let output = self.client.output_dir()?.to_path_buf();
                let screen = crate::watch::screen(&self.client, &output, self.port).await;
                let shared = screen.as_ref().and_then(|screen| screen.shared.clone());
                let frame = match screen {
                    Some(screen) => self.frame_of(screen.png).await.ok(),
                    None => None,
                };
                (frame, shared, false)
            }
        };
        if let Some((key, _)) = &shared {
            self.saw_frame(key.clone()).await;
        }

        let unchanged = frame
            .as_ref()
            .is_some_and(|frame| since.is_some_and(|since| frame["id"] == since));
        let mut result = said(&status);
        result["structuredContent"]["unchanged"] = json!(unchanged);
        result["structuredContent"]["screen"] = json!(frame.is_some());
        result["structuredContent"]["streaming"] = json!(streaming);
        if let Some((_, display)) = &shared {
            add_points(&mut result, *display);
        }
        let controlling = self.controlling().await;
        if let Some(mut frame) = frame.filter(|_| !unchanged || controlling) {
            if let Some((key, _)) = shared {
                frame["key"] = json!(key);
            }
            result["_meta"] = json!({ FRAME_META: frame });
        }
        Ok(result)
    }

    /// The frame for a screen: the last one made when the screen has not
    /// changed, else a new one.
    pub(super) async fn frame_of(&self, png: Vec<u8>) -> Result<Value> {
        let unprepared = || Error::new(Code::Capture, "The helper's screen couldn't be prepared");
        let digest: [u8; 32] = Sha256::digest(&png).into();
        if let Some((known, frame)) = self.framed.lock().await.as_ref()
            && *known == digest
        {
            return Ok(frame.clone());
        }

        let made = tokio::task::spawn_blocking(move || frame(&png))
            .await
            .map_err(|error| unprepared().with_source(error))?
            .map_err(|error| unprepared().with_source(error))?;
        *self.framed.lock().await = Some((digest, made.clone()));
        Ok(made)
    }

    /// Everything done in the helper lately, newest first.
    pub(super) async fn activity(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        let (rows, named) = tokio::task::spawn_blocking(move || {
            let rows = atlas_client::activity_log::recent(&output, ACTIVITY);
            let records = Store::at(&output).recent();
            (rows, names(records.iter().map(|seen| &seen.record)))
        })
        .await
        .unwrap_or_default();
        let now = atlas_core::time::timestamp();
        let rows = rows
            .iter()
            .map(|row| {
                let row = atlas_client::overview::shown(row, now);
                let name = row
                    .session
                    .as_deref()
                    .and_then(|key| session_name(key, &named));
                let mut shown = json!({
                    "says": row.says,
                    "ok": row.ok,
                    "started": row.started,
                    "actor": row.actor,
                    "ageSeconds": row.age_seconds,
                    "reason": row.reason,
                    "nothingSent": row.nothing_sent,
                });
                if let Some(name) = name {
                    shown["session"] = json!(name);
                }
                shown
            })
            .collect::<Vec<_>>();

        Ok(tool_result(
            &format!("{} recent actions.", rows.len()),
            json!({"activity": rows}),
        ))
    }

    /// The apps open in the helper with their windows, front to back, and
    /// their icons.
    pub(super) async fn helper_apps(&self) -> Result<Value> {
        let apps = self.client.running_apps().await?;
        let filter = atlas_client::agent::WindowFilter::default();
        let windows = atlas_client::agent::list(&self.client, &filter).await?;
        let mut listed = open_apps(&apps, &windows)?;

        listed.iter_mut().for_each(only_title_and_size);
        self.icons.add(&mut listed, ICONS).await;

        Ok(tool_result(
            &format!("{} apps open in the helper.", listed.len()),
            json!({"apps": listed}),
        ))
    }

    /// The screenshots agents took, newest first.
    pub(super) async fn screenshots(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        let now = atlas_core::time::timestamp();
        let shots = tokio::task::spawn_blocking(move || captures(&output, SHOTS))
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(at, name)| {
                let seconds = now.saturating_sub(at) / 1_000_000_000;
                json!({"name": name, "ageSeconds": u64::try_from(seconds).unwrap_or(u64::MAX)})
            })
            .collect::<Vec<_>>();

        Ok(tool_result(
            &format!("{} screenshots.", shots.len()),
            json!({"screenshots": shots}),
        ))
    }

    /// One screenshot an agent took, as a thumbnail or `full`.
    pub(super) async fn screenshot(&self, name: &str, full: bool) -> Result<Value> {
        if capture_time(name).is_none() {
            return Err(Error::new(Code::BadRequest, "No screenshot has that name"));
        }

        let output = self.client.output_dir()?.to_path_buf();
        let wanted = name.to_owned();
        let image = tokio::task::spawn_blocking(move || -> Result<Value> {
            let png = read_capture(&output, &wanted)?;
            if full { frame(&png) } else { thumbnail(&png) }
        })
        .await
        .map_err(|error| {
            Error::new(Code::Capture, "The screenshot could not be read").with_source(error)
        })??;

        let mut result = tool_result("A screenshot from the helper.", json!({"name": name}));
        result["_meta"] = json!({ FRAME_META: image });
        Ok(result)
    }
}
