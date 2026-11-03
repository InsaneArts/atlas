//! The frame pipe's envelope: a fixed 32-byte prefix, one JSON metadata
//! object, then an optional binary payload. A stream socket may split or
//! join packets anywhere, so the decoder keeps its place across reads and
//! checks every length before it reserves anything. Every integer is
//! unsigned big-endian.
//!
//! | Offset | Bytes | Field |
//! | --- | --- | --- |
//! | 0 | 4 | `ATFP` |
//! | 4 | 2 | protocol major, 1 |
//! | 6 | 2 | kind |
//! | 8 | 4 | flags, 0 |
//! | 12 | 4 | metadata length |
//! | 16 | 8 | payload length |
//! | 24 | 8 | request ID |
use super::diagnostics::Reason;

pub mod schema;

/// The prefix's length.
pub const PREFIX: usize = 32;
/// The protocol major version this codec speaks.
pub const MAJOR: u16 = 1;
/// The largest metadata object.
pub const METADATA_MAX: usize = 8 * 1024;
/// The largest packet without a payload, prefix included.
pub const CONTROL_MAX: usize = 16 * 1024;
/// The largest payload: one 4096×4096 RGB picture.
pub const PAYLOAD_MAX: u64 = 4096 * 4096 * 3;
/// The largest payload piece the decoder hands out at once.
pub const CHUNK: usize = 64 * 1024;

const MAGIC: &[u8; 4] = b"ATFP";

/// Which connection a packet may travel on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Requests, acknowledgements and control between client and broker.
    Control,
    /// Pictures, after the pair's attach handshake.
    Frame,
    /// App hosts and cleanup of old observers; never pictures or input.
    Management,
}

/// Each kind's code, word and lanes are one row, so a declared kind cannot
/// go unrouted.
macro_rules! kinds {
    ($($kind:ident = $code:literal, $($lane:ident)|+;)+) => {
        /// Every packet kind, by its code on the wire.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Kind { $($kind),+ }

        impl Kind {
            /// The kind with `code`, if there is one.
            pub fn of(code: u16) -> Option<Self> {
                match code { $($code => Some(Self::$kind),)+ _ => None }
            }

            pub fn code(self) -> u16 {
                match self { $(Self::$kind => $code),+ }
            }

            /// Whether the kind may travel on `lane`.
            pub fn travels(self, lane: Lane) -> bool {
                match self { $(Self::$kind => matches!(lane, $(Lane::$lane)|+)),+ }
            }
        }
    };
}

kinds! {
    Hello = 1, Control;
    Welcome = 2, Control;
    Attach = 3, Frame;
    Attached = 4, Frame;
    PairConfirm = 5, Control;
    PairReady = 6, Control;
    Subscribe = 7, Control;
    Subscribed = 8, Control;
    Credit = 9, Control;
    CreditAccepted = 10, Control;
    FullFrame = 11, Frame;
    DeltaFrame = 12, Frame;
    UnchangedFrame = 13, Frame;
    Decoded = 14, Control;
    DecodedAck = 15, Control;
    Commit = 16, Control;
    CommitAck = 17, Control;
    Drop = 18, Control;
    DropAck = 19, Control;
    Resync = 20, Control;
    ResyncAck = 21, Control;
    Acquire = 22, Control;
    Acquired = 23, Control;
    Renew = 24, Control;
    Renewed = 25, Control;
    Input = 26, Control;
    InputAck = 27, Control;
    Release = 28, Control;
    Released = 29, Control;
    Stop = 30, Control;
    StopResult = 31, Control;
    Suspend = 32, Control;
    Suspended = 33, Control;
    Close = 34, Control;
    Closed = 35, Control;
    CreditExpired = 36, Control;
    State = 37, Control;
    Error = 38, Control | Frame | Management;
    RequestDiagnostics = 39, Control;
    DiagnosticsReply = 40, Control;
    RecoveryHello = 41, Management;
    RecoveryReady = 42, Management;
    Cleanup = 43, Management;
    CleanupResult = 44, Management;
    RegisterAppHost = 45, Management;
    AppHostReady = 46, Management;
    AppCommand = 47, Management;
    AppCommandAck = 48, Management;
    AppHostClosing = 49, Management;
    AppHostClosed = 50, Management;
}

