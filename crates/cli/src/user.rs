//! The user desktop, from the command line's library: its setup, which the
//! Atlas window's onboarding drives; its removal's account steps; and
//! showing it to the person from the MCP server. Shared code reaches it
//! through owners.
pub mod onboarding;
pub(crate) mod switch;
pub(crate) mod uninstall;
