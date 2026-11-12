//! What runs inside the helper login: the daemon that serves capture, reads,
//! commands and jobs, the helper's own paths, and its layout, launchd
//! actions, first login and pairing. It never reaches a controller module.

pub mod daemon;
pub mod helper_paths;
pub mod helper_setup;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
