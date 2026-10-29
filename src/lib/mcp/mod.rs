//! `atlas mcp`: a Model Context Protocol server on standard input and
//! output, so agent apps can show what an agent does in the helper. It
//! offers the status and, in apps that draw MCP Apps views (the Claude app,
//! the Codex app), a live view of the helper's screen with Stop.
//!
//! It never acts in the helper for an agent: agents keep using the command
//! line. The model sees only the status and the view's tool; the rest are
//! tools only the view may call, as the person's: the frames, Stop and
//! Resume, taking control and the input the person sends with it, the
//! activity, the helper's apps and the agent's screenshots, Recover, Open in
//! Atlas, Switch and Log Out. Control goes through the menu bar's viewer,
//! as its own window's does, so one controller at a time holds it.
use crate::{
    client::{Actor, Client},
    error::{Error, Result},
    helper_info::{
        FRAME_LIMIT, ICONS, Icons, capture_time, captures, frame, open_apps, read_capture,
        thumbnail,
    },
    host_session::{Host, HostSession, Key, names::names, record, store::Store},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

mod guarded;

/// The MCP Apps extension, as a client declares it draws views.
const UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
/// Where a result carries the helper's screen for the view.
const FRAME_META: &str = "atlas/frame";
/// The view's address and its MCP Apps type.
const SCREEN_URI: &str = "ui://atlas/screen.html";
const APP_MIME: &str = "text/html;profile=mcp-app";
/// The view: one page, its script inlined where it marks.
const SCREEN: &str = include_str!("screen.html");
const SCREEN_SCRIPT: &str = include_str!("screen.js");
/// MCP versions this server speaks; it answers in the client's when it is
/// one of them.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// The longest request line read, in bytes.
const MAX_LINE: usize = 1 << 20;
/// How long a status read serves the view's frames: each read asks the
/// helper to prove its session, so frames once a second share one.
const STATUS_REUSE: Duration = Duration::from_secs(2);
/// How long a frame's key names it in control requests: the viewer keeps
/// its last 24 frames for eight seconds, and a key it no longer keeps ends
/// control, so only a recent one is named even while Atlas's own viewer
/// window takes frames quickly.
const KEY_REUSE: Duration = Duration::from_secs(1);
/// How long the viewer's control lasts unless it is kept.
const LEASE: Duration = Duration::from_secs(10);
/// How long a frame read from the menu bar's viewer may take.
const FRAME_DEADLINE: Duration = Duration::from_secs(6);
/// The most activity rows and screenshots listed.
const ACTIVITY: usize = 100;
const SHOTS: usize = 30;
/// The most actions one input request carries, as the helper takes them.
const ACTIONS: usize = 32;
/// The input actions the view may send.
const KINDS: [&str; 5] = ["pointer", "move", "scroll", "type", "key"];
/// The tools only a client that draws views is offered: they are the
/// person's, and only such a client keeps a view's tools from the model.
const VIEW_ONLY: [&str; 11] = [
    "resume_agent",
    "take_control",
    "keep_control",
    "give_back",
    "send_input",
    "activity",
    "helper_apps",
    "screenshots",
    "screenshot",
    "switch_helper",
    "log_out_helper",
];
/// The tools that only look, all a read-only server offers.
const LOOKING: [&str; 7] = [
    "atlas_status",
    "show_screen",
    "screen_frame",
    "activity",
    "helper_apps",
    "screenshots",
    "screenshot",
];

/// What the server needs to answer: the helper's client, and where to open
/// Atlas's viewer from.
struct Server {
    client: Client,
    config: PathBuf,
    port: u16,
    /// Offer only what looks: no Stop, Resume, Recover or viewer.
    read_only: bool,
    /// Only a guarded root has a registry. Children are private tool contexts.
    app_views: OnceLock<Arc<guarded::Registry>>,
    /// Whether the client said it draws MCP Apps views: only then may the
    /// person's Resume be offered, as only such a client keeps a view's
    /// tools from the model.
    views: AtomicBool,
    /// The last status read, and when that read began: a read that began
    /// earlier never replaces it.
    last: Mutex<Option<(Instant, Value)>>,
    /// The last screen made into a frame, by its digest, and that frame.
    framed: Mutex<Option<([u8; 32], Value)>>,
    /// The menu bar viewer's newest frame seen.
    shared: Mutex<Option<Seen>>,
    /// The person's control taken from the view.
    held: Mutex<Option<Held>>,
    /// App icons made.
    icons: Icons,
    /// The host session the server's environment names, read once: the
    /// view says "This chat" only when it is certainly this one.
    host_session: Option<HostSession>,
}

/// A frame of the menu bar's viewer: the key control requests name it by,
/// and when it was seen.
#[derive(Clone)]
struct Seen {
    key: String,
    at: Instant,
}

/// The person's control taken from the view: the viewer's address, the
/// lease it gave, and when it was last kept.
#[derive(Clone)]
struct Held {
    url: String,
    lease: String,
    kept: Instant,
}

/// End `held` at the viewer, as far as it still holds it: a release names
/// its own lease, so it never ends anyone else's.
async fn release(held: &Held) {
    let _ = post(&held.url, "control/release", json!({"lease": held.lease})).await;
}

/// Serve requests from standard input until it closes.
pub async fn serve(
    client: Client,
    config: PathBuf,
    port: u16,
    read_only: bool,
    guard_app_controls: bool,
) -> Result<()> {
    let server = Arc::new(Server {
        client,
        config,
        port,
        read_only,
        app_views: if guard_app_controls {
            OnceLock::from(Arc::new(guarded::Registry::default()))
        } else {
            OnceLock::new()
        },
        views: AtomicBool::new(false),
        last: Mutex::new(None),
        framed: Mutex::new(None),
        shared: Mutex::new(None),
        held: Mutex::new(None),
        icons: Icons::default(),
        host_session: HostSession::from_env(),
    });
    let out = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut input = BufReader::new(tokio::io::stdin());
    let mut answering = tokio::task::JoinSet::new();

    // The app closing standard input, failing to read, or ending this
    // process ends the session alike.
    let stopping = crate::watch::stopped();
    tokio::pin!(stopping);
    let mut sweeping = tokio::time::interval(Duration::from_secs(1));
    loop {
        let read = tokio::select! {
            read = next_with_maintenance(&mut input, &mut sweeping, || {
                while answering.try_join_next().is_some() {}
                if let Some(registry) = server.app_views.get() {
                    let registry = registry.clone();
                    answering.spawn(async move { registry.sweep(false).await; });
                }
            }) => read,
            () = &mut stopping => break,
        };
        let Ok(Some(line)) = read else {
            break;
        };

        while answering.try_join_next().is_some() {}
        if line.as_ref().is_ok_and(|line| line.trim().is_empty()) {
            continue;
        }

        let message = line.and_then(|line| {
            serde_json::from_str::<Value>(&line)
                .map_err(|_| failure(&Value::Null, -32700, "Parse error"))
        });
        // Apply restrictive negotiation in read order, before concurrent
        // dispatch: a pipelined tool call cannot outrun Cursor's guard.
        if let Ok(message) = &message {
            server.prepare(message);
        }

        // Each request is answered on its own, so a frame being made never
        // holds up the status.
        let (server, out) = (server.clone(), out.clone());
        answering.spawn(async move {
            let reply = match message {
                Err(refused) => Some(guarded::Reply::new(refused)),
                Ok(message) => server.answer(&message).await,
            };

            if let Some(mut reply) = reply {
                let mut out = out.lock().await;
                if out
                    .write_all(format!("{}\n", reply.value).as_bytes())
                    .await
                    .is_ok()
                    && out.flush().await.is_ok()
                {
                    reply.delivered();
                }
            }
        });
    }

    // The client stopped asking: what it asked last is still answered, and
    // control the person kept from here ends.
    while answering.join_next().await.is_some() {}
    if let Some(registry) = server.app_views.get() {
        for _ in 0..3 {
            registry.sweep(true).await;
            if registry.is_empty() {
                break;
            }
        }
        if !registry.is_empty() {
            return Err(Error::new(
                "menubar",
                "A view's cleanup is unconfirmed; Atlas retains pending native work for recovery",
            ));
        }
    }
    if let Some(held) = server.held.lock().await.take() {
        release(&held).await;
    }
    Ok(())
}

/// Maintenance must not cancel a line read after it has consumed a prefix.
async fn next_with_maintenance<R: AsyncBufRead + Unpin>(
    input: &mut R,
    ticks: &mut tokio::time::Interval,
    mut maintain: impl FnMut(),
) -> std::io::Result<Option<std::result::Result<String, Value>>> {
    let read = next_request(input);
    tokio::pin!(read);
    loop {
        tokio::select! {
            result = &mut read => return result,
            _ = ticks.tick() => maintain(),
        }
    }
}

/// The next request line from `input`, or the error that answers it: a
/// line longer than MAX_LINE is read to its end and dropped, never held
/// whole, and one that is not UTF-8 is refused while the session goes on.
/// None once the input ends.
async fn next_request<R: AsyncBufRead + Unpin>(
    input: &mut R,
) -> std::io::Result<Option<std::result::Result<String, Value>>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let chunk = input.fill_buf().await?;
        if chunk.is_empty() {
            return Ok((!line.is_empty() || too_long).then(|| finished(line, too_long)));
        }

        let (taken, ended) = match chunk.iter().position(|byte| *byte == b'\n') {
            Some(at) => (at + 1, true),
            None => (chunk.len(), false),
        };
        if !too_long {
            // Keep room for CRLF without copying an oversized reader chunk.
            if taken > MAX_LINE + 2 - line.len() {
                too_long = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(&chunk[..taken]);
            }
        }
        input.consume(taken);
        if ended {
            return Ok(Some(finished(line, too_long)));
        }
    }
}

