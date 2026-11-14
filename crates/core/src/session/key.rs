//! A desktop's or a station's public key, as the other end pins it.
use std::fmt;

/// One end's public key, as the other end pins it once a pairing proved it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerKey([u8; 32]);

impl PeerKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The key in `bytes`, when they are one's length.
    #[must_use]
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(Self)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The key's SHA-256 in hex: what a record or a person names it by.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        crate::digest::bytes_sha256(&self.0)
    }
}

impl fmt::Debug for PeerKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PeerKey({})", &self.fingerprint()[..16])
    }
}

impl serde::Serialize for PeerKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hex: String = self.0.iter().map(|byte| format!("{byte:02x}")).collect();
        serializer.serialize_str(&hex)
    }
}

impl<'de> serde::Deserialize<'de> for PeerKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(deserializer)?;
        Self::from_hex(&hex)
            .ok_or_else(|| serde::de::Error::custom("not a key: 64 lowercase hexadecimal digits"))
    }
}

impl PeerKey {
    /// The key written as 64 lowercase hexadecimal digits.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return None;
        }
        let mut bytes = [0; 32];
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
