//! Agent-facing commands: small, stable verbs over the guarded input path,
//! the helper's window list, accessibility elements, app launching and
//! waiting for windows. Every action is an ordinary validated batch with the
//! usual ownership and epoch checks; nothing here bypasses them.
mod elements;
mod expect;
mod guide;
mod input;
mod launch;
mod quit;
mod target;
mod windows;

pub use crate::app_selection::Closing;
pub use crate::select::ElementFilter;
pub use elements::{click_element, elements, screenshot};
pub use expect::{Expected, act};
#[cfg(test)]
pub(crate) use guide::CORE;
pub(crate) use guide::REFERENCES;
pub use guide::{ERRORS, GUIDE, skill};
pub use input::{click, drag, keys, move_to, scroll, type_text};
pub use launch::open;
pub use quit::close;
pub(crate) use quit::{QUIT_WAIT, Quitting, quit_until};
pub use target::Target;
pub use windows::{WindowFilter, list, wait};

#[cfg(test)]
mod tests;
