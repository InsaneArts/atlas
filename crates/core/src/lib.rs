//! The facts every Atlas layer shares: names, errors, time, wire types,
//! configuration, observed screens and sessions, and how Atlas is installed.
//!
//! Nothing here reaches up to a macOS call or a controller decision. A
//! dependency points only down the workspace: this crate, then
//! `atlas-native`, `atlas-daemon`, `atlas-control`, `atlas-client`,
//! `atlas-agents` and `atlas-cli`.

pub mod account;
pub mod acl;
pub mod actions;
pub mod app_bundle;
pub mod bounded;
pub mod buffer;
pub mod config;
pub mod context;
pub mod desktop;
pub mod digest;
pub mod error;
pub mod files;
pub mod frame_pipe;
#[allow(unsafe_code, reason = "FFI: libc's renameatx_np has no safe binding")]
pub mod fs_move;
pub mod host;
pub mod lenient;
pub mod login_log;
pub mod name;
pub mod process;
pub mod protocol;
pub mod record_folder;
pub mod screen;
pub mod session;
pub mod single_flight;
pub mod spawn;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod time;
pub mod transport;

/// This build's version, as released.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
