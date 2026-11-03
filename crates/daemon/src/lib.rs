//! What runs inside the helper login: the daemon that serves capture, reads,
//! commands and jobs, and the helper's layout, launchd actions, first login
//! and pairing. It never reaches a controller module.

pub mod daemon;
pub mod helper_setup;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
