//! The guest's generation count: each control connection's number, never
//! given twice. It starts when the guest commits to a station and only goes
//! up; a count that cannot be read refuses, never starts again.
use super::vault::{Vault, damaged};
use atlas_core::error::{Code, Error, Result};
use atlas_session::generation::{Generation, Generations};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};

const NAME: &str = "guest generation";

/// One generation given at a time in this process, whatever adapter gives it.
static GIVING: Mutex<()> = Mutex::new(());
const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Count {
    version: u32,
    last: u64,
}

/// Start the count at nothing given yet, unless one is kept already.
///
/// # Errors
///
/// Fails when it cannot be read or kept.
pub(super) fn begin(vault: &impl Vault) -> Result<()> {
    if read(vault)?.is_some() {
        return Ok(());
    }
    write(vault, 0)
}

fn read(vault: &impl Vault) -> Result<Option<u64>> {
    let Some(bytes) = vault.read(NAME)? else {
        return Ok(None);
    };
    let count: Count = serde_json::from_slice(&bytes).map_err(|_| damaged("generation count"))?;
    if count.version != VERSION {
        return Err(damaged("generation count"));
    }
    Ok(Some(count.last))
}

fn write(vault: &impl Vault, last: u64) -> Result<()> {
    let count = Count {
        version: VERSION,
        last,
    };
    vault.write(NAME, &serde_json::to_vec(&count)?)
}

/// The guest's generation count in `vault`.
#[derive(Debug)]
pub struct Kept<V> {
    vault: V,
}

impl<V: Vault> Kept<V> {
    pub const fn new(vault: V) -> Self {
        Self { vault }
    }
}

impl<V: Vault> Generations for Kept<V> {
    fn next(&self) -> Result<Generation> {
        let _one = GIVING.lock().unwrap_or_else(PoisonError::into_inner);
        let last = read(&self.vault)?.ok_or_else(|| {
            Error::new(
                Code::Identity,
                "The guest keeps no generation count: it is not paired.",
            )
        })?;
        let next = last
            .checked_add(1)
            .ok_or_else(|| damaged("generation count"))?;
        write(&self.vault, next)?;
        Ok(Generation(next))
    }
}

#[cfg(test)]
#[path = "generations_tests.rs"]
mod tests;
