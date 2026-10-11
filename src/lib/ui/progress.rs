//! Progress for a person: a spinner while something takes a while, then one
//! line per finished step (`✓ Helper job  installed`). Everything goes to
//! standard error, so results on standard output stay clean; programs see
//! none of it.
use super::{human, style};
use indicatif::{ProgressBar, ProgressStyle};
use std::{io::IsTerminal, time::Duration};

/// A line for the person, on standard error; nothing for programs.
pub fn say(text: &str) {
    if human() {
        eprintln!("{text}");
    }
}

/// A heading that starts a part of a longer flow.
pub fn heading(text: &str) {
    say(&format!("\n{}", style::bold(text)));
}

/// A finished step: `✓ label  detail`.
pub fn done(label: &str, detail: &str) {
    say(&step(&style::ok(), label, detail));
}

/// A step left out, and why.
pub fn skipped(label: &str, detail: &str) {
    say(&step(&style::unknown(), label, detail));
}

fn step(mark: &str, label: &str, detail: &str) -> String {
    if detail.is_empty() {
        format!("{mark} {label}")
    } else {
        format!("{mark} {label}  {}", style::dim(detail))
    }
}

/// A spinner with the time spent, while the person waits. Without a
/// terminal it says what it waits for once, and nothing for programs.
pub struct Spinner {
    bar: Option<ProgressBar>,
}

impl Spinner {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        if !human() {
            return Self { bar: None };
        }
        if !std::io::stderr().is_terminal() {
            eprintln!("{message}");
            return Self { bar: None };
        }
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            // The message is cut to the terminal's width: a wrapped line
            // would be redrawn over the lines above it.
            ProgressStyle::with_template("{spinner:.cyan} {wide_msg} {elapsed:.dim}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
        );
        bar.set_message(message);
        bar.enable_steady_tick(Duration::from_millis(90));
        Self { bar: Some(bar) }
    }

    /// Change what the spinner says.
    pub fn set(&self, message: impl Into<String>) {
        if let Some(bar) = &self.bar {
            bar.set_message(message.into());
        }
    }

    /// Run `work` with the spinner out of the way, as when another program
    /// (sudo) asks the person something on the same terminal.
    pub fn suspend<R>(&self, work: impl FnOnce() -> R) -> R {
        match &self.bar {
            Some(bar) => bar.suspend(work),
            None => work(),
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
}
