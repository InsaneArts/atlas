//! Atlas's surfaces: the viewer service, the MCP server, the menu bar,
//! onboarding and the formatting of everything the `atlas` command prints.

#[allow(
    dead_code,
    reason = "the frame service that uses these parts is built on them next"
)]
pub(crate) mod frame_pipe;
pub mod mcp;
pub mod menubar;
pub mod onboarding;
pub mod terminal;
pub mod ui;
pub mod uninstall;
pub mod viewer;
pub mod watch;
