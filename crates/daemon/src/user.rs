//! What only the user desktop's helper does: a hidden user on this Mac,
//! which people know as Second Login, set up from the inside. Its daemon's
//! serving remains in [`crate::daemon`], with its user-desktop assumptions
//! kept until another kind is designed.
pub mod setup;
