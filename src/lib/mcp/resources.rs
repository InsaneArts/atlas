use super::ACTIONS;
use serde_json::{Value, json};
use std::sync::OnceLock;

/// The MCP Apps extension, as a client declares it draws views.
pub(super) const UI_EXTENSION: &str = "io.modelcontextprotocol/ui";

/// Where a result carries the helper's screen for the view.
pub(super) const FRAME_META: &str = "atlas/frame";

/// The view's address and its MCP Apps type.
pub(super) const SCREEN_URI: &str = "ui://atlas/screen.html";

pub(super) const APP_MIME: &str = "text/html;profile=mcp-app";

/// The view: one page, its script inlined where it marks.
pub(super) const SCREEN: &str = include_str!("screen.html");

pub(super) const SCREEN_SCRIPT: &str = include_str!("screen.js");

/// MCP versions this server speaks; it answers in the client's when it is
/// one of them.
pub(super) const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub(super) fn initialized(params: &Value) -> Value {
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
pub(super) fn ui(visibility: &[&str]) -> Value {
    json!({"resourceUri": SCREEN_URI, "visibility": visibility})
}

/// A tool's metadata when only the view may call it, in the MCP Apps
/// words and in OpenAI's, which the Codex app reads.
pub(super) fn app_only() -> Value {
    json!({
        "ui": ui(&["app"]),
        "openai/visibility": "private",
        "openai/widgetAccessible": true,
    })
}

/// The tools: two the model sees, the rest only the view may call.
pub(super) fn tools() -> Value {
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
pub(super) fn screen_meta() -> Value {
    json!({
        "ui": {"csp": {"connectDomains": [], "resourceDomains": []}, "prefersBorder": false},
        "openai/widgetPrefersBorder": false,
        "openai/widgetDescription": "The helper's screen, live, with what the agent is doing.",
    })
}

pub(super) fn screen_resource() -> Value {
    json!({
        "uri": SCREEN_URI,
        "name": "Atlas screen",
        "description": "The helper's screen, live, with what the agent is doing and Stop.",
        "mimeType": APP_MIME,
        "_meta": screen_meta(),
    })
}

pub(super) fn screen_contents() -> Value {
    let page = SCREEN
        .replacen("/*SCRIPT*/", SCREEN_SCRIPT, 1)
        .replacen("/*LOGOS*/", logos(), 1);
    json!({"uri": SCREEN_URI, "mimeType": APP_MIME, "text": page, "_meta": screen_meta()})
}

/// The hosts' logos the view draws beside each session, as one line of
/// script holding a data URI for each: made once, from the app's own
/// images.
pub(super) fn logos() -> &'static str {
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