/// A whole line read, as its text or the error that answers it.
fn finished(line: Vec<u8>, too_long: bool) -> std::result::Result<String, Value> {
    let payload = line.strip_suffix(b"\n").unwrap_or(&line);
    let payload = payload.strip_suffix(b"\r").unwrap_or(payload);
    if too_long || payload.len() > MAX_LINE {
        return Err(failure(&Value::Null, -32600, "Request too large"));
    }
    String::from_utf8(line)
        .map(|text| text.trim_end_matches(['\n', '\r']).to_owned())
        .map_err(|_| failure(&Value::Null, -32700, "Parse error"))
}

/// POST `body` to `route` of the menu bar's viewer at `url`.
async fn post(url: &str, route: &'static str, body: Value) -> Result<Value> {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || crate::viewer::post_local(&url, route, &body))
        .await
        .map_err(|_| Error::new("menubar", "Atlas's menu bar didn't answer"))?
}

/// A JSON-RPC result for request `id`.
fn success(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// A JSON-RPC error for request `id`.
fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

impl Server {
    /// Cursor imports other hosts' plugins too. Its declared client name
    /// can only add the private-view restriction, never grant authority.
    /// Once enabled, subsequent negotiation cannot remove that guard.
    fn prepare(&self, message: &Value) {
        if message["method"] != "initialize" {
            return;
        }
        let params = &message["params"];
        let name = params["clientInfo"]["name"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if name == "cursor" || name.starts_with("cursor-") || name.starts_with("cursor ") {
            self.app_views
                .get_or_init(|| Arc::new(guarded::Registry::default()));
        }
        let views = params["capabilities"]["extensions"][UI_EXTENSION].is_object();
        self.views.store(views, Ordering::Relaxed);
    }

    /// The answer to one message; none for a notification.
    async fn answer(&self, message: &Value) -> Option<guarded::Reply> {
        self.prepare(message);
        let id = message.get("id")?;
        let params = &message["params"];

        if message["method"] == "tools/call"
            && let Some(registry) = self.app_views.get()
        {
            return Some(registry.call(self, id, params).await);
        }

        Some(guarded::Reply::new(
            match message["method"].as_str().unwrap_or_default() {
                "initialize" => success(id, initialized(params)),
                "ping" => success(id, json!({})),
                "tools/list" => success(id, json!({"tools": self.tools()})),
                "tools/call" if !self.offers(params["name"].as_str().unwrap_or_default()) => {
                    failure(id, -32602, "Unknown tool")
                }
                "tools/call" => success(id, self.call(params).await),
                "resources/list" => success(id, json!({"resources": [screen_resource()]})),
                "resources/templates/list" => success(id, json!({"resourceTemplates": []})),
                "resources/read" if params["uri"] == SCREEN_URI => {
                    success(id, json!({"contents": [screen_contents()]}))
                }
                "resources/read" => failure(id, -32002, "Resource not found"),
                _ => failure(id, -32601, "Method not found"),
            },
        ))
    }

    /// The result of the tool `params` names, an error result when it fails.
    async fn call(&self, params: &Value) -> Value {
        let arguments = &params["arguments"];
        let outcome = match params["name"].as_str().unwrap_or_default() {
            "atlas_status" | "show_screen" => self.status().await.map(|status| said(&status)),
            "screen_frame" => self.screen(arguments["since"].as_str()).await,
            "stop_agent" => self.set_stopped(true).await,
            "resume_agent" => self.set_stopped(false).await,
            "recover" => self.recover().await,
            "take_control" => self.take_control().await,
            "keep_control" => self.keep_control().await,
            "give_back" => self.give_back().await,
            "send_input" => {
                self.send_input(&arguments["frame"], &arguments["actions"])
                    .await
            }
            "activity" => self.activity().await,
            "helper_apps" => self.helper_apps().await,
            "screenshots" => self.screenshots().await,
            "screenshot" => {
                self.screenshot(
                    arguments["name"].as_str().unwrap_or_default(),
                    arguments["full"] == true,
                )
                .await
            }
            "switch_helper" => self.switch_helper().await,
            "log_out_helper" => self.log_out_helper().await,
            _ => self.open_viewer().await,
        };

        let mut result = outcome.unwrap_or_else(|error| {
            json!({
                "content": [{"type": "text", "text": error.error}],
                "structuredContent": {"error": error},
                "isError": true,
            })
        });
        // The view offers Resume and control only where this client may
        // call them, and knows whether the control shown is its own.
        let can_control = self.views.load(Ordering::Relaxed) && !self.read_only;
        result["structuredContent"]["canResume"] = json!(can_control);
        result["structuredContent"]["canControl"] = json!(can_control);
        result["structuredContent"]["control"] = json!(self.controlling().await);
        result
    }

    /// The tools this client is offered: Resume only to one that draws
    /// views.
    fn tools(&self) -> Value {
        let views = self.views.load(Ordering::Relaxed);
        let named = |names: &[&str], tool: &Value| names.iter().any(|name| tool["name"] == *name);
        let mut listed: Vec<Value> = tools()
            .as_array()
            .into_iter()
            .flatten()
            .filter(|tool| views || !named(&VIEW_ONLY, tool))
            .filter(|tool| !self.read_only || named(&LOOKING, tool))
            .filter(|tool| self.app_views.get().is_none() || views || named(&LOOKING, tool))
            .cloned()
            .collect();
        if self.app_views.get().is_some() && views && !self.read_only {
            for tool in &mut listed {
                tool["inputSchema"]["properties"][guarded::ARGUMENT] = json!({"type":"string"});
                if tool["name"] == "show_screen" {
                    tool["inputSchema"]["properties"]["_atlasOpen"] = json!({"type":"boolean"});
                }
            }
            listed.push(json!({"name":"view_close","description":"Close this Atlas view's private context.","inputSchema":{"type":"object","properties":{guarded::ARGUMENT:{"type":"string"}},"required":[guarded::ARGUMENT],"additionalProperties":false},"_meta":app_only()}));
        }
        json!(listed)
    }

    /// Whether this client is offered the tool `name`.
    fn offers(&self, name: &str) -> bool {
        self.tools()
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == name))
    }

    /// The helper's status as the view and the model read it.
    async fn status(&self) -> Result<Value> {
        let began = Instant::now();
        let output = self.client.output_dir()?.to_path_buf();
        let (status, _) = crate::watch::read(&self.client, &output, self.port)
            .await
            .ok_or_else(|| Error::new("menubar", "Atlas's menu bar is busy; try again"))?;
        // The records and notes are read under their locks, off the runtime.
        let client = self.client.clone();
        let (recent, unrecorded) = tokio::task::spawn_blocking(move || {
            let recent = Store::at(&output).recent();
            let unrecorded = crate::overview::first_command_running(&client, &recent);
            (recent, unrecorded)
        })
        .await
        .map_err(|_| Error::new("state", "Atlas could not read its sessions"))?;
        let named = names(recent.iter().map(|seen| &seen.record));
        let own = own_session(
            self.host_session.as_ref(),
            &recent,
            parent_pid(),
            unrecorded,
        );
        let status = summary(&status, &named, own.as_ref());

        let mut last = self.last.lock().await;
        if last.as_ref().is_none_or(|(at, _)| *at <= began) {
            *last = Some((began, status.clone()));
        }
        Ok(status)
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
    /// does not.
    async fn screen(&self, since: Option<&str>) -> Result<Value> {
        let status = self.recent_status().await?;
        let output = self.client.output_dir()?.to_path_buf();
        let screen = crate::watch::screen(&self.client, &output, self.port).await;
        let shared = screen.as_ref().and_then(|screen| screen.shared.clone());
        if let Some((key, _)) = &shared {
            *self.shared.lock().await = Some(Seen {
                key: key.clone(),
                at: Instant::now(),
            });
        }
        let frame = match screen {
            Some(screen) => self.frame_of(screen.png).await,
            None => None,
        };

        let unchanged = frame
            .as_ref()
            .is_some_and(|frame| since.is_some_and(|since| frame["id"] == since));
        let mut result = said(&status);
        result["structuredContent"]["unchanged"] = json!(unchanged);
        result["structuredContent"]["screen"] = json!(frame.is_some());
        // Where the person's clicks land: display points, as input takes.
        if let Some((_, (width, height))) = &shared {
            result["structuredContent"]["points"] = json!({"width": width, "height": height});
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
    async fn frame_of(&self, png: Vec<u8>) -> Option<Value> {
        let digest: [u8; 32] = Sha256::digest(&png).into();
        if let Some((known, frame)) = self.framed.lock().await.as_ref()
            && *known == digest
        {
            return Some(frame.clone());
        }

        let made = tokio::task::spawn_blocking(move || frame(&png))
            .await
            .ok()?
            .ok()?;
        *self.framed.lock().await = Some((digest, made.clone()));
        Some(made)
    }

    /// Stop agents, or let them go on: the person's, from the view.
    async fn set_stopped(&self, stopped: bool) -> Result<Value> {
        self.person().hold(stopped).await?;
        let status = self.status().await?;
        Ok(said(&status))
    }

    /// Recover the helper, as the menu bar's Recover does: restart it when
    /// an action's outcome is unknown, else start its input.
    async fn recover(&self) -> Result<Value> {
        let client = self.person();
        if client.controller.pending()?.is_empty() {
            client.start_input().await?;
        } else {
            client.restart().await?;
        }

        let status = self.status().await?;
        Ok(said(&status))
    }

    /// The menu bar viewer's address; with `start`, starting the menu bar
    /// first when it is not running.
    async fn viewer(&self, start: bool) -> Result<String> {
        let output = self.client.output_dir()?.to_path_buf();
        let port = self.port;
        let found = {
            let output = output.clone();
            tokio::task::spawn_blocking(move || crate::viewer::local(&output, port))
                .await
                .ok()
                .flatten()
        };
        if let Some(url) = found {
            return Ok(url);
        }
        if !start {
            return Err(Error::new("menubar", "Atlas's menu bar isn't running"));
        }

        let (_, record) = crate::menubar::launch(&self.config, self.port, &output).await?;
        record["url"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::new("menubar", "The menu bar's record names no viewer"))
    }

    /// A key the viewer at `url` keeps a frame under now: the last one seen
    /// while it is recent, unless `fresh`, else a new frame's.
    async fn key(&self, url: &str, fresh: bool) -> Result<String> {
        if !fresh
            && let Some(seen) = self.shared.lock().await.as_ref()
            && seen.at.elapsed() < KEY_REUSE
        {
            return Ok(seen.key.clone());
        }

        let address = url.to_owned();
        let frame = tokio::task::spawn_blocking(move || {
            crate::viewer::fetch_frame(&address, FRAME_DEADLINE, FRAME_LIMIT)
        })
        .await
        .map_err(|_| Error::new("menubar", "Atlas's menu bar didn't answer"))??;
        *self.shared.lock().await = Some(Seen {
            key: frame.key.clone(),
            at: Instant::now(),
        });
        Ok(frame.key)
    }

    /// Take control of the helper for the person, through the menu bar's
    /// viewer (started when it is not running). Agents wait meanwhile.
    /// Control already taken from here is kept, or taken afresh when it
    /// lapsed.
    async fn take_control(&self) -> Result<Value> {
        // The slow steps run outside the lock, so a status or Give Back
        // never waits on them.
        let current = self.held.lock().await.clone();
        let url = match &current {
            Some(current) if current.kept.elapsed() < LEASE => current.url.clone(),
            _ => self.viewer(true).await?,
        };
        let address = url.clone();
        let frame = tokio::task::spawn_blocking(move || {
            crate::viewer::fetch_frame(&address, FRAME_DEADLINE, FRAME_LIMIT)
        })
        .await
        .map_err(|_| Error::new("menubar", "Atlas's menu bar didn't answer"))??;
        *self.shared.lock().await = Some(Seen {
            key: frame.key.clone(),
            at: Instant::now(),
        });
        let kept = match &current {
            Some(current) if current.kept.elapsed() < LEASE => {
                let renewed = self.renew(current).await;
                self.settle(current, &renewed, true).await;
                renewed.is_ok()
            }
            _ => false,
        };

        if !kept {
            if let Some(old) = current {
                self.forget(&old).await;
                release(&old).await;
            }
            let reply = post(&url, "control/acquire", json!({"frame": frame.key})).await?;
            let lease = reply["lease"]
                .as_str()
                .ok_or_else(|| Error::new("menubar", "The menu bar gave no control"))?;
            let taken = Held {
                url,
                lease: lease.to_owned(),
                kept: Instant::now(),
            };

            let mut held = self.held.lock().await;
            if held.is_none() {
                *held = Some(taken);
            } else {
                // Another take landed meanwhile: that one stays.
                drop(held);
                release(&taken).await;
            }
        }

        let status = self.status().await?;
        let mut result = said(&status);
        let (width, height) = frame.display;
        result["structuredContent"]["points"] = json!({"width": width, "height": height});
        let mut picture = self
            .frame_of(frame.png)
            .await
            .ok_or_else(|| Error::new("capture", "The helper's screen couldn't be prepared"))?;
        picture["key"] = json!(frame.key);
        result["_meta"] = json!({ FRAME_META: picture });
        Ok(result)
    }

    /// Keep the person's control a while longer: the view says so every
    /// few seconds, so control ends soon after it closes.
    async fn keep_control(&self) -> Result<Value> {
        let held = self.holding().await?;
        let renewed = self.renew(&held).await;
        self.settle(&held, &renewed, true).await;
        renewed?;
        Ok(json!({
            "content": [{"type": "text", "text": "You still have control."}],
            "structuredContent": {},
        }))
    }

    /// Renew `held` at the viewer, naming a frame it keeps.
    async fn renew(&self, held: &Held) -> Result<Value> {
        let key = self.key(&held.url, false).await?;
        post(
            &held.url,
            "control/renew",
            json!({"lease": held.lease, "frame": key}),
        )
        .await
    }

    /// End the person's control; agents may go on.
    async fn give_back(&self) -> Result<Value> {
        let held = self.held.lock().await.clone();
        if let Some(held) = held {
            // Forgotten once the viewer let it go, or never had it; kept
            // (and still kept alive by the view) when the release failed.
            match post(&held.url, "control/release", json!({"lease": held.lease})).await {
                Ok(_) => self.forget(&held).await,
                Err(error) if error.code == "lease-lost" => self.forget(&held).await,
                Err(error) => return Err(error),
            }
        }

        let status = self.status().await?;
        Ok(said(&status))
    }

    /// Send the person's input to the helper while they have control.
    async fn send_input(&self, frame: &Value, actions: &Value) -> Result<Value> {
        let actions = checked(actions)?;
        let held = self.holding().await?;
        let key = frame
            .as_str()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                Error::new("stale-frame", "Wait for a fresh frame before sending input")
            })?;
        let body = json!({"lease": held.lease, "frame": key, "actions": actions});
        let sent = post(&held.url, "input", body).await;
        self.settle(&held, &sent, false).await;
        sent?;

        let count = actions.as_array().map_or(0, Vec::len);
        Ok(json!({
            "content": [{"type": "text", "text": format!("Sent {count} actions.")}],
            "structuredContent": {"sent": count},
        }))
    }

    /// The person's control from here, or why there is none: control nobody
    /// kept for as long as it lasts has ended, and is let go of.
    async fn holding(&self) -> Result<Held> {
        let mut held = self.held.lock().await;
        if let Some(lapsed) = held.take_if(|held| held.kept.elapsed() >= LEASE) {
            release(&lapsed).await;
        }
        held.clone()
            .ok_or_else(|| Error::new("lease-lost", "Take control first"))
    }

    /// Whether the person has control from here now.
    async fn controlling(&self) -> bool {
        self.held
            .lock()
            .await
            .as_ref()
            .is_some_and(|held| held.kept.elapsed() < LEASE)
    }

    /// After a request on `used`: a renewal that went through keeps it
    /// another lease (`renewed`; input does not, as only a renewal extends
    /// the viewer's). It is forgotten and let go of when refused for any
    /// reason the viewer ends it on; a busy viewer, input it would not read,
    /// or an answer that never came (`menubar`) leave it to its lease. Only
    /// that same control is touched: one taken since stays.
    async fn settle(&self, used: &Held, outcome: &Result<Value>, renewed: bool) {
        let mut held = self.held.lock().await;
        let same = held.as_ref().is_some_and(|held| held.lease == used.lease);
        match outcome {
            Ok(_) if same && renewed => {
                if let Some(held) = held.as_mut() {
                    held.kept = Instant::now();
                }
            }
            Err(error)
                if same && !matches!(error.code, "busy" | "input" | "invalid-json" | "menubar") =>
            {
                if let Some(ended) = held.take() {
                    release(&ended).await;
                }
            }
            _ => {}
        }
    }

    /// Forget `used`, unless control taken since replaced it.
    async fn forget(&self, used: &Held) {
        let mut held = self.held.lock().await;
        if held.as_ref().is_some_and(|held| held.lease == used.lease) {
            held.take();
        }
    }

    /// Everything done in the helper lately, newest first.
    async fn activity(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        let (rows, named) = tokio::task::spawn_blocking(move || {
            (
                crate::activity_log::recent(&output, ACTIVITY),
                session_names(&output),
            )
        })
        .await
        .unwrap_or_default();
        let now = crate::client::timestamp();
        let rows = rows
            .iter()
            .map(|row| crate::overview::shown(row, now))
            .map(|row| {
                let mut shown = json!({
                    "says": row.says,
                    "ok": row.ok,
                    "started": row.started,
                    "actor": row.actor,
                    "ageSeconds": row.age_seconds,
                    "reason": row.reason,
                    "nothingSent": row.nothing_sent,
                });
                if let Some(name) = session_name(row.session.as_deref(), &named) {
                    shown["session"] = json!(name);
                }
                shown
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "content": [{"type": "text", "text": format!("{} recent actions.", rows.len())}],
            "structuredContent": {"activity": rows},
        }))
    }

    /// The apps open in the helper with their windows, front to back, and
    /// their icons.
    async fn helper_apps(&self) -> Result<Value> {
        let apps = self.client.apps().await?;
        let filter = crate::agent::WindowFilter {
            app: None,
            title: None,
            all: false,
        };
        let windows = crate::agent::list(&self.client, &filter).await?;
        let mut listed = open_apps(&apps["apps"], &windows["windows"])?;

        for (at, app) in listed.iter_mut().enumerate() {
            only_title_and_size(app);
            if at < ICONS
                && let Some(bundle) = app["bundle"].as_str().map(PathBuf::from)
                && let Some(icon) = self.icons.get(bundle).await
            {
                app["icon"] = json!(icon);
            }
        }

        Ok(json!({
            "content": [{"type": "text", "text": format!("{} apps open in the helper.", listed.len())}],
            "structuredContent": {"apps": listed},
        }))
    }

    /// The screenshots agents took, newest first.
    async fn screenshots(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        let now = crate::client::timestamp();
        let shots = tokio::task::spawn_blocking(move || captures(&output, SHOTS))
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(at, name)| {
                let seconds = now.saturating_sub(at) / 1_000_000_000;
                json!({"name": name, "ageSeconds": u64::try_from(seconds).unwrap_or(u64::MAX)})
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "content": [{"type": "text", "text": format!("{} screenshots.", shots.len())}],
            "structuredContent": {"screenshots": shots},
        }))
    }

    /// One screenshot an agent took, as a thumbnail or `full`.
    async fn screenshot(&self, name: &str, full: bool) -> Result<Value> {
        if capture_time(name).is_none() {
            return Err(Error::new("bad-request", "No screenshot has that name"));
        }

        let output = self.client.output_dir()?.to_path_buf();
        let wanted = name.to_owned();
        let image = tokio::task::spawn_blocking(move || -> Result<Value> {
            let png = read_capture(&output, &wanted)?;
            if full { frame(&png) } else { thumbnail(&png) }
        })
        .await
        .map_err(|_| Error::new("capture", "The screenshot could not be read"))??;

        Ok(json!({
            "content": [{"type": "text", "text": "A screenshot from the helper."}],
            "structuredContent": {"name": name},
            "_meta": {FRAME_META: image},
        }))
    }

    /// Lock the person's screen so they can log in to the helper there;
    /// refused (`not-ready`), with nothing locked, while a new login could
    /// hang.
    async fn switch_helper(&self) -> Result<Value> {
        let helper = self.client.config.helper_uid;
        tokio::task::spawn_blocking(move || {
            let account = crate::switch::accounts(Some(helper))?
                .into_iter()
                .find(|account| account.helper)
                .ok_or_else(|| Error::new("switch", "The helper's account wasn't found"))?;
            crate::switch::to(&account)
        })
        .await
        .map_err(|_| Error::new("switch", "The screen didn't lock"))??;

        Ok(json!({
            "content": [{"type": "text", "text": "The screen is locked. Choose the helper there."}],
            "structuredContent": {"switched": true},
        }))
    }

    /// Log the helper out, as Log Out in Atlas does: its apps quit as at
    /// any log-out.
    async fn log_out_helper(&self) -> Result<Value> {
        let reply = self.person().logout(false).await?;
        if reply["ok"] != true {
            return Err(Error::new(
                "logout",
                reply["error"]
                    .as_str()
                    .unwrap_or("The helper didn't log out")
                    .to_owned(),
            ));
        }

        let status = self.status().await?;
        Ok(said(&status))
    }

    /// Open Atlas's own viewer window on the person's desktop.
    async fn open_viewer(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        crate::viewer::open_viewer(&self.config, self.port, &output).await?;
        Ok(json!({
            "content": [{"type": "text", "text": "Atlas's viewer is open."}],
            "structuredContent": {"viewer": "open"},
        }))
    }

    /// The client for what the person does from the view: the person's
    /// only in a client that keeps the view's tools from the model, else
    /// whoever called it, as an agent.
    fn person(&self) -> Client {
        let mut client = self.client.clone();
        if self.views.load(Ordering::Relaxed) {
            client.actor = Actor::Viewer;
        }
        client
    }
}

