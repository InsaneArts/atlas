//! An update's admission and a Resume never cross: each marks itself here
//! before it touches anything, and each is refused while the other is
//! under way. Only marks are read and set under its lock, never the
//! Keychain or the disk, so a Stop and a cancel are as quick as ever.
use atlas_core::files::Token;
use std::sync::{Mutex, PoisonError};

/// What an admission's beginning came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// It is this one's now.
    Began,
    /// This very change is being admitted already, by another asking.
    Already,
    /// A Resume, another update's admission, or an unresolved update holds.
    Held,
}

/// The marks an update's admission and a Resume leave.
#[derive(Debug, Default)]
struct Marks {
    /// A Resume is committing.
    resuming: bool,
    /// The change of the update being admitted.
    admitting: Option<Token>,
    /// An update is kept and not settled, or whether one is cannot be told.
    unresolved: bool,
    /// How many admissions ended with a record kept, or perhaps kept.
    admitted: u64,
}

/// The gate both pass.
#[derive(Debug, Default)]
pub struct Gate {
    marks: Mutex<Marks>,
}

impl Gate {
    /// The gate, with an update unresolved or not, as the record says.
    #[must_use]
    pub fn new(unresolved: bool) -> Self {
        Self {
            marks: Mutex::new(Marks {
                unresolved,
                ..Marks::default()
            }),
        }
    }

    fn marks(&self) -> std::sync::MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A Resume begins, before it touches the saved Stop: refused while an
    /// update is unresolved or being admitted. It stands committing until
    /// what this gives is dropped.
    #[must_use]
    pub fn resume(&self) -> Option<Resuming<'_>> {
        let mut marks = self.marks();
        if marks.unresolved || marks.admitting.is_some() {
            return None;
        }
        marks.resuming = true;
        Some(Resuming(self))
    }

    /// The admission of the update `change` begins: refused while a Resume
    /// is committing, another update is being admitted, or one is
    /// unresolved; this very one being admitted already is said so.
    pub fn begin_admission(&self, change: &Token) -> Admission {
        let mut marks = self.marks();
        if marks.admitting.as_ref() == Some(change) {
            return Admission::Already;
        }
        if marks.resuming || marks.admitting.is_some() || marks.unresolved {
            return Admission::Held;
        }
        marks.admitting = Some(change.clone());
        Admission::Began
    }

    /// The admission ended: `kept`, its record kept, so its update stands
    /// unresolved from here.
    pub fn end_admission(&self, kept: bool) {
        let mut marks = self.marks();
        marks.admitting = None;
        if kept {
            marks.unresolved = true;
            marks.admitted += 1;
        }
    }

    /// How many admissions have kept a record so far: read before the
    /// record is, so a reading that predates one never clears it.
    pub fn admitted(&self) -> u64 {
        self.marks().admitted
    }

    /// Every update is settled, as the record read after `admitted`
    /// admissions says: unless another was admitted since, or is being.
    pub fn resolved(&self, admitted: u64) {
        let mut marks = self.marks();
        if marks.admitted == admitted && marks.admitting.is_none() {
            marks.unresolved = false;
        }
    }
}

/// A Resume committing: it ends when this is dropped, however it ends.
pub struct Resuming<'a>(&'a Gate);

impl Drop for Resuming<'_> {
    fn drop(&mut self) {
        self.0.marks().resuming = false;
    }
}
