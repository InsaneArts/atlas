//! Authenticated, encrypted connections between the station and a desktop
//! that shares nothing with it: each end's key, the Noise handshakes that
//! prove them, and the framing of what follows.
//!
//! Nothing here knows what a lane carries: the station's client and the
//! guest's daemon put requests, pictures and stages on them. Generations
//! and the right to act for one are here; what retiring one cancels is the
//! owner of the work's.

pub mod authority;
pub mod clock;
pub mod compatibility;
pub mod control;
pub mod credit;
pub mod generation;
pub mod keys;
pub mod lane;
pub mod pairing;
pub mod pairing_file;
pub mod secure;
#[cfg(any(test, feature = "test-support"))]
pub mod standin;
pub mod station;
