//! Metadata as the pipe accepts it: one bounded JSON object, read in a
//! lexical pass before any typed decoding, so duplicate keys, escaped
//! duplicates, invalid escapes and lone surrogates are refused rather than
//! silently merged. Typed decoding then takes the exact keys a kind allows.
use super::Refusal;
use std::collections::HashSet;

/// The deepest nesting of objects and arrays.
pub const DEPTH_MAX: usize = 8;
/// The most keys in the whole object, nested ones included.
pub const KEYS_MAX: usize = 512;
/// The longest key, in UTF-8 bytes once decoded.
pub const KEY_MAX: usize = 128;
/// The longest string value, in UTF-8 bytes once decoded.
pub const STRING_MAX: usize = 1024;

/// Whether `bytes` is one JSON object within the pipe's bounds, with no key
/// twice in any object, however it is escaped, and nothing after it.
///
/// # Errors
///
/// Refuses invalid UTF-8 or JSON, a value that is not an object, a bound
/// passed, a duplicate key, or trailing data.
pub fn guard(bytes: &[u8]) -> Result<(), Refusal> {
    if bytes.len() > super::METADATA_MAX {
        return Err(Refusal::Oversized);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Refusal::Invalid)?;
    let mut lexer = Lexer {
        bytes: text.as_bytes(),
        at: 0,
        keys: 0,
    };
    lexer.space();
    if lexer.peek() != Some(b'{') {
        return Err(Refusal::Invalid);
    }
    lexer.value(0)?;
    lexer.space();
    if lexer.at != lexer.bytes.len() {
        return Err(Refusal::Invalid);
    }
    Ok(())
}

struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
    keys: usize,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Result<u8, Refusal> {
        let byte = self.peek().ok_or(Refusal::Invalid)?;
        self.at += 1;
        Ok(byte)
    }

    fn expect(&mut self, byte: u8) -> Result<(), Refusal> {
        if self.bump()? == byte {
            Ok(())
        } else {
            Err(Refusal::Invalid)
        }
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<(), Refusal> {
        match self.peek().ok_or(Refusal::Invalid)? {
            b'{' => self.object(depth + 1),
            b'[' => self.array(depth + 1),
            b'"' => self.string(STRING_MAX).map(drop),
            b't' => self.literal(b"true"),
            b'f' => self.literal(b"false"),
            b'n' => self.literal(b"null"),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(Refusal::Invalid),
        }
    }

    fn object(&mut self, depth: usize) -> Result<(), Refusal> {
        if depth > DEPTH_MAX {
            return Err(Refusal::Oversized);
        }
        self.expect(b'{')?;
        let mut seen = HashSet::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            let key = self.string(KEY_MAX)?;
            self.keys += 1;
            if self.keys > KEYS_MAX {
                return Err(Refusal::Oversized);
            }
            if !seen.insert(key) {
                return Err(Refusal::Invalid);
            }
            self.space();
            self.expect(b':')?;
            self.space();
            self.value(depth)?;
            self.space();
            match self.bump()? {
                b',' => {}
                b'}' => return Ok(()),
                _ => return Err(Refusal::Invalid),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<(), Refusal> {
        if depth > DEPTH_MAX {
            return Err(Refusal::Oversized);
        }
        self.expect(b'[')?;
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            self.value(depth)?;
            self.space();
            match self.bump()? {
                b',' => {}
                b']' => return Ok(()),
                _ => return Err(Refusal::Invalid),
            }
        }
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), Refusal> {
        let end = self.at + word.len();
        if self.bytes.get(self.at..end) != Some(word) {
            return Err(Refusal::Invalid);
        }
        self.at = end;
        Ok(())
    }

    /// A JSON number: an optional minus, an integer without leading zeros,
    /// then an optional fraction and exponent.
    fn number(&mut self) -> Result<(), Refusal> {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.bump()? {
            b'0' => {}
            b'1'..=b'9' => self.digits(),
            _ => return Err(Refusal::Invalid),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.digit()?;
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digit()?;
            self.digits();
        }
        Ok(())
    }

    fn digit(&mut self) -> Result<(), Refusal> {
        if self.bump()?.is_ascii_digit() {
            Ok(())
        } else {
            Err(Refusal::Invalid)
        }
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
    }

    /// A string, decoded, refused past `limit` decoded bytes.
    fn string(&mut self, limit: usize) -> Result<String, Refusal> {
        self.expect(b'"')?;
        let mut decoded = String::new();
        loop {
            let byte = self.bump()?;
            match byte {
                b'"' => return Ok(decoded),
                b'\\' => {
                    let escaped = match self.bump()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.unicode()?,
                        _ => return Err(Refusal::Invalid),
                    };
                    decoded.push(escaped);
                }
                0..=0x1f => return Err(Refusal::Invalid),
                _ => {
                    // The input is valid UTF-8, so a lead byte starts a whole
                    // character: copy it with its continuation bytes.
                    let start = self.at - 1;
                    let width = match byte {
                        0x00..=0x7f => 1,
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    let end = start + width;
                    let character = self
                        .bytes
                        .get(start..end)
                        .and_then(|bytes| std::str::from_utf8(bytes).ok())
                        .ok_or(Refusal::Invalid)?;
                    decoded.push_str(character);
                    self.at = end;
                }
            }
            if decoded.len() > limit {
                return Err(Refusal::Oversized);
            }
        }
    }

    /// The character after `\u`: four hex digits, or a surrogate pair of two
    /// escapes. A lone surrogate is refused.
    fn unicode(&mut self) -> Result<char, Refusal> {
        let high = self.hex4()?;
        if (0xdc00..=0xdfff).contains(&high) {
            return Err(Refusal::Invalid);
        }
        if !(0xd800..=0xdbff).contains(&high) {
            return char::from_u32(high).ok_or(Refusal::Invalid);
        }
        self.expect(b'\\')?;
        self.expect(b'u')?;
        let low = self.hex4()?;
        if !(0xdc00..=0xdfff).contains(&low) {
            return Err(Refusal::Invalid);
        }
        char::from_u32(0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00)).ok_or(Refusal::Invalid)
    }

    fn hex4(&mut self) -> Result<u32, Refusal> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = char::from(self.bump()?)
                .to_digit(16)
                .ok_or(Refusal::Invalid)?;
            value = value * 16 + digit;
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