/// The answer to `initialize`, in the client's protocol version when this
/// server speaks it.
/// An app's windows as the view shows them: each one's title and size only,
/// never its place on the screen or its number.
fn only_title_and_size(app: &mut Value) {
    for window in app["windows"].as_array_mut().into_iter().flatten() {
        *window = json!({
            "title": window["title"],
            "width": window["width"],
            "height": window["height"],
        });
    }
}

fn initialized(params: &Value) -> Value {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    let version = VERSIONS
        .iter()
        .find(|known| **known == asked)
        .unwrap_or(&VERSIONS[0]);

    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false}},
        "serverInfo": {"name": "atlas", "title": "Atlas", "version": crate::VERSION},
        "instructions": "Atlas runs Mac apps in a hidden helper login. Use the atlas command \
            line to act there (see the atlas skill). Call show_screen once when visible work in \
            the helper starts, so the person can watch it and stop it.",
    })
}

/// A tool's MCP Apps metadata: the view it shows, and who may call it.
fn ui(visibility: &[&str]) -> Value {
    json!({"resourceUri": SCREEN_URI, "visibility": visibility})
}

/// A tool's metadata when only the view may call it, in the MCP Apps
/// words and in OpenAI's, which the Codex app reads.
fn app_only() -> Value {
    json!({
        "ui": ui(&["app"]),
        "openai/visibility": "private",
        "openai/widgetAccessible": true,
    })
}

