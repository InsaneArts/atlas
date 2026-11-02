use super::control::ACTIONS;
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

#[derive(Clone, Copy)]
pub(super) enum Audience {
    Any,
    Views,
    /// Only views that control through a private context.
    ViewContexts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Access {
    Read,
    Change,
}

enum Metadata {
    Public,
    Screen,
    App,
    /// Only a view's own context reaches it: no title or hints.
    Context,
}

/// A tool's identity, catalog and exposure decisions are declared together.
macro_rules! tools {
    ($none:ident; $($tool:ident => ($name:literal, $title:literal, $description:literal,
        $audience:ident, $access:ident, $schema:expr, $metadata:ident)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(super) enum Tool { $($tool),+ }

        impl Tool {
            pub(super) const ALL: [Self; [$($name),+].len()] = [$(Self::$tool),+];

            pub(super) fn parse(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$tool),)+ _ => None }
            }

            pub(super) fn policy(self) -> (Audience, Access) {
                match self { $(Self::$tool => (Audience::$audience, Access::$access)),+ }
            }

            pub(super) fn catalog(self) -> Value {
                let $none = json!({"type": "object", "properties": {}, "additionalProperties": false});
                let (mut catalog, metadata) = match self {
                    $(Self::$tool => (json!({
                        "name": $name,
                        "title": $title,
                        "description": $description,
                        "inputSchema": $schema,
                        "annotations": {"readOnlyHint": Access::$access == Access::Read, "openWorldHint": false},
                    }), Metadata::$metadata)),+
                };
                match metadata {
                    Metadata::Public => {}
                    Metadata::App => catalog["_meta"] = app_only(),
                    Metadata::Context => catalog = json!({
                        "name": catalog["name"],
                        "description": catalog["description"],
                        "inputSchema": catalog["inputSchema"],
                        "_meta": app_only(),
                    }),
                    Metadata::Screen => catalog["_meta"] = json!({
                        "ui": ui(&["model", "app"]),
                        "ui/resourceUri": SCREEN_URI,
                        "openai/outputTemplate": SCREEN_URI,
                        "openai/widgetAccessible": true,
                        "openai/ui": {"entrypoints": [{"type": "thread"}]},
                    }),
                }
                catalog
            }
        }
    };
}

tools! { none;
    Status => ("atlas_status", "Atlas status", "What is happening in Atlas's hidden helper now: whether an agent is working there, whether the person stopped agents, and the last actions. Read-only.", Any, Read, none, Public),
    ShowScreen => ("show_screen", "Atlas", "Show the person the helper's screen live, with what the agent is doing and a Stop button. Call it once when visible work in the helper starts. Read-only.", Any, Read, none, Screen),
    ScreenFrame => ("screen_frame", "Helper screen", "The helper's newest screen and status, unless the screen is still the one named by since.", Any, Read, json!({"type": "object", "properties": {"since": {"type": "string"}}, "additionalProperties": false}), App),
    StopAgent => ("stop_agent", "Stop Agent", "Stop agents in the helper until the person resumes them.", Any, Change, none, App),
    ResumeAgent => ("resume_agent", "Resume Agent", "Let agents act in the helper again.", Views, Change, none, App),
    Recover => ("recover", "Recover", "Recover the helper, as Atlas's menu bar does.", Any, Change, none, App),
    OpenViewer => ("open_viewer", "Open in Atlas", "Open Atlas's viewer window.", Any, Change, none, App),
    TakeControl => ("take_control", "Take Control", "Take control of the helper for the person; agents wait.", Views, Change, none, App),
    KeepControl => ("keep_control", "Keep Control", "Keep the person's control a while longer.", Views, Change, none, App),
    GiveBack => ("give_back", "Give Back", "End the person's control of the helper.", Views, Change, none, App),
    SendInput => ("send_input", "Send input", "Send the person's clicks, scrolls and keys to the helper while they have control.", Views, Change, json!({"type": "object", "properties": {"frame": {"type": "string"}, "actions": {"type": "array", "items": {"type": "object"}, "maxItems": ACTIONS}}, "required": ["frame", "actions"], "additionalProperties": false}), App),
    Activity => ("activity", "Activity", "Everything done in the helper lately, newest first.", Views, Read, none, App),
    HelperApps => ("helper_apps", "Apps", "The apps open in the helper and their windows.", Views, Read, none, App),
    Screenshots => ("screenshots", "Screenshots", "The screenshots agents took in the helper.", Views, Read, none, App),
    Screenshot => ("screenshot", "Screenshot", "One screenshot an agent took, as a thumbnail or full size.", Views, Read, json!({"type": "object", "properties": {"name": {"type": "string"}, "full": {"type": "boolean"}}, "required": ["name"], "additionalProperties": false}), App),
    SwitchHelper => ("switch_helper", "Switch to the Helper", "Lock the screen so the person can log in to the helper.", Views, Change, none, App),
    LogOutHelper => ("log_out_helper", "Log Out the Helper", "Log the helper out; its apps quit.", Views, Change, none, App),
    CloseView => ("view_close", "", "Close this Atlas view's private context.", ViewContexts, Change, none, Context),
}

/// The view's metadata: no network, no border of the app's own.
fn screen_meta() -> Value {
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

#[cfg(test)]
#[test]
fn the_tool_catalog_keeps_its_fields_and_order() -> crate::error::Result<()> {
    let expected: Value = serde_json::from_str(include_str!("tools.fixture.json"))?;
    let catalog = Tool::ALL
        .into_iter()
        .filter(|tool| *tool != Tool::CloseView)
        .map(Tool::catalog)
        .collect::<Vec<_>>();
    assert_eq!(json!(catalog), expected);
    Ok(())
}
