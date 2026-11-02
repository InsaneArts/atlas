//! Observe and control a configured hidden macOS helper login.
//!
//! A successful Rust [`error::Result`] containing JSON is a received result,
//! not necessarily a successful helper action: inspect the reply's `ok` field.
//! Operations can retain individual failures in their JSON report. Inspect
//! `uncertain` on both errors and replies; a missing acknowledgement never
//! makes an input batch or filesystem mutation safe to replay.
//!
//! [`client::Client`] verifies identity and owns controller-side operations;
//! [`control`] retains exclusion and pending work through cleanup. Observations
//! from [`session`] describe one epoch and must be rechecked before effects.

pub mod account;
pub(crate) mod acl;
pub mod actions;
pub mod activity_log;
pub mod agent;
pub mod agents;
pub mod app_bundle;
pub mod app_selection;
pub(crate) mod approval;
mod bounded;
pub mod capability;
pub mod claude_app;
pub mod cleanup;
pub mod client;
pub mod collect;
pub mod config;
mod context;
pub mod control;
pub mod daemon;
mod digest;
pub mod doing;
pub mod environment;
pub mod error;
pub mod files;
#[allow(unsafe_code, reason = "FFI: libc's renameatx_np has no safe binding")]
mod fs_move;
pub(crate) mod handoff;
pub(crate) mod helper_info;
pub mod helper_setup;
mod host;
pub mod host_session;
pub mod install_app;
pub mod interrupt;
pub mod jobs;
mod last_capture;
mod lenient;
pub mod login_log;
pub mod mcp;
pub mod menubar;
mod name;
pub mod native;
pub mod onboarding;
pub(crate) mod overview;
pub mod process;
pub mod protocol;
pub mod readiness;
mod record_folder;
mod screen;
pub mod session;
mod session_view;
pub mod setup;
mod single_flight;
pub mod switch;
pub mod task;
mod time;
pub mod transport;
pub mod ui;
pub mod uninstall;
pub mod update;
pub mod viewer;
pub mod watch;

#[cfg(test)]
mod test_support;

/// This build's version, as released.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
