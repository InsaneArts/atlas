//! The one stream between a machine desktop's daemon and its input service:
//! framed, so every connection the daemon proxies, every renewal and
//! revocation of a generation's authority, and every hold travel over it
//! with no descriptor ever passed. Anything it cannot read whole is loss.
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The most bytes one frame carries.
pub const PAYLOAD: usize = 64 * 1024;

/// One connection's number on the channel, chosen by the daemon.
pub type Connection = u32;

/// One generation of the desktop's session, as the daemon numbers them.
pub type Generation = u64;

/// What one frame says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// Start a connection, for a generation.
    Open(Connection, Generation),
    /// Bytes on a connection, either way.
    Data(Connection, Vec<u8>),
    /// End a connection.
    Close(Connection),
    /// A generation's authority holds until a deadline, in nanoseconds on
    /// the continuous clock both processes read.
    Renew(Generation, u64),
    /// A generation's authority is over.
    Revoke(Generation),
    /// Hold agents' input, or lift the hold: the daemon's `seq`-th word on it.
    Hold { seq: u64, on: bool },
    /// No event of a revoked generation can post any more.
    Revoked(Generation),
    /// What a revoked generation held is released, and its work joined.
    Settled(Generation),
    /// The daemon's `seq`-th word on the hold took effect.
    Held { seq: u64, on: bool },
}

/// Each frame's first byte.
mod kind {
    pub(super) const OPEN: u8 = 1;
    pub(super) const DATA: u8 = 2;
    pub(super) const CLOSE: u8 = 3;
    pub(super) const RENEW: u8 = 4;
    pub(super) const REVOKE: u8 = 5;
    pub(super) const HOLD: u8 = 6;
    pub(super) const REVOKED: u8 = 7;
    pub(super) const HELD: u8 = 8;
    pub(super) const SETTLED: u8 = 9;
}

/// Why a frame could not be read: the channel is lost either way.
#[derive(Debug, PartialEq, Eq)]
pub struct Lost;

impl std::fmt::Display for Lost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The input service's channel was lost")
    }
}

impl Frame {
    /// The frame as it travels: kind, connection, generation, length, then
    /// the bytes, all big-endian.
    pub fn encode(&self) -> Vec<u8> {
        let (kind, connection, generation, bytes): (u8, Connection, Generation, Vec<u8>) =
            match self {
                Self::Open(connection, generation) => {
                    (kind::OPEN, *connection, *generation, vec![])
                }
                Self::Data(connection, bytes) => (kind::DATA, *connection, 0, bytes.clone()),
                Self::Close(connection) => (kind::CLOSE, *connection, 0, vec![]),
                Self::Renew(generation, deadline) => {
                    (kind::RENEW, 0, *generation, deadline.to_be_bytes().to_vec())
                }
                Self::Revoke(generation) => (kind::REVOKE, 0, *generation, vec![]),
                Self::Hold { seq, on } => (kind::HOLD, 0, *seq, vec![u8::from(*on)]),
                Self::Revoked(generation) => (kind::REVOKED, 0, *generation, vec![]),
                Self::Settled(generation) => (kind::SETTLED, 0, *generation, vec![]),
                Self::Held { seq, on } => (kind::HELD, 0, *seq, vec![u8::from(*on)]),
            };
        let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let mut encoded = Vec::with_capacity(17 + bytes.len());
        encoded.push(kind);
        encoded.extend_from_slice(&connection.to_be_bytes());
        encoded.extend_from_slice(&generation.to_be_bytes());
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(&bytes);
        encoded
    }

    /// Read the next whole frame from `reader`.
    ///
    /// # Errors
    ///
    /// [`Lost`] when the stream ends, fails, or says what is not a frame.
    pub async fn read(reader: &mut (impl AsyncRead + Unpin)) -> Result<Self, Lost> {
        let mut head = [0; 17];
        reader.read_exact(&mut head).await.map_err(|_| Lost)?;
        let [kind, rest @ ..] = head;
        let connection = Connection::from_be_bytes(rest[..4].try_into().map_err(|_| Lost)?);
        let generation = Generation::from_be_bytes(rest[4..12].try_into().map_err(|_| Lost)?);
        let length = u32::from_be_bytes(rest[12..16].try_into().map_err(|_| Lost)?);
        let length = usize::try_from(length).map_err(|_| Lost)?;
        let expected = match kind {
            kind::DATA => length,
            kind::RENEW => 8,
            kind::HOLD | kind::HELD => 1,
            _ => 0,
        };
        if length > PAYLOAD || length != expected {
            return Err(Lost);
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).await.map_err(|_| Lost)?;
        let flag = || match bytes.as_slice() {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(Lost),
        };
        let deadline = || {
            bytes
                .as_slice()
                .try_into()
                .map(u64::from_be_bytes)
                .map_err(|_| Lost)
        };
        match kind {
            kind::OPEN => Ok(Self::Open(connection, generation)),
            kind::DATA if !bytes.is_empty() => Ok(Self::Data(connection, bytes)),
            kind::CLOSE => Ok(Self::Close(connection)),
            kind::RENEW => deadline().map(|deadline| Self::Renew(generation, deadline)),
            kind::REVOKE => Ok(Self::Revoke(generation)),
            kind::HOLD => flag().map(|on| Self::Hold {
                seq: generation,
                on,
            }),
            kind::REVOKED => Ok(Self::Revoked(generation)),
            kind::HELD => flag().map(|on| Self::Held {
                seq: generation,
                on,
            }),
            kind::SETTLED => Ok(Self::Settled(generation)),
            _ => Err(Lost),
        }
    }

    /// Write the frame whole to `writer`.
    ///
    /// # Errors
    ///
    /// [`Lost`] when it cannot be written, or carries too much.
    pub async fn write(&self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), Lost> {
        if let Self::Data(_, bytes) = self
            && (bytes.is_empty() || bytes.len() > PAYLOAD)
        {
            return Err(Lost);
        }
        writer.write_all(&self.encode()).await.map_err(|_| Lost)?;
        writer.flush().await.map_err(|_| Lost)
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
