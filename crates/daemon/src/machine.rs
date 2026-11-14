//! What a machine desktop's daemon keeps of its session in the guest. Every
//! record trust depends on is in the guest account's Keychain, where only
//! Atlas's own code reads it: an agent runs as the same account, so any
//! file of the account's could be rewritten under it.
pub mod commitment;
pub mod generations;
pub mod key;
pub mod pairing;
pub mod vault;