/// The tools: two the model sees, the rest only the view may call.
fn tools() -> Value {
    let none = json!({"type": "object", "properties": {}, "additionalProperties": false});
    let app = |name: &str, title: &str, description: &str, read_only: bool| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": none,
            "annotations": {"readOnlyHint": read_only, "openWorldHint": false},
            "_meta": app_only(),
        })
    };

    json!([
        {
            "name": "atlas_status",
            "title": "Atlas status",
            "description": "What is happening in Atlas's hidden helper now: whether an agent is \
                working there, whether the person stopped agents, and the last actions. Read-only.",
            "inputSchema": none,
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
        },
        {
            "name": "show_screen",
            "title": "Atlas",
            "description": "Show the person the helper's screen live, with what the agent is \
                doing and a Stop button. Call it once when visible work in the helper starts. \
                Read-only.",
            "inputSchema": none,
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
            "_meta": {
                "ui": ui(&["model", "app"]),
                "ui/resourceUri": SCREEN_URI,
                "openai/outputTemplate": SCREEN_URI,
                "openai/widgetAccessible": true,
                // The Codex app also offers it as a tab beside the thread.
                "openai/ui": {"entrypoints": [{"type": "thread"}]},
            },
        },
        {
            "name": "screen_frame",
            "title": "Helper screen",
            "description": "The helper's newest screen and status, unless the screen is still \
                the one named by since.",
            "inputSchema": {
                "type": "object",
                "properties": {"since": {"type": "string"}},
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
            "_meta": app_only(),
        },
        app("stop_agent", "Stop Agent", "Stop agents in the helper until the person resumes them.", false),
        app("resume_agent", "Resume Agent", "Let agents act in the helper again.", false),
        app("recover", "Recover", "Recover the helper, as Atlas's menu bar does.", false),
        app("open_viewer", "Open in Atlas", "Open Atlas's viewer window.", false),
        app("take_control", "Take Control", "Take control of the helper for the person; agents wait.", false),
        app("keep_control", "Keep Control", "Keep the person's control a while longer.", false),
        app("give_back", "Give Back", "End the person's control of the helper.", false),
        {
            "name": "send_input",
            "title": "Send input",
            "description": "Send the person's clicks, scrolls and keys to the helper while they have control.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "frame": {"type": "string"},
                    "actions": {"type": "array", "items": {"type": "object"}, "maxItems": ACTIONS},
                },
                "required": ["frame", "actions"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": false, "openWorldHint": false},
            "_meta": app_only(),
        },
        app("activity", "Activity", "Everything done in the helper lately, newest first.", true),
        app("helper_apps", "Apps", "The apps open in the helper and their windows.", true),
        app("screenshots", "Screenshots", "The screenshots agents took in the helper.", true),
        {
            "name": "screenshot",
            "title": "Screenshot",
            "description": "One screenshot an agent took, as a thumbnail or full size.",
            "inputSchema": {
                "type": "object",
                "properties": {"name": {"type": "string"}, "full": {"type": "boolean"}},
                "required": ["name"],
                "additionalProperties": false,
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false},
            "_meta": app_only(),
        },
        app("switch_helper", "Switch to the Helper", "Lock the screen so the person can log in to the helper.", false),
        app("log_out_helper", "Log Out the Helper", "Log the helper out; its apps quit.", false),
    ])
}

