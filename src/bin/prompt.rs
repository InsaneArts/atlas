//! Questions for the person: yes or no, one choice from a list, or a short
//! answer. Each needs a terminal; without one it refuses with what to pass
//! instead, so nothing ever waits on a pipe or answers itself, and keys
//! pressed before a question appears are discarded. Esc cancels a yes/no or
//! a choice; Ctrl-C stops any question. Atlas never asks for a
//! password: sudo and macOS ask in their own words.
use atlas::{
    error::{Code, Error, Result},
    ui::interactive,
};
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
        Code::Terminal,
        format!("This asks you a question, so run it in a terminal{instead}."),
    )
}

/// The person stopped at a question.
pub(super) fn cancelled() -> Error {
    Error::new(Code::Cancelled, "Stopped.")
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
///
/// The handler is `stop_cleanly_on_interrupt` in main.rs, installed for the
/// commands that ask questions; the second's sleep below only gives it time
/// to print and exit. Change the two together.
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

/// Refuse without a terminal, saying to do `instead`; otherwise make the
/// terminal ready for a question.
fn ready(instead: &str) -> Result<()> {
    if !interactive() {
        return Err(needs_terminal(instead));
    }

    discard_typed_ahead();
    Ok(())
}

/// Yes or no. `instead` says how to answer without a terminal.
///
/// # Errors
///
/// Returns a terminal-required error without an interactive terminal, cancellation when no
/// answer is chosen, or an input/output failure.
pub(super) fn confirm(question: &str, default: bool, instead: &str) -> Result<bool> {
    ready(instead)?;
    Confirm::with_theme(&theme())
        .with_prompt(question)
        .default(default)
        .interact_opt()
        .map_err(failed)?
        .ok_or_else(cancelled)
}

/// One of `items`, by index.
///
/// # Errors
///
/// Returns a terminal-required error without an interactive terminal, cancellation when no
/// choice is made, or an input/output failure.
pub(super) fn select(
    question: &str,
    items: &[String],
    default: usize,
    instead: &str,
) -> Result<usize> {
    ready(instead)?;
    Select::with_theme(&theme())
        .with_prompt(question)
        .items(items)
        .default(default)
        .interact_opt()
        .map_err(failed)?
        .ok_or_else(cancelled)
}

/// A short answer, checked by `valid` before it is accepted.
///
/// # Errors
///
/// Returns a terminal-required error, cancellation or input/output failure. Invalid answers
/// are checked by the supplied validator before acceptance.
pub(super) fn input(
    question: &str,
    default: &str,
    valid: impl Fn(&str) -> std::result::Result<(), String>,
    instead: &str,
) -> Result<String> {
    ready(instead)?;
    Input::<String>::with_theme(&theme())
        .with_prompt(question)
        .default(default.to_owned())
        .validate_with(|answer: &String| valid(answer.trim()))
        .interact_text()
        .map(|answer| answer.trim().to_owned())
        .map_err(failed)
}
