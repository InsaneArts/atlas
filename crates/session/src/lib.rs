//! Authenticated, encrypted connections between the station and a desktop
//! that shares nothing with it: each end's key, the Noise handshakes that
//! prove them, and the framing of what follows.
//!
//! Nothing here knows what a connection carries; lanes, heartbeats and
//! pairing records build on it.

pub mod keys;
pub mod secure;
