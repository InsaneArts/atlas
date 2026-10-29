//! What Atlas knows things by: types that hold only text their own rule
//! accepts, the plain labels hosts and agents give, and the lowercase
//! hexadecimal digits digests and random names are written in.
use rand::RngCore;

/// A type that holds only text its own `parse` accepts: it is read from and
/// written as that text, and nothing else ever becomes one. `refused(text)`
/// is the error for text that is not one; `display` also shows it as its
/// text. The type's own `impl` gives `parse` and how one is made.
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
            pub fn as_str(&self) -> &str {
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
pub(crate) use name_type;

/// Whether `text` is exactly `digits` lowercase hexadecimal digits, as
/// Atlas writes digests and random names.
pub(crate) fn lower_hex(text: &str, digits: usize) -> bool {
    text.len() == digits
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `bytes` as lowercase hexadecimal digits, two a byte.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `digits` random lowercase hexadecimal digits.
pub(crate) fn random_hex(digits: usize) -> String {
    let mut bytes = vec![0; digits.div_ceil(2)];
    rand::rng().fill_bytes(&mut bytes);
    let mut text = hex(&bytes);
    text.truncate(digits);
    text
}

/// Whether `value` is a plain label of 1 to `max` bytes: ASCII letters,
/// digits, `.`, `_`, `:` and `-`, so no other value is ever kept or shown.
pub(crate) fn plain(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::{hex, lower_hex, plain, random_hex};

    #[test]
    fn hex_is_lowercase_two_digits_a_byte_and_read_back_exactly() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
        assert!(lower_hex("000fa0ff", 8));
        assert!(!lower_hex("000FA0FF", 8), "uppercase is not Atlas's");
        assert!(!lower_hex("000fa0f", 8), "every digit is there");
        for digits in [1, 6, 12, 48] {
            assert!(lower_hex(&random_hex(digits), digits), "{digits}");
        }
        assert_ne!(random_hex(48), random_hex(48));
    }

    #[test]
    fn a_plain_label_is_short_and_of_safe_characters() {
        assert!(plain("d9031471-5e2a:task_1.b", 64));
        assert!(!plain("", 64));
        assert!(!plain(&"a".repeat(65), 64));
        for refused in ["a b", "a/b", "é", "a\nb"] {
            assert!(!plain(refused, 64), "{refused:?}");
        }
    }
}
