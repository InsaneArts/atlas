//! Which desktop each command uses, and each agent session. A session is
//! bound at its first command and keeps that desktop for its life, whatever
//! later becomes the default. The bindings are kept per account, apart from
//! the output folder, so moving that folder never moves a session.
//!
//! A session's record is the only evidence of what it did before bindings
//! existed: only the Second Login writes records at the output folder's
//! root. Any other session takes the default, and is told so when there is
//! a choice.
use super::{Key, store::Store};
use atlas_core::{
    config::desktops::{Selection, Settings},
    desktop::RecordKey,
    error::{Code, Error, Result},
    files::{Lock, LockFile},
};
use std::{
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::Duration,
};

/// Where the bindings are kept, in a home.
const FOLDER: &str = "Library/Application Support/Atlas/session-desktops";
/// The lock that orders choosing against a change of desktops.
const LOCK: &str = "Library/Application Support/Atlas/session-desktops.lock";
/// The mark, at an output folder's root, that its recorded sessions were
/// all bound.
const SEEDED: &str = "desktops-seeded";
/// How long a command waits for a change of desktops to finish.
const PATIENCE: Duration = Duration::from_secs(5);
/// The most a binding holds.
const LIMIT: u64 = 128;

/// Where a test's Atlas keeps its bindings in place of the account's home.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_HOME: &str = "ATLAS_TEST_BINDINGS_HOME";

/// The bindings of one account.
pub struct Bindings {
    folder: PathBuf,
    lock: PathBuf,
}

/// What a command chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chosen {
    pub key: RecordKey,
    /// The desktop's name, for the person.
    pub name: String,
    /// The session was bound just now to the default, with no evidence of
    /// an earlier desktop, while the settings offer a choice: worth telling.
    pub told: bool,
}

impl Bindings {
    /// The bindings of this account.
    ///
    /// # Errors
    ///
    /// Returns an error when this account has no usable home.
    pub fn of_account() -> Result<Self> {
        // A test's Atlas keeps its bindings in the test's own folder.
        #[cfg(any(test, feature = "test-support"))]
        if let Some(home) = std::env::var_os(TEST_HOME) {
            return Ok(Self::in_home(Path::new(&home)));
        }
        Ok(Self::in_home(&atlas_core::config::home()?))
    }

    fn in_home(home: &Path) -> Self {
        Self {
            folder: home.join(FOLDER),
            lock: home.join(LOCK),
        }
    }

    /// Choose a command's desktop from the settings at `config`, as one
    /// operation under the lock that changes of desktops take: with
    /// `session`, its binding is read or made, and with more than one
    /// desktop listed, the output folder's recorded sessions are bound to
    /// the Second Login first, once. Then `selection`, with the binding,
    /// resolves.
    ///
    /// # Errors
    ///
    /// Returns the failure to load the settings, to read or keep a binding,
    /// or to resolve; nothing falls back to another desktop.
    pub fn choose(
        &self,
        config: &Path,
        selection: &Selection,
        session: Option<&Key>,
    ) -> Result<Chosen> {
        let mut lock = Lock::Shared;
        loop {
            let _held = self.ordered(lock)?;
            let settings = Settings::load(config)?;
            let default = settings.default_desktop();
            let output = default.desktop.output_dir().to_path_buf();
            let several = settings.desktops().len() > 1;
            let binding = session.map(|session| self.folder.join(session.as_str()));
            let bound = match &binding {
                Some(path) => read(path)?,
                None => None,
            };
            let unseeded = several && !output.join(SEEDED).exists();
            let changes = unseeded || (binding.is_some() && bound.is_none());
            if changes && lock == Lock::Shared {
                lock = Lock::Exclusive;
                continue;
            }

            // Seeding binds sessions, under the exclusive hold taken above.
            if unseeded {
                self.seed(&output)?;
            }
            let (bound, by_default) = match (session, binding, bound) {
                (_, _, Some(key)) => (Some(key), false),
                (Some(session), Some(path), None) => match read(&path)? {
                    Some(key) => (Some(key), false),
                    None if Store::at(&output).holds(session) => {
                        (Some(self.keep(&path, RecordKey::user())?), false)
                    }
                    None => (Some(self.keep(&path, default.record_key())?), true),
                },
                _ => (None, false),
            };
            let resolved = settings.resolve(&Selection {
                bound,
                ..selection.clone()
            })?;
            return Ok(Chosen {
                key: resolved.record_key(),
                name: resolved.name.clone(),
                told: by_default && several && resolved.name == default.name,
            });
        }
    }

