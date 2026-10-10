//! A few marks and colours, used the same way everywhere: ✓ done, ✗ failed,
//! – unknown or skipped, ! a warning, and commands in cyan. Colour is off
//! for programs, with `NO_COLOR`, on a dumb terminal, or when either output
//! is not a terminal.
use super::Format;
use console::style;
use std::{fmt::Display, io::IsTerminal};

pub(super) fn init(format: Format) {
    let wanted = format == Format::Human
        && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && std::env::var("TERM").map_or(true, |term| term != "dumb");
    // Both streams share one setting, so styled text reads the same on either.
    let on = wanted && std::io::stdout().is_terminal() && std::io::stderr().is_terminal();
    console::set_colors_enabled(on);
    console::set_colors_enabled_stderr(on);
}

pub fn ok() -> String {
    style("✓").green().to_string()
}

pub fn fail() -> String {
    style("✗").red().to_string()
}

pub fn unknown() -> String {
    style("–").dim().to_string()
}

pub fn warn() -> String {
    style("!").yellow().bold().to_string()
}

/// The mark for a yes/no/unknown fact.
pub fn mark(fact: Option<bool>) -> String {
    match fact {
        Some(true) => ok(),
        Some(false) => fail(),
        None => unknown(),
    }
}

/// A command the person can type.
pub fn cmd(text: impl Display) -> String {
    style(text).cyan().to_string()
}

pub fn dim(text: impl Display) -> String {
    style(text).dim().to_string()
}

pub fn bold(text: impl Display) -> String {
    style(text).bold().to_string()
}
