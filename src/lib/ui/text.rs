//! Text that fits a terminal: names from the helper made safe to print,
//! values cut to a width, paths with `~`, sizes and counts in plain words.
use std::{borrow::Cow, path::Path};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// `text` with every character that could mislead the terminal written as
/// its escape: control characters, bidirectional controls and line or
/// paragraph separators. Window titles and element names come from apps in
/// the helper and must not colour the terminal or move its cursor.
pub fn printable(text: &str) -> Cow<'_, str> {
    if !text.chars().any(misleading) {
        return Cow::Borrowed(text);
    }

    let mut escaped = String::with_capacity(text.len() + 8);

    for character in text.chars() {
        if misleading(character) {
            escaped.extend(character.escape_debug());
        } else {
            escaped.push(character);
        }
    }

    Cow::Owned(escaped)
}

fn misleading(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{061c}'
                | '\u{2028}'
                | '\u{2029}'
        )
}

/// `text` cut to `width` terminal cells, ending in `…` where it was cut.
pub fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }

    let (mut out, mut used) = (String::new(), 0);

    for grapheme in text.graphemes(true) {
        let cells = grapheme.width();
        if used + cells + 1 > width {
            break;
        }
        out.push_str(grapheme);
        used += cells;
    }

    format!("{}…", out.trim_end())
}

/// A path with the home directory written as `~`.
pub fn tilde(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match path.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// The first non-empty line of a program's output, at most `limit`
/// characters, for a report; None when it printed nothing.
pub fn first_line(output: &[u8], limit: usize) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.chars().take(limit).collect())
}

/// `1 window`, `3 windows`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// `a`, `a and b`, `a, b and c`.
pub fn listed(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Width × height, without fractional zeros: `2560 × 1440`.
pub fn dimensions(width: f64, height: f64) -> String {
    format!("{} × {}", number(width), number(height))
}

/// A number without a trailing `.0`.
pub fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value:.1}")
    }
}

/// Milliseconds as `12 ms`, `1.2 s` or `2m 05s`.
pub fn duration(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{} ms", ms.round() as u64)
    } else if ms < 59_950.0 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        let seconds = (ms / 1000.0).round() as u64;
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_names_cannot_drive_the_terminal() {
        assert_eq!(printable("Untitled"), "Untitled");
        assert_eq!(printable("a\u{1b}[31mb"), "a\\u{1b}[31mb");
        assert_eq!(printable("x\u{202e}y\nz"), "x\\u{202e}y\\nz");
    }

    #[test]
    fn values_are_cut_to_fit_with_an_ellipsis() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("a long window title", 8), "a long…");
        assert!(clip("日本語のタイトル", 7).width() <= 7);
        assert_eq!(clip("abc", 0), "…");
        assert_eq!(clip("abc", 1), "…");
    }

    #[test]
    fn the_first_line_of_output_is_bounded() {
        assert_eq!(
            first_line(b"\n\nerror: x\nmore", 300).as_deref(),
            Some("error: x")
        );
        assert_eq!(
            first_line(&[b'x'; 500], 200).map(|line| line.len()),
            Some(200)
        );
        assert_eq!(first_line(b"  \n", 10), None);
    }

    #[test]
    fn plain_words_for_numbers() {
        assert_eq!(count(1, "window"), "1 window");
        assert_eq!(count(2, "window"), "2 windows");
        assert_eq!(dimensions(2560.0, 1440.0), "2560 × 1440");
        assert_eq!(number(1.5), "1.5");
        assert_eq!(duration(12.3), "12 ms");
        assert_eq!(duration(1234.0), "1.2 s");
        assert_eq!(duration(125_000.0), "2m 05s");
        assert_eq!(duration(59_960.0), "1m 00s", "never 60.0 s");
    }
}
