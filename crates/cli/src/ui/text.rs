//! Text that fits a terminal: names from the helper made safe to print,
//! values cut to a width, paths with `~`, sizes and counts in plain words.
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) use atlas_core::config::tilde;
pub(crate) use atlas_core::name::count;
pub(crate) use atlas_core::name::listed;

/// `text` with every character that could mislead the terminal written as
/// its escape: control characters, bidirectional controls and line or
/// paragraph separators. Window titles and element names come from apps in
/// the helper and must not colour the terminal or move its cursor.
pub(crate) fn printable(text: &str) -> Cow<'_, str> {
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
pub(crate) fn clip(text: &str, width: usize) -> String {
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

/// `1 copy`, `3 copies`: a count of a noun that takes no plain `s`.
pub(crate) fn amount(n: u64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Width × height, without fractional zeros: `2560 × 1440`.
pub(crate) fn dimensions(width: f64, height: f64) -> String {
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
pub(crate) fn duration(ms: f64) -> String {
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
mod tests;
