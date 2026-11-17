//! Whether a VM desktop is still being added, as only the station's
//! lifecycle record tells: shared by every copy of a selection, so a
//! long-lived client sees the add reach its end, and looked at again only
//! when the record's file changed, by one caller at a time.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};

/// Where a VM's add stands for the commands on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Added, or never added through a lifecycle: as the settings name it.
    Open,
    /// Still being added: only the person's Stop reaches it.
    Adding,
    /// Its lifecycle record cannot be read: only the person's Stop reaches
    /// it until it can.
    Uncertain,
}

/// What a record's file was when it was last read: its inode, length and
/// change time; none while there is no file.
pub type Mark = Option<(u64, u64, i64, i64)>;

#[derive(Debug, Default)]
struct Kept {
    admission: AtomicU8,
    /// The mark last read; none before the first look.
    seen: Mutex<Option<Mark>>,
}

/// A selection's admission, shared by its copies.
#[derive(Clone, Debug, Default)]
pub struct Adding(Arc<Kept>);

impl Adding {
    /// Where it stands now.
    #[must_use]
    pub fn get(&self) -> Admission {
        match self.0.admission.load(Ordering::SeqCst) {
            1 => Admission::Adding,
            2 => Admission::Uncertain,
            _ => Admission::Open,
        }
    }

    /// Say it stands at `admission`, whatever was read.
    pub fn set(&self, admission: Admission) {
        let value = match admission {
            Admission::Open => 0,
            Admission::Adding => 1,
            Admission::Uncertain => 2,
        };
        self.0.admission.store(value, Ordering::SeqCst);
    }

    /// Look again with `look` when the record's file is now `mark` and was
    /// otherwise when last read; while another caller looks, its look is
    /// taken as it lands. A look that could not tell is never kept: the
    /// next one reads again.
    pub fn renew(&self, mark: Mark, look: impl FnOnce() -> Admission) {
        let Ok(mut seen) = self.0.seen.try_lock() else {
            return;
        };
        if *seen == Some(mark) {
            return;
        }
        let admission = look();
        self.set(admission);
        *seen = (admission != Admission::Uncertain).then_some(mark);
    }

    /// Say it cannot be told now, as when the record's file cannot be
    /// looked at: the next look reads again, whatever it finds.
    pub fn doubt(&self) {
        self.set(Admission::Uncertain);
        if let Ok(mut seen) = self.0.seen.lock() {
            *seen = None;
        }
    }
}

impl PartialEq for Adding {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

#[cfg(test)]
#[path = "adding_tests.rs"]
mod tests;
