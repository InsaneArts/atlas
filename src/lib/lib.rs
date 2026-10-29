pub mod acl;
pub mod actions;
pub mod agent;
pub mod agents;
mod app_selection;
pub mod approval;
mod bounded;
pub mod capability;
pub mod claude_app;
pub mod cleanup;
pub mod client;
pub mod collect;
pub mod config;
pub mod control;
pub mod daemon;
pub mod digest;
pub mod doing;
pub mod environment;
pub mod error;
pub mod files;
#[allow(unsafe_code, reason = "FFI: libc's renameatx_np has no safe binding")]
mod fs_move;
pub mod handoff;
pub mod helper_info;
pub mod host_session;
pub mod init;
pub mod installs;
pub mod interrupt;
pub mod jobs;
mod last_capture;
pub mod login_log;
pub mod mcp;
pub mod menubar;
mod name;
pub mod native;
pub mod onboarding;
pub mod overview;
pub mod process;
pub mod readiness;
mod record_folder;
mod select;
pub mod session;
mod session_view;
pub mod setup;
pub mod switch;
pub mod task;
mod text;
pub mod transport;
pub mod ui;
pub mod uninstall;
pub mod update;
pub mod viewer;
pub mod watch;

#[cfg(test)]
mod helper_test;
#[cfg(test)]
mod transaction_test;

/// This build's version, as released.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
