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
