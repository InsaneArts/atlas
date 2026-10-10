//! Questions for the person: yes or no, one choice from a list, or a short
//! answer. Each needs a terminal; without one it refuses with what to pass
//! instead, so nothing ever waits on a pipe or answers itself, and keys
//! pressed before a question appears are discarded. Esc cancels a yes/no or
//! a choice; Ctrl-C stops any question. Understudy never asks for a
//! password: sudo and macOS ask in their own words.
use super::interactive;
use crate::error::{Error, Result};
use console::style;
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

fn theme() -> ColorfulTheme {
    ColorfulTheme {
        success_prefix: style("✓".to_owned()).green(),
        error_prefix: style("✗".to_owned()).red(),
        ..ColorfulTheme::default()
    }
}

fn needs_terminal(instead: &str) -> Error {
    let instead = if instead.is_empty() {
        String::new()
    } else {
        format!(", or {instead}")
    };
    Error::new(
        "terminal",
        format!("This asks you a question, so run it in a terminal{instead}."),
    )
}

/// The person stopped at a question.
pub fn cancelled() -> Error {
    Error::new("cancelled", "Stopped.")
}

/// Keys pressed before the question appeared (a stray Return during a long
/// wait) must not answer it.
fn discard_typed_ahead() {
    use std::os::fd::AsFd;
    let _ = nix::sys::termios::tcflush(
        std::io::stdin().as_fd(),
        nix::sys::termios::FlushArg::TCIFLUSH,
    );
}

/// A question that ended without an answer, with the cursor back. Ctrl-C
/// raises SIGINT, whose handler reports it and exits; this only waits for
/// that, so the person sees one message. Input that ended is a cancel.
fn failed(error: dialoguer::Error) -> Error {
    let _ = console::Term::stderr().show_cursor();
    let dialoguer::Error::IO(error) = error;
    match error.kind() {
        std::io::ErrorKind::Interrupted => {
            std::thread::sleep(std::time::Duration::from_secs(1));
            cancelled()
        }
        std::io::ErrorKind::UnexpectedEof => cancelled(),
        _ => error.into(),
    }
}

/// Yes or no. `instead` says how to answer without a terminal.
pub fn confirm(question: &str, default: bool, instead: &str) -> Result<bool> {
    if !interactive() {
        return Err(needs_terminal(instead));
    }
    discard_typed_ahead();
    Confirm::with_theme(&theme())
        .with_prompt(question)
        .default(default)
        .interact_opt()
        .map_err(failed)?
        .ok_or_else(cancelled)
}

/// One of `items`, by index.
pub fn select(question: &str, items: &[String], default: usize, instead: &str) -> Result<usize> {
    if !interactive() {
        return Err(needs_terminal(instead));
    }
    discard_typed_ahead();
    Select::with_theme(&theme())
        .with_prompt(question)
        .items(items)
        .default(default)
        .interact_opt()
        .map_err(failed)?
        .ok_or_else(cancelled)
}

/// A short answer, checked by `valid` before it is accepted.
pub fn input(
    question: &str,
    default: &str,
    valid: impl Fn(&str) -> std::result::Result<(), String>,
    instead: &str,
) -> Result<String> {
    if !interactive() {
        return Err(needs_terminal(instead));
    }
    discard_typed_ahead();
    Input::<String>::with_theme(&theme())
        .with_prompt(question)
        .default(default.to_owned())
        .validate_with(|answer: &String| valid(answer.trim()))
        .interact_text()
        .map(|answer| answer.trim().to_owned())
        .map_err(failed)
}
