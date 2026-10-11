//! How Atlas talks to people and to programs. On a terminal it writes
//! short plain text, asks one question at a time and shows progress; for
//! programs (a pipe, `--json`, `ATLAS_JSON=1`) every result stays the
//! same JSON it always was, so agents and scripts never see formatting.
pub mod hints;
pub mod progress;
pub mod prompt;
pub mod render;
pub mod style;
pub mod table;
pub mod text;

use std::{io::IsTerminal, sync::OnceLock};

/// Who reads the output: a person at a terminal, or a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
}

static FORMAT: OnceLock<Format> = OnceLock::new();

/// `--json` wins; then `ATLAS_JSON` (`1` for JSON, `0` for text even
/// through a pipe); otherwise text only when the output is a terminal.
pub fn detect(json: bool, variable: Option<&str>, terminal: bool) -> Format {
    if json {
        return Format::Json;
    }
    match variable
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1" | "true" | "yes") => Format::Json,
        Some("0" | "false" | "no") => Format::Human,
        _ if terminal => Format::Human,
        _ => Format::Json,
    }
}

/// The format for this run; set once, before anything is written.
pub fn set(format: Format) {
    if FORMAT.set(format).is_ok() {
        style::init(format);
    }
}

/// The format for this run; JSON until chosen, so library use stays quiet.
pub fn format() -> Format {
    FORMAT.get().copied().unwrap_or(Format::Json)
}

/// Whether a person reads the output.
pub fn human() -> bool {
    format() == Format::Human
}

/// Whether a person can be asked: text output, and a terminal to read the
/// answer from and draw the question on.
pub fn interactive() -> bool {
    human() && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// What to try next once Atlas is set up, one command per line.
pub fn suggestions() -> Vec<String> {
    let commands = [
        ("atlas screenshot", "see the helper's screen"),
        ("atlas menubar", "watch it, or take control"),
        ("atlas doctor", "check that it is ready"),
        ("atlas --help", "every command"),
    ];
    commands
        .iter()
        .map(|(command, what)| format!("  {}  {what}", style::cmd(format!("{command:<21}"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_for_programs_and_text_for_terminals() {
        assert_eq!(detect(true, None, true), Format::Json);
        assert_eq!(detect(false, None, true), Format::Human);
        assert_eq!(detect(false, None, false), Format::Json);
        assert_eq!(detect(false, Some("1"), true), Format::Json);
        assert_eq!(detect(false, Some("0"), false), Format::Human);
        assert_eq!(detect(true, Some("0"), true), Format::Json, "--json wins");
        assert_eq!(detect(false, Some(""), false), Format::Json);
    }
}
