//! What Atlas knows things by: types that hold only text their own rule
//! accepts, the plain labels hosts and agents give, and the lowercase
//! hexadecimal digits digests and random names are written in.
use rand::RngCore;

/// The most characters an app name or element label keeps.
pub const NAME_LIMIT: usize = 40;

/// A type that holds only text its own `parse` accepts: it is read from and
/// written as that text, and nothing else ever becomes one. `refused(text)`
/// is the error for text that is not one; `display` also shows it as its
/// text. The type's own `impl` gives `parse` and how one is made.
#[macro_export]
macro_rules! name_type {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
        refused($text:ident) = $refused:expr;
        $($display:ident)?
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(try_from = "String", into = "String")]
        $vis struct $name(String);

        impl $name {
            $vis fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;

            fn try_from($text: String) -> std::result::Result<Self, Self::Error> {
                Self::parse(&$text).ok_or_else(|| $refused)
            }
        }

        impl From<$name> for String {
            fn from(name: $name) -> Self {
                name.0
            }
        }

        $($crate::name::name_type!(@$display $name);)?
    };
    (@display $name:ident) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}
pub use name_type;

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

/// At most [`NAME_LIMIT`] characters, cut with an ellipsis.
pub fn bounded(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= NAME_LIMIT {
        return text.to_owned();
    }

    let mut cut = text.chars().take(NAME_LIMIT - 1).collect::<String>();
    cut.push('…');
    cut
}

/// `text` with its first letter upper case, for a line shown on its own.
pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Whether `text` is exactly `digits` lowercase hexadecimal digits, as
/// Atlas writes digests and random names.
pub fn lower_hex(text: &str, digits: usize) -> bool {
    text.len() == digits
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `bytes` as lowercase hexadecimal digits, two a byte.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `digits` random lowercase hexadecimal digits.
pub fn random_hex(digits: usize) -> String {
    let mut bytes = vec![0; digits.div_ceil(2)];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = hex(&bytes);
    text.truncate(digits);
    text
}

/// Whether `value` is a plain label of 1 to `max` bytes: ASCII letters,
/// digits, `.`, `_`, `:` and `-`, so no other value is ever kept or shown.
pub fn plain(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

/// Whether two names people or apps give things name the same thing:
/// compared case-insensitively, so `textedit` and `TextEdit` (or `ÉDITEUR`
/// and `Éditeur`) agree. App names and element names are both matched so.
pub fn same_text(a: &str, b: &str) -> bool {
    a == b
        || a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests;