impl Kind {
    /// Whether the kind carries a binary payload: only changed pictures do.
    pub fn has_payload(self) -> bool {
        matches!(self, Self::FullFrame | Self::DeltaFrame)
    }

    /// Whether the kind may have request ID zero: an unsolicited state
    /// change, or an error that answers no request.
    fn unsolicited(self) -> bool {
        matches!(self, Self::State | Self::Error)
    }
}

/// A decoded prefix: what follows it and how long it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Head {
    pub kind: Kind,
    pub metadata: u32,
    pub payload: u64,
    pub request: u64,
}

/// Why bytes were refused. A refused connection is closed: the codec never
/// resumes inside a packet or skips to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not this protocol: wrong magic, flags, kind, lane or request ID.
    Invalid,
    /// Another protocol major version.
    Version,
    /// A length beyond the protocol's bounds.
    Oversized,
    /// The connection ended inside a packet.
    Truncated,
}

impl Refusal {
    /// The diagnostic reason for it.
    pub fn reason(self) -> Reason {
        match self {
            Self::Invalid => Reason::InvalidSchema,
            Self::Version => Reason::UnsupportedVersion,
            Self::Oversized => Reason::Oversized,
            Self::Truncated => Reason::Truncated,
        }
    }
}

/// The prefix and metadata of one packet whose payload is `payload` bytes,
/// checked against the protocol's bounds before anything is written.
///
/// # Errors
///
/// Refuses a kind with a payload it may not carry, metadata or a payload
/// past their bounds, or request ID zero for a kind that answers requests.
pub fn encode(kind: Kind, request: u64, metadata: &[u8], payload: u64) -> Result<Vec<u8>, Refusal> {
    let head = Head {
        kind,
        metadata: u32::try_from(metadata.len()).map_err(|_| Refusal::Oversized)?,
        payload,
        request,
    };
    check(&head)?;
    let mut bytes = Vec::with_capacity(PREFIX + metadata.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&MAJOR.to_be_bytes());
    bytes.extend_from_slice(&kind.code().to_be_bytes());
    bytes.extend_from_slice(&0_u32.to_be_bytes());
    bytes.extend_from_slice(&head.metadata.to_be_bytes());
    bytes.extend_from_slice(&payload.to_be_bytes());
    bytes.extend_from_slice(&request.to_be_bytes());
    bytes.extend_from_slice(metadata);
    Ok(bytes)
}

/// The bounds every head must keep, whichever side made it.
fn check(head: &Head) -> Result<(), Refusal> {
    let metadata = head.metadata as usize;
    // The shortest object, `{}`, has two bytes.
    if metadata < 2 {
        return Err(Refusal::Invalid);
    }
    if metadata > METADATA_MAX {
        return Err(Refusal::Oversized);
    }
    if head.kind.has_payload() {
        if head.payload == 0 {
            return Err(Refusal::Invalid);
        }
        if head.payload > PAYLOAD_MAX {
            return Err(Refusal::Oversized);
        }
    } else {
        if head.payload != 0 {
            return Err(Refusal::Invalid);
        }
        if PREFIX + metadata > CONTROL_MAX {
            return Err(Refusal::Oversized);
        }
    }
    if head.request == 0 && !head.kind.unsolicited() {
        return Err(Refusal::Invalid);
    }
    // A state change is never an answer.
    if head.kind == Kind::State && head.request != 0 {
        return Err(Refusal::Invalid);
    }
    Ok(())
}

fn prefix(bytes: &[u8; PREFIX], lane: Lane) -> Result<Head, Refusal> {
    let u16_at = |at: usize| u16::from_be_bytes([bytes[at], bytes[at + 1]]);
    let u32_at =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let u64_at = |at: usize| {
        let mut word = [0; 8];
        word.copy_from_slice(&bytes[at..at + 8]);
        u64::from_be_bytes(word)
    };
    if &bytes[..4] != MAGIC {
        return Err(Refusal::Invalid);
    }
    if u16_at(4) != MAJOR {
        return Err(Refusal::Version);
    }
    let kind = Kind::of(u16_at(6))
        .filter(|kind| kind.travels(lane))
        .ok_or(Refusal::Invalid)?;
    if u32_at(8) != 0 {
        return Err(Refusal::Invalid);
    }
    let head = Head {
        kind,
        metadata: u32_at(12),
        payload: u64_at(16),
        request: u64_at(24),
    };
    check(&head)?;
    Ok(head)
}

