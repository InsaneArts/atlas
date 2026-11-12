//! The user desktop, from this Mac: a hidden user here, which people know as
//! Second Login. What only it needs on the controller's side lives here;
//! shared code reaches it through owners that match on the desktop's kind.
pub mod capability;
pub mod console_agents;
pub mod copies;
pub mod environment;
pub mod home_privacy;
pub(crate) mod install;
pub(crate) mod service;
pub mod setup;
pub(crate) mod shared;
pub mod switch;