/// The view's metadata: no network, no border of the app's own.
fn screen_meta() -> Value {
    json!({
        "ui": {"csp": {"connectDomains": [], "resourceDomains": []}, "prefersBorder": false},
        "openai/widgetPrefersBorder": false,
        "openai/widgetDescription": "The helper's screen, live, with what the agent is doing.",
    })
}

fn screen_resource() -> Value {
    json!({
        "uri": SCREEN_URI,
        "name": "Atlas screen",
        "description": "The helper's screen, live, with what the agent is doing and Stop.",
        "mimeType": APP_MIME,
        "_meta": screen_meta(),
    })
}

fn screen_contents() -> Value {
    let page = SCREEN
        .replacen("/*SCRIPT*/", SCREEN_SCRIPT, 1)
        .replacen("/*LOGOS*/", logos(), 1);
    json!({"uri": SCREEN_URI, "mimeType": APP_MIME, "text": page, "_meta": screen_meta()})
}

/// The hosts' logos the view draws beside each session, as one line of
/// script holding a data URI for each: made once, from the app's own
/// images.
fn logos() -> &'static str {
    static LOGOS: OnceLock<String> = OnceLock::new();
    LOGOS.get_or_init(|| {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let images: [(&str, &[u8]); 3] = [
            (
                "claude",
                include_bytes!("../../../app/Resources/ClaudeLogo.png"),
            ),
            (
                "codex",
                include_bytes!("../../../app/Resources/CodexLogo.png"),
            ),
            (
                "cursor",
                include_bytes!("../../../app/Resources/CursorLogo.png"),
            ),
        ];
        let logos: serde_json::Map<String, Value> = images
            .into_iter()
            .map(|(host, png)| {
                let uri = format!("data:image/png;base64,{}", STANDARD.encode(png));
                (host.to_owned(), json!(uri))
            })
            .collect();
        format!("const LOGOS = {};", Value::Object(logos))
    })
}

