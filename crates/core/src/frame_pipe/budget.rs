//! Byte reservations made before anything is allocated: all or nothing, and
//! returned only when their last owner drops. Reserving does not allocate;
//! it bounds what may be.
use std::sync::{Arc, Mutex, PoisonError};

/// The largest canonical source picture: 4096 × 4096 RGB.
pub const SOURCE_MAX: u64 = 4096 * 4096 * 3;
/// The broker's whole pixel reservation.
pub const BROKER_PIXELS: u64 = 384 * 1024 * 1024;
/// The most observers one broker serves.
pub const OBSERVERS_MAX: u64 = 4;

/// A shared byte account with a fixed limit.
#[derive(Clone, Debug)]
pub struct Budget(Arc<Mutex<Account>>);

#[derive(Debug)]
struct Account {
    limit: u64,
    reserved: u64,
}

/// Bytes held against a budget until this drops.
#[derive(Debug)]
#[must_use]
pub struct Reservation {
    budget: Budget,
    bytes: u64,
}

impl Reservation {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut account = self.budget.account();
        account.reserved = account.reserved.saturating_sub(self.bytes);
    }
}

impl Budget {
    pub fn new(limit: u64) -> Self {
        Self(Arc::new(Mutex::new(Account { limit, reserved: 0 })))
    }

    fn account(&self) -> std::sync::MutexGuard<'_, Account> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hold `bytes`, or none when they would pass the limit.
    pub fn reserve(&self, bytes: u64) -> Option<Reservation> {
        let mut account = self.account();
        let reserved = account
            .reserved
            .checked_add(bytes)
            .filter(|reserved| *reserved <= account.limit)?;
        account.reserved = reserved;
        Some(Reservation {
            budget: self.clone(),
            bytes,
        })
    }

    pub fn reserved(&self) -> u64 {
        self.account().reserved
    }
}

/// Whether the broker may serve `observers` observers of a source whose
/// pictures are `source` bytes, with a snapshot encoding when `snapshot`:
/// the latest and in-progress source pictures, each observer's baseline and
/// candidate, and the snapshot's source pin must all fit together.
pub fn admits(source: u64, observers: u64, snapshot: bool) -> bool {
    let copies = 2_u64
        .checked_add(observers.saturating_mul(2))
        .and_then(|copies| copies.checked_add(u64::from(snapshot)));
    source <= SOURCE_MAX
        && observers <= OBSERVERS_MAX
        && copies
            .and_then(|copies| copies.checked_mul(source))
            .is_some_and(|bytes| bytes <= BROKER_PIXELS)
}

#[cfg(test)]
mod tests;