/// What the decoder has read.
#[derive(PartialEq, Eq)]
pub enum Item {
    /// A packet's head and its whole metadata; its payload, if any, follows
    /// in pieces.
    Packet(Head, Vec<u8>),
    /// The next piece of the current packet's payload, at most [`CHUNK`]
    /// bytes.
    Payload(Vec<u8>),
}

enum Place {
    Prefix { bytes: [u8; PREFIX], filled: usize },
    Metadata { head: Head, bytes: Vec<u8> },
    Payload { left: u64 },
}

/// Metadata may carry credentials, so only lengths are shown.
impl std::fmt::Debug for Item {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Packet(head, metadata) => write!(
                formatter,
                "Packet({head:?}, {} metadata bytes)",
                metadata.len()
            ),
            Self::Payload(piece) => write!(formatter, "Payload({} bytes)", piece.len()),
        }
    }
}

/// One lane's incremental decoder.
pub struct Decoder {
    lane: Lane,
    place: Place,
}

impl Decoder {
    pub fn new(lane: Lane) -> Self {
        Self {
            lane,
            place: Place::Prefix {
                bytes: [0; PREFIX],
                filled: 0,
            },
        }
    }

    /// The next item from the front of `input`, consuming what it uses; none
    /// when `input` ran out before one was complete. A refusal ends the lane.
    ///
    /// # Errors
    ///
    /// Refuses a prefix that is not this protocol or names lengths past its
    /// bounds, before any metadata is reserved.
    pub fn next(&mut self, input: &mut &[u8]) -> Result<Option<Item>, Refusal> {
        loop {
            match &mut self.place {
                Place::Prefix { bytes, filled } => {
                    let take = (PREFIX - *filled).min(input.len());
                    bytes[*filled..*filled + take].copy_from_slice(&input[..take]);
                    *filled += take;
                    *input = &input[take..];
                    if *filled < PREFIX {
                        return Ok(None);
                    }
                    let head = prefix(bytes, self.lane)?;
                    self.place = Place::Metadata {
                        head,
                        bytes: Vec::with_capacity(head.metadata as usize),
                    };
                }
                Place::Metadata { head, bytes } => {
                    let take = (head.metadata as usize - bytes.len()).min(input.len());
                    bytes.extend_from_slice(&input[..take]);
                    *input = &input[take..];
                    if bytes.len() < head.metadata as usize {
                        return Ok(None);
                    }
                    let (head, metadata) = (*head, std::mem::take(bytes));
                    self.place = if head.payload > 0 {
                        Place::Payload { left: head.payload }
                    } else {
                        Self::start()
                    };
                    return Ok(Some(Item::Packet(head, metadata)));
                }
                Place::Payload { left } => {
                    if input.is_empty() {
                        return Ok(None);
                    }
                    let take = usize::try_from(*left)
                        .unwrap_or(usize::MAX)
                        .min(CHUNK)
                        .min(input.len());
                    let piece = input[..take].to_vec();
                    *input = &input[take..];
                    *left -= take as u64;
                    if *left == 0 {
                        self.place = Self::start();
                    }
                    return Ok(Some(Item::Payload(piece)));
                }
            }
        }
    }

    fn start() -> Place {
        Place::Prefix {
            bytes: [0; PREFIX],
            filled: 0,
        }
    }

    /// Whether the lane may end here: between packets, not inside one.
    ///
    /// # Errors
    ///
    /// Refuses an end inside a packet as truncated.
    pub fn end(&self) -> Result<(), Refusal> {
        match self.place {
            Place::Prefix { filled: 0, .. } => Ok(()),
            _ => Err(Refusal::Truncated),
        }
    }
}

#[cfg(test)]
mod tests;
