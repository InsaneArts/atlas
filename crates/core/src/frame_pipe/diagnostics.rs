//! What happened to each picture acquisition, in a fixed vocabulary of
//! numbers and words: no pixels, input text, paths, URLs or credentials.
//! The ring keeps the newest records and the first failure of the latest
//! outage, so a later retry cannot hide what started it.
use crate::error::Code;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, time::Duration};

/// How many records the ring keeps.
pub const RING: usize = 256;
/// The most records one page returns.
pub const PAGE: usize = 15;
/// The most bytes one page's records take as JSON.
pub const PAGE_BYTES: usize = 8 * 1024;

/// A capture source, as asked for or as it delivered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    #[default]
    None,
    Cg,
    Sck,
    CgFallback,
}

impl Backend {
    /// The backend a frame header names, `none` for any other word.
    pub fn named(word: &str) -> Self {
        match word {
            "cg" => Self::Cg,
            "sck" => Self::Sck,
            "cg-fallback" => Self::CgFallback,
            _ => Self::None,
        }
    }
}

/// How an acquisition or delivery ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    #[default]
    Accepted,
    Rejected,
    Cancelled,
    Timeout,
}

/// Where in an acquisition, delivery or cleanup something ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    #[default]
    None,
    Peer,
    Authenticate,
    Subscribe,
    Credit,
    BeforeProof,
    ReadyHeader,
    Allocation,
    BodyRead,
    AfterProof,
    SourceAge,
    ClientAge,
    Reconstruct,
    Decoded,
    Convert,
    Select,
    Commit,
    Input,
    Release,
    Stop,
    Drain,
    Recovery,
    Budget,
    Protocol,
    Focus,
    Shutdown,
}

/// Why something ended, in words that name a cause, never a native string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    #[default]
    None,
    SourceStale,
    ClientStale,
    PictureGap,
    CreditExpired,
    WrongBase,
    WrongGeometry,
    ViewChanged,
    Hidden,
    Paused,
    Cancelled,
    ConversionFailed,
    InvalidSchema,
    Truncated,
    Oversized,
    UnsupportedVersion,
    WrongPeer,
    WrongChallenge,
    ExpiredCapability,
    RightsDenied,
    BackendInUse,
    CaptureLimitConflict,
    HelperBindingConflict,
    ServiceVersionInUse,
    MaintenanceIncomplete,
    BudgetExhausted,
    SlowWriter,
    SourceStartFailed,
    SourceStopFailed,
    DrainUnconfirmed,
    EpochChanged,
    ConsentChanged,
    FocusLost,
    LeaseLost,
    MutationRefused,
    MutationUnknown,
    QueueOverflow,
    StorageContract,
    TransportEnded,
    OtherKnownError,
}

impl Reason {
    /// The reason an error of `code` gives, as far as its code alone says.
    pub fn of(code: Code) -> Self {
        match code {
            Code::StaleFrame => Self::SourceStale,
            Code::SessionChanged | Code::OnConsole | Code::Identity => Self::EpochChanged,
            Code::Permission => Self::ConsentChanged,
            Code::Transport => Self::TransportEnded,
            Code::Cancelled => Self::Cancelled,
            Code::Busy => Self::BackendInUse,
            Code::LeaseLost => Self::LeaseLost,
            _ => Self::OtherKnownError,
        }
    }
}

/// The capture source's lifecycle, as the helper reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lifecycle {
    #[default]
    None,
    Stopped,
    Starting,
    Active,
    Stopping,
    Quarantined,
}

/// The stages a record times, by their index in [`Record::timings_ns`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    BeforeProof,
    ReadyHeader,
    Allocation,
    BodyRead,
    AfterProof,
    Reconstruction,
    Conversion,
    Total,
}

/// One acquisition's outcome and where its time went. Every field is a
/// number or a fixed word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    version: u8,
    #[serde(rename = "acquisitionID")]
    pub acquisition_id: u64,
    pub source_generation: u64,
    pub requested_backend: Backend,
    pub actual_backend: Backend,
    /// Bit `i` says whether timing `i` was taken, failed stages included.
    timing_mask: u32,
    timings_ns: [u64; 8],
    pub age_at_decision_ns: u64,
    pub credit_wait_ns: u64,
    pub broker_age_ns: u64,
    pub client_bound_ns: u64,
    pub source_sequence: u64,
    pub input_revision: u64,
    pub outcome: Outcome,
    pub stage: Stage,
    pub reason: Reason,
    pub pipe_synchronized: bool,
    pub pipe_reused: bool,
    pub last_good_gap_ns: u64,
    pub release_reason: Reason,
    pub allocated_bytes: u64,
    pub reserved_bytes: u64,
    pub lifecycle: Lifecycle,
}

impl Record {
    /// A record of acquisition `acquisition_id`, accepted until told otherwise.
    pub fn new(acquisition_id: u64) -> Self {
        Self {
            version: 1,
            acquisition_id,
            ..Self::default()
        }
    }

    /// Note that `timing` took `elapsed`.
    pub fn time(&mut self, timing: Timing, elapsed: Duration) {
        let index = timing as usize;
        self.timings_ns[index] = nanos(elapsed);
        self.timing_mask |= 1 << index;
    }

    /// How long `timing` took, when it was taken.
    pub fn timing(&self, timing: Timing) -> Option<Duration> {
        let index = timing as usize;
        (self.timing_mask & (1 << index) != 0).then(|| Duration::from_nanos(self.timings_ns[index]))
    }

    /// End the record as `outcome` at `stage` for `reason`.
    pub fn end(&mut self, outcome: Outcome, stage: Stage, reason: Reason) {
        self.outcome = outcome;
        self.stage = stage;
        self.reason = reason;
    }
}

/// `elapsed` in whole nanoseconds, saturating.
pub fn nanos(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
}

/// The newest records.
#[derive(Debug, Default)]
pub struct Ring {
    records: VecDeque<(u64, Record)>,
    next: u64,
}

/// Records after a cursor, and the cursor to ask from next.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub cursor: u64,
    pub next_cursor: u64,
    pub records: Vec<Record>,
}

impl Ring {
    /// Keep `record`, dropping the oldest beyond [`RING`].
    pub fn push(&mut self, record: Record) {
        self.records.push_back((self.next, record));
        self.next += 1;
        while self.records.len() > RING {
            self.records.pop_front();
        }
    }

    /// The newest record.
    pub fn last(&self) -> Option<&Record> {
        self.records.back().map(|(_, record)| record)
    }

    /// Up to `count` (at most [`PAGE`]) records from `cursor` on, fewer
    /// when the page's whole JSON would pass [`PAGE_BYTES`]. Records the
    /// ring dropped are skipped.
    pub fn page(&self, cursor: u64, count: usize) -> Page {
        let mut records = Vec::new();
        // The page around its records, with the widest cursors it can have.
        let mut bytes = serde_json::to_vec(&Page {
            cursor: u64::MAX,
            next_cursor: u64::MAX,
            records: Vec::new(),
        })
        .map_or(PAGE_BYTES, |json| json.len());
        let mut next_cursor = cursor.max(self.records.front().map_or(self.next, |(id, _)| *id));
        for (id, record) in self.records.iter().filter(|(id, _)| *id >= cursor) {
            let size = serde_json::to_vec(record).map_or(usize::MAX, |json| json.len() + 1);
            if records.len() == count.min(PAGE) || bytes + size > PAGE_BYTES {
                break;
            }
            bytes += size;
            records.push(*record);
            next_cursor = id + 1;
        }
        Page {
            cursor,
            next_cursor,
            records,
        }
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
