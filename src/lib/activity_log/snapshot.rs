//! One bounded, ordered view of the retained log, shared by its readers.
use super::{FILE, PREVIOUS, READ_LIMIT, Row, read_back, selected};
use std::{
    collections::HashSet,
    fs::File,
    io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Atlas polls one configured output folder. Keep only one snapshot, rather
// than retaining another byte cap's worth of rows for every folder visited.
// Holding the lock through a rebuild also coalesces concurrent cold reads.
static CACHE: Mutex<Cache> = Mutex::new(Cache { kept: None });

#[derive(Debug, PartialEq, Eq)]
struct Revision {
    device: u64,
    inode: u64,
    length: u64,
    modified: i64,
    modified_nanos: i64,
}

impl Revision {
    fn of(file: &File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
        })
    }
}

struct Snapshot {
    directory: PathBuf,
    revision: [Option<Revision>; 2],
    rows: Vec<Row>,
}

struct Cache {
    kept: Option<Snapshot>,
}

impl Cache {
    fn recent(&mut self, directory: &Path, limit: usize) -> Vec<Row> {
        self.recent_using(directory, limit, read_back)
    }

    fn recent_using(
        &mut self,
        directory: &Path,
        limit: usize,
        mut read: impl FnMut(File, &mut u64, &mut Vec<Row>) -> io::Result<()>,
    ) -> Vec<Row> {
        if limit == 0 {
            return Vec::new();
        }

        // Inspect the descriptors try_open_plain checked, never following links
        // or waiting on pipes. Open both first, retaining the rotation guard.
        let mut files = [None, None];
        let mut revision = [None, None];
        let mut complete = true;
        for (index, name) in [FILE, PREVIOUS].into_iter().enumerate() {
            match crate::files::try_open_plain(&directory.join(name)) {
                Ok(Some(file)) => match Revision::of(&file) {
                    Ok(found) => {
                        revision[index] = Some(found);
                        files[index] = Some(file);
                    }
                    Err(_) => complete = false,
                },
                Ok(None) => {}
                Err(_) => complete = false,
            }
        }
        if let [Some(current), Some(previous)] = &revision
            && (current.device, current.inode) == (previous.device, previous.inode)
        {
            files[1] = None;
            revision[1] = None;
        }
        let unchanged = self.kept.as_ref().is_some_and(|kept| {
            complete && kept.directory == directory && kept.revision == revision
        });
        if !unchanged {
            // Discard the previous candidates before allocating their
            // replacement: the cache retains only one read budget of rows.
            self.kept = None;
            let mut rows = Vec::new();
            let mut room = READ_LIMIT;
            #[cfg(test)]
            observed(directory);
            for file in files.into_iter().flatten() {
                if read(file, &mut room, &mut rows).is_err() {
                    complete = false;
                }
            }
            // Wall-clock time can move backwards during a command. Its last
            // committed step wins by append order, before timestamps order
            // the commands for display. Current-file rows precede rotated ones.
            let mut commands = HashSet::new();
            rows.retain(|row| {
                row.command
                    .as_ref()
                    .is_none_or(|command| commands.insert(command.clone()))
            });
            rows.sort_by_key(|row| std::cmp::Reverse(row.at_nanos()));
            if !complete {
                // Preserve the endpoint's best-effort response, but retry the
                // read next time even if the file's revision has not changed.
                return selected(&rows, limit);
            }
            self.kept = Some(Snapshot {
                directory: directory.to_owned(),
                revision,
                rows,
            });
        }
        selected(&self.kept.as_ref().expect("snapshot was built").rows, limit)
    }
}

/// Synchronous consumers already reading off the runtime share this cache.
pub(super) fn recent(directory: &Path, limit: usize) -> Vec<Row> {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .recent(directory, limit)
}

#[cfg(test)]
use std::{collections::HashMap, sync::Arc, thread::ThreadId};

#[cfg(test)]
type ParseThreads = Arc<Mutex<Vec<ThreadId>>>;

#[cfg(test)]
static OBSERVATIONS: std::sync::LazyLock<Mutex<HashMap<PathBuf, ParseThreads>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// A directory-scoped parse probe, independent of other tests' reads.
#[cfg(test)]
pub(crate) struct Observation {
    directory: PathBuf,
    threads: ParseThreads,
}

#[cfg(test)]
impl Observation {
    pub(crate) fn watch(directory: &Path) -> Self {
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

    pub(crate) fn threads(&self) -> Vec<ThreadId> {
        self.threads.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Drop for Observation {
    fn drop(&mut self) {
        OBSERVATIONS.lock().unwrap().remove(&self.directory);
    }
}

#[cfg(test)]
fn observed(directory: &Path) {
    if let Some(threads) = OBSERVATIONS.lock().unwrap().get(directory) {
        threads.lock().unwrap().push(std::thread::current().id());
    }
}

#[cfg(test)]
mod tests;
