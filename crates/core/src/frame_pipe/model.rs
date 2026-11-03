//! The typed metadata each packet kind carries, with exactly its keys:
//! unknown keys are refused, optional keys are absent rather than null, and
//! identifiers keep their fixed shapes. A value read here names facts; it
//! never grants authority on its own.
use super::{
    diagnostics::{Reason, Stage},
    wire::{Refusal, schema::guard},
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};

/// `bytes` as `T`, after the lexical guard.
///
/// # Errors
///
/// Refuses metadata the guard refuses, or that is not exactly a `T`.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Refusal> {
    guard(bytes)?;
    serde_json::from_slice(bytes).map_err(|_| Refusal::Invalid)
}

/// `value` as compact metadata.
///
/// # Errors
///
/// Refuses metadata past its bound.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Refusal> {
    let bytes = serde_json::to_vec(value).map_err(|_| Refusal::Invalid)?;
    if bytes.len() > super::wire::METADATA_MAX {
        return Err(Refusal::Oversized);
    }
    Ok(bytes)
}

/// An optional key: absent is none, and null is refused.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// Lowercase hex identifiers of a fixed length: 32 digits (128 bits) or 64
/// (256 bits).
macro_rules! hex {
    ($($name:ident = $digits:literal;)+) => {$(
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
        pub struct $name(String);

        impl $name {
            /// The identifier `text` spells, if it has exactly the shape.
            pub fn parse(text: &str) -> Option<Self> {
                (text.len() == $digits
                    && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
                .then(|| Self(text.to_owned()))
            }

            /// A new random identifier.
            pub fn random() -> Self {
                Self(crate::name::random_hex($digits))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::parse(&text).ok_or_else(|| serde::de::Error::custom("bad identifier"))
            }
        }
    )+};
}

hex! {
    Hex32 = 32;
    Hex64 = 64;
}

/// A whole number above zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Positive(u64);

impl Positive {
    pub fn new(value: u64) -> Option<Self> {
        (value > 0).then_some(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Positive {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("must be above zero"))
    }
}

/// A finite measure above zero.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Measure(f64);

impl Measure {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Measure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(f64::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("must be finite and above zero"))
    }
}

/// A picture side in pixels: 1 to 4096.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Side(u32);

impl Side {
    pub fn new(value: u32) -> Option<Self> {
        (1..=4096).contains(&value).then_some(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Side {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("must be 1 to 4096"))
    }
}

/// What a client may be granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Right {
    Observe,
    Control,
    Revoke,
    StopAgents,
    Diagnostics,
}

/// How a picture may be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateKind {
    Full,
    Delta,
    Unchanged,
}

/// Which capture source a client asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Cg,
    Sck,
}

/// One committed picture exactly, with every generation it was made under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ref {
    pub service_instance: Hex32,
    #[serde(rename = "observerID")]
    pub observer_id: Hex32,
    pub observer_generation: Positive,
    pub stream_generation: Positive,
    pub geometry_generation: Positive,
    pub source_sequence: Positive,
    pub image_sequence: Positive,
    pub frame_key: Hex64,
    pub view_generation: Positive,
}

/// The detail and source a client wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Demand {
    pub desired_longest: Longest,
    pub backend: Source,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub hard_ceiling: Option<Longest>,
}

/// A requested longest side: 320 to 4096 pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Longest(u32);

impl Longest {
    pub fn new(value: u32) -> Option<Self> {
        (320..=4096).contains(&value).then_some(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Longest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?)
            .ok_or_else(|| serde::de::Error::custom("must be 320 to 4096"))
    }
}

/// The picture geometry a subscription accepted.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub stream_generation: Positive,
    pub geometry_generation: Positive,
    pub width: Side,
    pub height: Side,
    pub row_stride: u32,
    pub display_width: Measure,
    pub display_height: Measure,
    pub display_scale: Measure,
    pub backend: Source,
}

impl Profile {
    /// Whether the row stride is exactly three bytes a pixel.
    pub fn valid(&self) -> bool {
        u64::from(self.row_stride) == u64::from(self.width.get()) * 3
    }

    /// The bytes of one full picture.
    pub fn bytes(&self) -> u64 {
        u64::from(self.row_stride) * u64::from(self.height.get())
    }
}

/// A failure's fixed words: no native text or path enters it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub code: WireCode,
    pub stage: Stage,
    pub uncertain: bool,
    pub dispatched: bool,
    pub reason: Reason,
}

/// A failure's code on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireCode {
    Busy,
    Cancelled,
    Capture,
    Cleanup,
    Identity,
    Input,
    LeaseLost,
    OnConsole,
    Permission,
    Protocol,
    SessionChanged,
    StaleFrame,
    State,
    Stopped,
    Transport,
    Uncertain,
    Usage,
}

/// What an error ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    RequestRefused,
    CreditRetired,
    PairFatal,
}

/// The client's opening on its control lane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hello {
    pub protocol_major: u16,
    pub service_instance: Hex32,
    pub bootstrap_capability: Hex64,
    pub client_challenge: Hex32,
    pub role: Role,
    pub client_kind: ClientKind,
    pub requested_rights: Vec<Right>,
    pub offered_updates: Vec<UpdateKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Observer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClientKind {
    Terminal,
    Swift,
}

/// The broker's answer to [`Hello`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Welcome {
    pub service_instance: Hex32,
    #[serde(rename = "observerID")]
    pub observer_id: Hex32,
    pub observer_generation: Positive,
    #[serde(rename = "controlConnectionID")]
    pub control_connection_id: Hex32,
    pub client_challenge: Hex32,
    pub granted_rights: Vec<Right>,
    pub accepted_updates: Vec<UpdateKind>,
    pub pair_token: Hex64,
    pub pair_expires_in_ms: u32,
    pub cleanup_token: Hex64,
}

/// The frame lane's opening, and the broker's answer without the token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attach {
    pub service_instance: Hex32,
    #[serde(rename = "observerID")]
    pub observer_id: Hex32,
    pub observer_generation: Positive,
    #[serde(rename = "controlConnectionID")]
    pub control_connection_id: Hex32,
    pub frame_challenge: Hex32,
    pub pair_token: Hex64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attached {
    pub service_instance: Hex32,
    #[serde(rename = "observerID")]
    pub observer_id: Hex32,
    pub observer_generation: Positive,
    #[serde(rename = "controlConnectionID")]
    pub control_connection_id: Hex32,
    pub frame_challenge: Hex32,
    #[serde(rename = "frameConnectionID")]
    pub frame_connection_id: Hex32,
}

/// The pair's last two steps, each naming both lanes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pair {
    pub service_instance: Hex32,
    #[serde(rename = "observerID")]
    pub observer_id: Hex32,
    pub observer_generation: Positive,
    #[serde(rename = "controlConnectionID")]
    pub control_connection_id: Hex32,
    #[serde(rename = "frameConnectionID")]
    pub frame_connection_id: Hex32,
}

/// An error on either lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorMetadata {
    pub failure: Failure,
    pub disposition: Disposition,
    #[serde(
        rename = "creditID",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub credit_id: Option<Positive>,
}

impl ErrorMetadata {
    /// Whether the error names a credit exactly when it retires one.
    pub fn valid(&self) -> bool {
        (self.disposition == Disposition::CreditRetired) == self.credit_id.is_some()
    }
}

#[cfg(test)]
mod tests;
