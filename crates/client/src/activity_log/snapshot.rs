//! One bounded, ordered view of the retained log, shared by its readers.
use super::{FILE, PREVIOUS, READ_LIMIT, Row, ordered, read_back, retained::Revision, selected};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Atlas polls one configured output folder. Keep only one snapshot, rather
// than retaining another byte cap's worth of rows for every folder visited.
// Holding the lock through a rebuild also coalesces concurrent cold reads.
static CACHE: Mutex<Cache> = Mutex::new(Cache { kept: None });

struct Snapshot {
    directory: PathBuf,
    revision: [Option<Revision>; 2],
    rows: Vec<Row>,
}

struct Cache {
    kept: Option<Snapshot>,
}

struct Reading {
    rows: Vec<Row>,
    failure: Option<io::Error>,
}

impl Reading {
    fn checked(self) -> io::Result<Vec<Row>> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.rows),
        }
    }
}

impl Cache {
    fn recent(&mut self, directory: &Path, limit: usize) -> Vec<Row> {
        self.read_using(directory, limit, read_back).rows
    }

    fn read_using(
        &mut self,
        directory: &Path,
        limit: usize,
        mut read: impl FnMut(File, &mut u64, &mut Vec<Row>) -> io::Result<bool>,
    ) -> Reading {
        if limit == 0 {
            return Reading {
                rows: Vec::new(),
                failure: None,
            };
        }

        // Inspect the descriptors try_open_plain checked, never following links
        // or waiting on pipes. Open both first, retaining the rotation guard.
        let mut files = [None, None];
        let mut revision = [None, None];
        let mut failure = None;
        for (index, name) in [FILE, PREVIOUS].into_iter().enumerate() {
            match atlas_core::files::try_open_plain(&directory.join(name)) {
                Ok(Some(file)) => match Revision::of(&file) {
                    Ok(found) => {
                        revision[index] = Some(found);
                        files[index] = Some(file);
                    }
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let [Some(current), Some(previous)] = &revision
            && current.same_file(previous)
        {
            files[1] = None;
            revision[1] = None;
        }
        if let Some(kept) = self.kept.as_ref().filter(|kept| {
            failure.is_none() && kept.directory == directory && kept.revision == revision
        }) {
            return Reading {
                rows: selected(&kept.rows, limit),
                failure: None,
            };
        }
        // Discard the previous candidates before allocating their
        // replacement: the cache retains only one read budget of rows.
        self.kept = None;
        let mut rows = Vec::new();
        let mut room = READ_LIMIT;
        #[cfg(any(test, feature = "test-support"))]
        observed(directory);
        for file in files.into_iter().flatten() {
            if let Err(error) = read(file, &mut room, &mut rows) {
                failure.get_or_insert(error);
            }
        }
        // Current-file rows precede rotated ones.
        ordered(&mut rows);
        if failure.is_some() {
            // Status keeps the rows that read, but Activity reports the
            // failure. Retry even if the revision has not changed.
            return Reading {
                rows: selected(&rows, limit),
                failure,
            };
        }
        let kept = self.kept.insert(Snapshot {
            directory: directory.to_owned(),
            revision,
            rows,
        });
        Reading {
            rows: selected(&kept.rows, limit),
            failure: None,
        }
    }
}

/// One row per command from the log in `directory`, newest first, at most
/// `limit`. A command that acted in several steps (typing in batches,
/// opening then waiting) is its newest step, and a run of the person's
/// input through a viewer is its newest row, counting every action of the
/// run. A line that does not read, or names no operation, is passed over.
///
/// Candidates are read from the end, within [`READ_LIMIT`] bytes across
/// both logs. Each command's last appended step is selected before ordering
/// by recorded time, grouping personal input and limiting. Buffered personal
/// input can be appended after a newer action,
/// including across rotation. Equal or missing times keep reverse append
/// order; rows without a readable time follow those with one.
///
/// Synchronous consumers already reading off the runtime share this cache.
pub fn recent(directory: &Path, limit: usize) -> Vec<Row> {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .recent(directory, limit)
}

/// Like [`recent`], but reports an unreadable or refused log instead of
/// returning only the rows that could be read. Missing logs are empty;
/// malformed rows are still skipped within the same byte budget.
///
/// # Errors
///
/// Returns the I/O error of an unreadable or refused log.
pub fn try_recent(directory: &Path, limit: usize) -> io::Result<Vec<Row>> {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .read_using(directory, limit, read_back)
        .checked()
}

#[cfg(any(test, feature = "test-support"))]
use std::{collections::HashMap, sync::Arc, thread::ThreadId};

#[cfg(any(test, feature = "test-support"))]
type ParseThreads = Arc<Mutex<Vec<ThreadId>>>;

#[cfg(any(test, feature = "test-support"))]
static OBSERVATIONS: std::sync::LazyLock<Mutex<HashMap<PathBuf, ParseThreads>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// A directory-scoped parse probe, independent of other tests' reads.
#[cfg(any(test, feature = "test-support"))]
pub struct Observation {
    directory: PathBuf,
    threads: ParseThreads,
}

#[cfg(any(test, feature = "test-support"))]
impl Observation {
    /// # Panics
    ///
    /// If a thread panicked while holding the probes.
    pub fn watch(directory: &Path) -> Self {
        let threads = Arc::new(Mutex::new(Vec::new()));
        OBSERVATIONS
            .lock()
            .unwrap()
            .insert(directory.to_owned(), threads.clone());
        Self {
            directory: directory.to_owned(),
            threads,
        }
    }

    /// # Panics
    ///
    /// If a thread panicked while holding the probes.
    pub fn threads(&self) -> Vec<ThreadId> {
        self.threads.lock().unwrap().clone()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for Observation {
    fn drop(&mut self) {
        OBSERVATIONS.lock().unwrap().remove(&self.directory);
    }
}

#[cfg(any(test, feature = "test-support"))]
fn observed(directory: &Path) {
    if let Some(threads) = OBSERVATIONS.lock().unwrap().get(directory) {
        threads.lock().unwrap().push(std::thread::current().id());
    }
}

#[cfg(test)]
mod tests;
