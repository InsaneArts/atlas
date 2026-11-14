//! What a pairing fixed between the station and a machine desktop: the
//! facts every later proof of that desktop is held to.
use super::key::PeerKey;
use crate::files::Token;
use serde::{Deserialize, Serialize};

/// The facts a completed pairing fixed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paired {
    /// The station's key, by its fingerprint.
    pub station: String,
    /// The guest's key.
    pub guest: PeerKey,
    /// The guest account Atlas runs in.
    pub uid: u32,
    /// The guest's boot when it paired, as it reported it.
    pub boot: String,
    /// The enrolment's change.
    pub change: Token,
}
