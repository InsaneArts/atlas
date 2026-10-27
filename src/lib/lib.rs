pub mod acl;
pub mod actions;
pub mod agent;
pub mod agents;
mod app_selection;
pub mod approval;
pub mod capability;
pub mod claude_app;
pub mod client;
pub mod config;
pub mod control;
pub mod daemon;
pub mod digest;
pub mod doing;
pub mod environment;
pub mod error;
pub mod handoff;
pub mod helper_info;
pub mod init;
mod last_capture;
pub mod login_log;
pub mod mcp;
pub mod menubar;
pub mod native;
pub mod onboarding;
pub mod overview;
pub mod readiness;
pub mod session;
pub mod setup;
pub mod switch;
pub mod transport;
pub mod ui;
pub mod uninstall;
pub mod update;
pub mod viewer;
pub mod watch;

#[cfg(test)]
mod transaction_test;

/// This build's version, as released.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
