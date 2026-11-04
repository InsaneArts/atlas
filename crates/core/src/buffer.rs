//! Fixed-capacity private pixel storage. Large allocations bypass malloc so
//! their anonymous pages return to the OS when the last frame owner drops.
use memmap2::MmapMut;
use std::{
    fmt, io,
    ops::{Deref, DerefMut},
};

/// Allocations above this many bytes are private anonymous mappings.
const HEAP_LIMIT: usize = 128 * 1024;

enum Storage {
    Heap(Box<[u8]>),
    Mapped(MmapMut),
}

/// Bytes that never grow past the capacity they were made with.
pub struct Buffer {
    storage: Storage,
    len: usize,
}

impl Buffer {
    /// `len` zero bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the memory cannot be reserved or mapped.
    pub fn zeroed(len: usize) -> io::Result<Self> {
        let mut buffer = Self::with_capacity(len)?;
        buffer.len = len;
        Ok(buffer)
    }

    /// An empty buffer that holds at most `capacity` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for a capacity beyond slice bounds, or when the
    /// memory cannot be reserved or mapped.
    pub fn with_capacity(capacity: usize) -> io::Result<Self> {
        if capacity > isize::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Buffer capacity exceeds slice bounds",
            ));
        }
        let storage = if capacity <= HEAP_LIMIT {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(capacity)
                .map_err(io::Error::other)?;
            bytes.resize(capacity, 0);
            Storage::Heap(bytes.into_boxed_slice())
        } else {
            // An anonymous mapping: no file, no sharing and no IPC handle.
            Storage::Mapped(MmapMut::map_anon(capacity)?)
        };
        Ok(Self { storage, len: 0 })
    }

    /// A private copy of `bytes`.
    ///
    /// # Errors
    ///
    /// Returns an error when the memory cannot be reserved or mapped.
    pub fn copy_from(bytes: &[u8]) -> io::Result<Self> {
        let mut buffer = Self::zeroed(bytes.len())?;
        buffer.copy_from_slice(bytes);
        Ok(buffer)
    }

    pub fn as_slice(&self) -> &[u8] {
        self
    }

    pub fn capacity(&self) -> usize {
        match &self.storage {
            Storage::Heap(bytes) => bytes.len(),
            Storage::Mapped(bytes) => bytes.len(),
        }
    }

    fn storage_mut(&mut self) -> &mut [u8] {
        match &mut self.storage {
            Storage::Heap(bytes) => bytes,
            Storage::Mapped(bytes) => bytes,
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            storage: Storage::Heap(Box::default()),
            len: 0,
        }
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let bytes = match &self.storage {
            Storage::Heap(bytes) => bytes.as_ref(),
            Storage::Mapped(bytes) => bytes.as_ref(),
        };
        &bytes[..self.len]
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        let len = self.len;
        &mut self.storage_mut()[..len]
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.capacity() - self.len {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Fixed buffer capacity exceeded",
            ));
        }
        let start = self.len;
        let end = start + bytes.len();
        self.storage_mut()[start..end].copy_from_slice(bytes);
        self.len = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl fmt::Debug for Buffer {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("Buffer")
            .field("len", &self.len)
            .field("capacity", &self.capacity())
            .finish()
    }
}

impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Buffer {}

impl PartialEq<Vec<u8>> for Buffer {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

#[cfg(test)]
mod tests;
