//! One picture's pixels with the reservation that pays for them. Callers
//! share it whole and read it as a slice; its buffer never leaves it, so no
//! copy escapes the account.
use super::{budget::Reservation, wire::Refusal};
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
    /// `buffer` as a `width` × `height` picture paid for by `reservation`.
    ///
    /// # Errors
    ///
    /// Refuses a buffer that is not exactly that picture, or a reservation
    /// smaller than it.
    pub fn new(
        buffer: Buffer,
        width: u32,
        height: u32,
        reservation: Reservation,
    ) -> Result<Self, Refusal> {
        let side = 1..=4096;
        if !side.contains(&width) || !side.contains(&height) {
            return Err(Refusal::Invalid);
        }
        // Both sides are at most 4096, so this cannot overflow.
        let bytes = u64::from(width) * u64::from(height) * 3;
        if buffer.len() as u64 != bytes || reservation.bytes() < buffer.capacity() as u64 {
            return Err(Refusal::Invalid);
        }
        Ok(Self {
            buffer,
            width,
            height,
            _reservation: reservation,
        })
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
