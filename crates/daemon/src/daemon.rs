//! Atlas's helper daemon: the process inside the helper login that
//! macOS grants Screen Recording and Accessibility. It answers protocol v1 —
//! session facts, screenshots, running apps, commands, background jobs and
//! reads of files to collect — on a private socket, and starts the input
//! service as its child.
#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod consent;
#[cfg(target_os = "macos")]
mod crop;
#[cfg(target_os = "macos")]
pub mod facts;
pub mod job;
#[cfg(target_os = "macos")]
mod preferences;
pub mod proof;
#[cfg(target_os = "macos")]
pub(crate) mod read;
mod run;
#[cfg(target_os = "macos")]
pub mod serve;

#[cfg(not(target_os = "macos"))]
pub mod serve {
    use atlas_core::error::{Code, Error, Result};

    /// The helper daemon runs on macOS only.
    pub fn serve_blocking(_: u32, _: u32, _: std::path::PathBuf) -> Result<()> {
        Err(Error::new(
            Code::Platform,
            "The helper daemon requires macOS",
        ))
    }
}
