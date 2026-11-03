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
use atlas_client::{
    client::{Actor, Client},
    helper_info::Icons,
};
use atlas_core::error::{Code, Error, Result};
use atlas_core::host::HostSession;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
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

mod control;
mod guarded;
mod presentation;
mod reads;
mod resources;
use control::{Held, Seen, release};
use presentation::tool_result;
use resources::{
    Access, Audience, SCREEN_URI, Tool, UI_EXTENSION, initialized, screen_contents, screen_resource,
};

/// The longest request line read, in bytes.
const MAX_LINE: usize = 1 << 20;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const RESOURCE_NOT_FOUND: i64 = -32002;

/// A request line's text, or the JSON-RPC error that answers it.
type RequestLine = std::result::Result<String, Value>;

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

/// Serve requests from standard input until it closes.
///
/// # Errors
///
/// Returns stream or protocol-service failures. Individual tool refusals are returned
/// through MCP, and disconnect cleanup retains unknown mutation outcomes.
pub async fn serve(
    client: Client,
    config: PathBuf,
    port: u16,
    read_only: bool,
    guard_app_controls: bool,
) -> Result<()> {
    let server = Arc::new(Server::new(
        client,
        config,
        port,
        read_only,
        HostSession::from_env(),
    ));
    if guard_app_controls {
        server.require_view_contexts();
    }
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
                .map_err(|_| failure(&Value::Null, PARSE_ERROR, "Parse error"))
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
        registry.shut_down().await?;
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
) -> std::io::Result<Option<RequestLine>> {
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
) -> std::io::Result<Option<RequestLine>> {
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
fn finished(line: Vec<u8>, too_long: bool) -> RequestLine {
    let payload = line.strip_suffix(b"\n").unwrap_or(&line);
    let payload = payload.strip_suffix(b"\r").unwrap_or(payload);
    if too_long || payload.len() > MAX_LINE {
        return Err(failure(&Value::Null, INVALID_REQUEST, "Request too large"));
    }
    String::from_utf8(line)
        .map(|text| text.trim_end_matches(['\n', '\r']).to_owned())
        .map_err(|_| failure(&Value::Null, PARSE_ERROR, "Parse error"))
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
    fn new(
        client: Client,
        config: PathBuf,
        port: u16,
        read_only: bool,
        host_session: Option<HostSession>,
    ) -> Self {
        Self {
            client,
            config,
            port,
            read_only,
            app_views: OnceLock::new(),
            views: AtomicBool::new(false),
            last: Mutex::new(None),
            framed: Mutex::new(None),
            shared: Mutex::new(None),
            held: Mutex::new(None),
            icons: Icons::default(),
            host_session,
        }
    }

    /// From now on control needs a view's private context.
    fn require_view_contexts(&self) {
        self.app_views.get_or_init(Arc::default);
    }

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
            self.require_view_contexts();
        }
        let views = params["capabilities"]["extensions"][UI_EXTENSION].is_object();
        self.views.store(views, Ordering::Relaxed);
    }

    /// The answer to one message; none for a notification. Its negotiation
    /// was already applied by `prepare`, in read order.
    async fn answer(&self, message: &Value) -> Option<guarded::Reply> {
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
                "tools/call" => match self.offered(params["name"].as_str().unwrap_or_default()) {
                    Some(Tool::CloseView) | None => failure(id, INVALID_PARAMS, "Unknown tool"),
                    Some(tool) => success(id, self.call(tool, &params["arguments"]).await),
                },
                "resources/list" => success(id, json!({"resources": [screen_resource()]})),
                "resources/templates/list" => success(id, json!({"resourceTemplates": []})),
                "resources/read" if params["uri"] == SCREEN_URI => {
                    success(id, json!({"contents": [screen_contents()]}))
                }
                "resources/read" => failure(id, RESOURCE_NOT_FOUND, "Resource not found"),
                _ => failure(id, METHOD_NOT_FOUND, "Method not found"),
            },
        ))
    }

    /// The result of `tool`, an error result when it fails.
    async fn call(&self, tool: Tool, arguments: &Value) -> Value {
        self.presented(self.run(tool, arguments).await).await
    }

    /// Do what `tool` does.
    async fn run(&self, tool: Tool, arguments: &Value) -> Result<Value> {
        match tool {
            Tool::Status | Tool::ShowScreen => self.status_result().await,
            Tool::ScreenFrame => self.screen(arguments["since"].as_str()).await,
            Tool::StopAgent => self.set_stopped(true).await,
            Tool::ResumeAgent => self.set_stopped(false).await,
            Tool::Recover => self.recover().await,
            Tool::TakeControl => self.take_control().await,
            Tool::KeepControl => self.keep_control().await,
            Tool::GiveBack => self.give_back().await,
            Tool::SendInput => {
                self.send_input(arguments["frame"].as_str(), &arguments["actions"])
                    .await
            }
            Tool::Activity => self.activity().await,
            Tool::HelperApps => self.helper_apps().await,
            Tool::Screenshots => self.screenshots().await,
            Tool::Screenshot => {
                self.screenshot(
                    arguments["name"].as_str().unwrap_or_default(),
                    arguments["full"] == true,
                )
                .await
            }
            Tool::SwitchHelper => self.switch_helper().await,
            Tool::LogOutHelper => self.log_out_helper().await,
            Tool::OpenViewer => self.open_viewer().await,
            // A view's context closes in its registry, never here.
            Tool::CloseView => Err(Error::new(Code::BadRequest, "Unknown tool")),
        }
    }

    /// `outcome` as a tool result, with what the view may offer.
    async fn presented(&self, outcome: Result<Value>) -> Value {
        let mut result = outcome.unwrap_or_else(|error| {
            let mut failed = tool_result(&error.error, json!({"error": error}));
            failed["isError"] = json!(true);
            failed
        });
        // The view offers Resume and control only where this client may
        // call them, and knows whether the control shown is its own.
        let can_control = self.can_control();
        result["structuredContent"]["canResume"] = json!(can_control);
        result["structuredContent"]["canControl"] = json!(can_control);
        result["structuredContent"]["control"] = json!(self.controlling().await);
        result
    }

    /// The tools this client is offered: Resume only to one that draws
    /// views.
    fn tools(&self) -> Value {
        let views = self.draws_views();
        let contexts = self.uses_view_contexts();
        let listed = Tool::ALL
            .into_iter()
            .filter(|tool| self.offers_tool(*tool, views))
            .map(|tool| {
                if contexts {
                    guarded::catalog(tool)
                } else {
                    tool.catalog()
                }
            })
            .collect::<Vec<_>>();
        json!(listed)
    }

    /// The tool `name` calls, when this client is offered it.
    fn offered(&self, name: &str) -> Option<Tool> {
        Tool::parse(name).filter(|tool| self.offers_tool(*tool, self.draws_views()))
    }

    fn offers_tool(&self, tool: Tool, views: bool) -> bool {
        let (audience, access) = tool.policy();
        let reached = match audience {
            Audience::Any => true,
            Audience::Views => views,
            Audience::ViewContexts => self.uses_view_contexts(),
        };
        reached
            && (!self.read_only || access == Access::Read)
            && (self.app_views.get().is_none() || views || access == Access::Read)
    }

    fn draws_views(&self) -> bool {
        self.views.load(Ordering::Relaxed)
    }

    /// Whether this client may offer the person Resume and control.
    fn can_control(&self) -> bool {
        self.draws_views() && !self.read_only
    }

    /// Whether this client's views control through private contexts.
    fn uses_view_contexts(&self) -> bool {
        self.app_views.get().is_some() && self.can_control()
    }

    /// Stop agents, or let them go on: the person's, from the view.
    async fn set_stopped(&self, stopped: bool) -> Result<Value> {
        self.person().hold(stopped).await?;
        self.status_result().await
    }

    /// Recover the helper, as the menu bar's Recover does: restart it when
    /// an action's outcome is unknown, else start its input.
    async fn recover(&self) -> Result<Value> {
        self.person().recover_helper().await?;
        self.status_result().await
    }

    /// Lock the person's screen so they can log in to the helper there;
    /// refused (`not-ready`), with nothing locked, while a new login could
    /// hang.
    async fn switch_helper(&self) -> Result<Value> {
        let helper = self.client.config.helper_uid;
        tokio::task::spawn_blocking(move || {
            let account = atlas_client::switch::accounts(Some(helper))?
                .into_iter()
                .find(|account| account.helper)
                .ok_or_else(|| Error::new(Code::Switch, "The helper's account wasn't found"))?;
            atlas_client::switch::to(&account)
        })
        .await
        .map_err(|error| Error::new(Code::Switch, "The screen didn't lock").with_source(error))??;

        Ok(tool_result(
            "The screen is locked. Choose the helper there.",
            json!({"switched": true}),
        ))
    }

    /// Log the helper out, as Log Out in Atlas does: its apps quit as at
    /// any log-out.
    async fn log_out_helper(&self) -> Result<Value> {
        let reply = self.person().logout(false).await?;
        if reply["ok"] != true {
            return Err(Error::new(
                Code::Logout,
                reply["error"]
                    .as_str()
                    .unwrap_or("The helper didn't log out"),
            ));
        }

        self.status_result().await
    }

    /// Open Atlas's own viewer window on the person's desktop.
    async fn open_viewer(&self) -> Result<Value> {
        let output = self.client.output_dir()?.to_path_buf();
        crate::viewer::open_viewer(&self.config, self.port, &output).await?;
        Ok(tool_result(
            "Atlas's viewer is open.",
            json!({"viewer": "open"}),
        ))
    }

    /// The client for what the person does from the view: the person's
    /// only in a client that keeps the view's tools from the model, else
    /// whoever called it, as an agent.
    fn person(&self) -> Client {
        let mut client = self.client.clone();
        if self.draws_views() {
            client.actor = Actor::Viewer;
        }
        client
    }
}

#[cfg(test)]
mod tests;