/// The process that started this server.
fn parent_pid() -> i32 {
    nix::unistd::getppid().as_raw()
}

/// This chat's host session, only where Atlas is certain of it: a Claude
/// Code session whose host process started this server (`parent`) and
/// which Atlas has recorded, moved on to the newest session that process
/// recorded, as after `/clear`. While a session on its first command has
/// no record yet (`unrecorded`), it may be this chat's newest, so none is
/// named. Hosts that share one server among their chats name none.
fn own_session(
    own: Option<&HostSession>,
    seen: &[record::Seen],
    parent: i32,
    unrecorded: bool,
) -> Option<Key> {
    let own = own.filter(|own| own.host() == Host::Claude)?;
    let process = own.process().filter(|process| process.pid == parent)?;
    let recorded = seen.iter().any(|seen| seen.record.key == *own.key());
    if !recorded || unrecorded {
        return None;
    }

    seen.iter()
        .map(|seen| &seen.record)
        .filter(|record| record.host_process == Some(process))
        .max_by_key(|record| record.last_seen)
        .map(|record| record.key.clone())
}

/// The name of every recorded host session in `output`, as every surface
/// shows it.
fn session_names(output: &Path) -> BTreeMap<Key, String> {
    let recent = Store::at(output).recent();
    names(recent.iter().map(|seen| &seen.record))
}

