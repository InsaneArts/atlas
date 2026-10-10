//! Understudy's helper daemon: the process inside the helper login that
//! macOS grants Screen Recording and Accessibility. It answers protocol v1 —
//! session facts, screenshots, running apps and commands — on a private
//! socket, and starts the input service as its child.
#[cfg(target_os = "macos")]
pub mod facts;
pub mod protocol;
pub mod run;
#[cfg(target_os = "macos")]
pub mod serve;
