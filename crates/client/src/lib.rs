//! Observe and control a configured hidden macOS helper login.
//!
//! A successful Rust [`atlas_core::error::Result`] containing JSON is a
//! received result, not necessarily a successful helper action: inspect the
//! reply's `ok` field. Operations can retain individual failures in their JSON
//! report. Inspect `uncertain` on both errors and replies; a missing
//! acknowledgement never makes an input batch or filesystem mutation safe to
//! replay.
//!
//! [`client::Client`] verifies identity and owns controller-side operations;
//! [`atlas_control`] retains exclusion and pending work through cleanup.
//! Observations from [`atlas_core::session`] describe one epoch and must be
//! rechecked before effects.

pub mod activity_log;
pub mod agent;
pub mod app_selection;
pub mod before;
pub mod capability;
pub mod cleanup;
pub mod client;
pub mod collect;
pub mod console_agents;
mod credential_files;
pub mod doing;
pub mod environment;
pub mod handoff;
pub mod helper_info;
pub mod home_privacy;
pub mod host_session;
pub mod install_app;
pub mod jobs;
mod last_capture;
pub mod overview;
pub mod readiness;
pub mod session_view;
pub mod setup;
pub mod sign_in_copies;
pub mod sound;
pub mod switch;
pub mod task;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod update;
