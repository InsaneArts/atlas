//! One picture's pixels with the reservation that pays for them, reserved
//! before they are allocated. Callers fill it while they alone own it, then
//! share it whole and read it as a slice; its buffer never leaves it, so no
//! copy escapes the account.
use super::{
    budget::{Budget, Reservation},
    diagnostics::Reason,
};
use crate::buffer::Buffer;

/// Packed RGB pixels, three bytes each, top to bottom and left to right.
#[derive(Debug)]
pub struct FramePixels {
    buffer: Buffer,
    width: u32,
    height: u32,
    _reservation: Reservation,
}

impl FramePixels {
    /// A zeroed `width` × `height` picture, its bytes reserved from `budget`
    /// before any is allocated.
    ///
    /// # Errors
    ///
    /// `WrongGeometry` for a side outside 1–4096, and `BudgetExhausted`
    /// when the budget or the system cannot hold it.
    pub fn reserve(budget: &Budget, width: u32, height: u32) -> Result<Self, Reason> {
        let side = 1..=4096;
        if !side.contains(&width) || !side.contains(&height) {
            return Err(Reason::WrongGeometry);
        }
        // Both sides are at most 4096, so this cannot overflow.
        let bytes = u64::from(width) * u64::from(height) * 3;
        let reservation = budget.reserve(bytes).ok_or(Reason::BudgetExhausted)?;
        let buffer = usize::try_from(bytes)
            .ok()
            .and_then(|len| Buffer::zeroed(len).ok())
            .ok_or(Reason::BudgetExhausted)?;
        Ok(Self {
            buffer,
            width,
            height,
            _reservation: reservation,
        })
    }

    /// The pixels to fill, while this is the only owner.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

#[cfg(test)]
mod tests;