    /// Hold `session`, found after this process chose its desktop (by the
    /// helper's verification, or named by a hook), to its own binding: it
    /// is read or made with the names this process was given, and a desktop
    /// other than the one this process chose refuses before anything is
    /// done. A process that chose nothing has nothing to hold it to.
    ///
    /// # Errors
    ///
    /// Returns the failure to choose for `session`, or the refusal of a
    /// different desktop, naming the session's own.
    pub fn confirm(&self, config: &Path, session: &Key) -> Result<()> {
        let Some((_, kept)) = atlas_core::config::desktops::for_children() else {
            return Ok(());
        };
        let selection = atlas_core::config::desktops::process_selection();
        let chosen = self.choose(config, &selection, Some(session))?;
        if chosen.key.as_str() != kept {
            return Err(Error::new(
                Code::ConfigInvalid,
                format!(
                    "This session uses the desktop \"{name}\", not the one this command chose. \
                     Run it again with --desktop {name}.",
                    name = chosen.name
                ),
            ));
        }
        Ok(())
    }

    /// Bind every session with a record at `output`'s root and no binding
    /// to the Second Login, then mark the folder seeded. The caller holds
    /// the order exclusive.
    ///
    /// # Errors
    ///
    /// Refuses when the records cannot all be listed, so no command chooses
    /// while some recorded session is still unbound.
    fn seed(&self, output: &Path) -> Result<()> {
        // The output folder is checked as every command that writes there
        // checks it; one no command has used yet is made.
        atlas_core::files::private_folder(output, Code::Config)?;
        for session in Store::at(output).every()? {
            let path = self.folder.join(session.as_str());
            if read(&path)?.is_none() {
                self.keep(&path, RecordKey::user())?;
            }
        }
        atlas_core::files::write_private(&output.join(SEEDED), b"")
    }

    /// Keep `key` at `path` unless a binding got there first, whose desktop
    /// then stands: what the session uses.
    fn keep(&self, path: &Path, key: RecordKey) -> Result<RecordKey> {
        atlas_core::files::private_folder(&self.folder, Code::State)?;
        let written = atlas_core::files::write_beside(
            path,
            0o600,
            |file| Ok(file.write_all(key.as_str().as_bytes())?),
            |temporary| Ok(fs::hard_link(temporary, path)?),
        );
        match written {
            Ok(()) => Ok(key),
            Err(error) => read(path)?.ok_or(error),
        }
    }

    /// Hold the order of choices: shared to read one, exclusive to bind a
    /// session or change the desktops the settings list.
    fn ordered(&self, lock: Lock) -> Result<LockFile> {
        if let Some(parent) = self.lock.parent() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|error| Error::writing(parent, error))?;
        }
        let file = atlas_core::files::open_lock(&self.lock)?;
        atlas_core::files::lock_within(&file, lock, PATIENCE).map_err(|held| {
            Error::new(
                Code::Busy,
                "The desktops are being changed; try again in a moment",
            )
            .with_source(held.0)
        })?;
        Ok(file)
    }
}

/// The binding at `path`; none when there is none.
fn read(path: &Path) -> Result<Option<RecordKey>> {
    if fs::symlink_metadata(path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    atlas_core::files::read_own(path, LIMIT)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| RecordKey::parse(&text))
        .map(Some)
        .ok_or_else(|| {
            Error::new(
                Code::State,
                format!(
                    "Atlas could not read which desktop this session uses ({}). Remove that \
                     file to choose again.",
                    atlas_core::config::tilde(path)
                ),
            )
        })
}

#[cfg(test)]
#[path = "desktops_tests.rs"]
mod tests;
