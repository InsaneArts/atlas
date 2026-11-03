//! What an observer and the broker say about its pictures once paired:
//! subscribing, credits, acknowledgements, closing and state changes.
use super::{DropReason, Failure, Metadata, Positive, Profile, Ref, ResyncReason, present};
use serde::{Deserialize, Serialize};

/// Why a client subscribes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubscribeReason {
    First,
    Resume,
    Reconfigure,
}

/// A client's request for pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Subscribe {
    pub reason: SubscribeReason,
    pub demand: super::Demand,
    pub view_generation: Positive,
}

/// The broker's answer: the geometry pictures will have, accepted before
/// any credit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Subscribed {
    pub profile: Profile,
    pub barrier_generation: Positive,
}

/// What a credit waits for before it is served.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Barrier {
    Ordinary,
    Subscribe,
    Resume,
    Reconfigure,
    AfterInput,
}

/// Room for one picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Credit {
    #[serde(rename = "creditID")]
    pub credit_id: Positive,
    pub view_generation: Positive,
    pub stream_generation: Positive,
    pub geometry_generation: Positive,
    pub barrier: Barrier,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub barrier_generation: Option<Positive>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_revision: Option<u64>,
}

impl Metadata for Credit {
    /// Whether an ordinary credit names no barrier generation and every
    /// other names one, and only an after-input credit names a revision.
    fn valid(&self) -> bool {
        (self.barrier == Barrier::Ordinary) == self.barrier_generation.is_none()
            && (self.barrier == Barrier::AfterInput) == self.input_revision.is_some()
    }
}

/// One delivery named exactly: Decoded, DecodedAck, Commit and CommitAck.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settle {
    #[serde(rename = "creditID")]
    pub credit_id: Positive,
    #[serde(rename = "ref")]
    pub reference: Ref,
}

/// A delivery set aside, keeping the base: Drop and DropAck.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dropped {
    #[serde(rename = "creditID")]
    pub credit_id: Positive,
    pub reason: DropReason,
    #[serde(
        rename = "ref",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reference: Option<Ref>,
}

/// A delivery set aside, forgetting the base: Resync and ResyncAck.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resynced {
    #[serde(rename = "creditID")]
    pub credit_id: Positive,
    pub reason: ResyncReason,
}

/// Why an observer ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseReason {
    Quit,
    WindowClosed,
    TransportEnded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Close {
    pub reason: CloseReason,
}

/// The broker's answer to [`Close`], once cleanup is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Closed {
    pub cleanup_confirmed: bool,
}

impl Metadata for Closed {
    /// Unknown cleanup is an error, never a closed observer.
    fn valid(&self) -> bool {
        self.cleanup_confirmed
    }
}

/// What a state change is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Starting,
    Live,
    Delayed,
    Idle,
    Draining,
    Quarantined,
    Reconnecting,
    Unavailable,
    SecurityInvalidated,
    ProfileChanged,
    ControlChanged,
}

/// Who controls the helper, as a hint for drawing; never authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlOwner {
    None,
    #[serde(rename = "self")]
    Own,
    Other,
}

/// A change the broker reports unasked.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct State {
    pub phase: Phase,
    pub stream_generation: u64,
    pub geometry_generation: u64,
    pub input_ready: bool,
    pub control_owner: ControlOwner,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub profile: Option<Profile>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub failure: Option<Failure>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub managed_bytes: Option<u64>,
}

impl Metadata for State {
    /// Whether a profile comes only with a profile change.
    fn valid(&self) -> bool {
        (self.phase == Phase::ProfileChanged) == self.profile.is_some()
            && self.profile.is_none_or(|profile| profile.valid())
    }
}

impl Metadata for Subscribe {}
impl Metadata for Subscribed {
    fn valid(&self) -> bool {
        self.profile.valid()
    }
}
impl Metadata for Settle {}
impl Metadata for Dropped {}
impl Metadata for Resynced {}
impl Metadata for Close {}

#[cfg(test)]
mod tests;