/// The name of the host session a shown row names by its key `session`,
/// when its record is there: never its key or the host's own id.
fn session_name<'a>(session: Option<&str>, named: &'a BTreeMap<Key, String>) -> Option<&'a String> {
    named.get(&Key::parse(session?)?)
}

/// What a row names a session it came from that no list names: another
/// session, never the commands no host named.
const ANOTHER_SESSION: &str = "Another session";

/// What the view and the model read of a status: the overview's state, its
/// words and recent rows, without the doctor report around it. Rows name
/// their session, by `named`, only when they come from two sessions or
/// more or the apps speak of sessions, so one agent's status costs it
/// nothing more. The sessions are told by name and state, only as much as
/// the view draws and never by the host's own id; `own` is this chat's,
/// when certain.
fn summary(status: &Value, named: &BTreeMap<Key, String>, own: Option<&Key>) -> Value {
    let overview = &status["overview"];
    let shown = overview["activity"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let sessions_shown = overview["sessionsShown"] == true;
    let sessions: BTreeSet<&str> = shown
        .iter()
        .filter_map(|row| row["session"].as_str())
        .collect();
    // The overview's own list names the sessions its rows come from,
    // those on their first command included; records name the rest.
    let listed_names: BTreeMap<&str, &str> = overview["hostSessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|session| Some((session["key"].as_str()?, session["name"].as_str()?)))
        .collect();
    let rows = shown
        .iter()
        .map(|row| {
            let mut kept = json!({
                "says": row["says"],
                "ok": row["ok"],
                "actor": row["actor"],
                "ageSeconds": row["ageSeconds"],
                "nothingSent": row["nothingSent"] == true,
                "uncertain": row["uncertain"] == true,
            });
            if (sessions.len() > 1 || sessions_shown) && row["session"].is_string() {
                let name = row["session"]
                    .as_str()
                    .and_then(|key| listed_names.get(key).copied())
                    .or_else(|| session_name(row["session"].as_str(), named).map(String::as_str))
                    .unwrap_or(ANOTHER_SESSION);
                kept["session"] = json!(name);
            }
            kept
        })
        .collect::<Vec<_>>();
    let listed = overview["hostSessions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|session| {
            json!({
                "key": session["key"],
                "host": session["host"],
                "name": session["name"],
                "state": session["state"],
                "uncertainVerb": session["uncertainVerb"],
                "holdsHelper": session["holdsHelper"] == true,
            })
        })
        .collect::<Vec<_>>();

    // The view is the person's: a step that names a command reads as where
    // to look instead.
    let headline = match overview["headline"].as_str() {
        Some(text) if text.contains("atlas ") => {
            "The helper isn't ready. Open Atlas to see why".to_owned()
        }
        Some(text) => text.to_owned(),
        None => String::new(),
    };

    json!({
        "state": overview["state"],
        "label": overview["label"],
        "headline": headline,
        "reason": overview["reason"],
        "helper": overview["accountName"].as_str().or(overview["account"].as_str()),
        "display": overview["display"],
        "pending": overview["pending"],
        "commands": overview["commands"],
        "activity": rows,
        "sessionsShown": sessions_shown,
        "holder": overview["holder"],
        "self": own,
        "sessions": listed,
    })
}

/// A tool result for `status`: its words for the model, its facts for the
/// view.
fn said(status: &Value) -> Value {
    let label = status["label"].as_str().unwrap_or("Not ready");
    let headline = status["headline"].as_str().unwrap_or_default();
    let mut text = format!("Atlas: {}", sentence(label));
    if !headline.is_empty() {
        text.push_str(&format!(" {}", sentence(headline)));
    }
    let earlier = status["activity"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["actor"] != "viewer")
        .filter_map(|row| row["says"].as_str())
        .take(3)
        .collect::<Vec<_>>();
    if !earlier.is_empty() {
        text.push_str(&format!(" Recent: {}", sentence(&earlier.join("; "))));
    }

    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": {"status": status},
    })
}

/// `text` ended as a sentence: with a full stop, unless it already ends
/// with one, an ellipsis ("Running a command…") or another mark that ends
/// a sentence.
fn sentence(text: &str) -> String {
    if text.ends_with(['.', '…', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

/// The view's input after checking it is only what the person may send:
/// pointer, move, scroll, typing and keys, at most as many as the helper
/// takes at once. The helper's own checks (points on its display, typing
/// within its time) come when it is sent.
fn checked(actions: &Value) -> Result<&Value> {
    let refused = |why: &str| Error::new("input", why.to_owned());
    let list = actions
        .as_array()
        .filter(|list| (1..=ACTIONS).contains(&list.len()))
        .ok_or_else(|| refused("Send 1 to 32 actions"))?;

    if list
        .iter()
        .any(|action| !KINDS.contains(&action["type"].as_str().unwrap_or_default()))
    {
        return Err(refused("Only clicks, scrolls, typing and keys may be sent"));
    }

    Ok(actions)
}

#[cfg(test)]
mod tests;
