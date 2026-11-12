//! Atlas's settings: which account is the helper, where its sockets, the
//! shared command and the screenshots live, and what the activity keeps,
//! read from a small TOML file whose relative paths resolve against the
//! file itself.
use crate::context::context;
use crate::desktop::{Desktop, User};
use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_PATH: &str = "atlas.local.toml";

/// The configuration's place in a home, as `atlas init` keeps it.
pub const SETTINGS: &str = ".config/atlas.toml";

/// Where `atlas init` keeps the configuration: `~/.config/atlas.toml`.
/// Screenshots, logs and records stay in `~/Library/Application Support/Atlas`.
///
/// # Errors
///
/// Returns a configuration error when this account has no usable home directory.
pub fn installed_path() -> Result<PathBuf> {
    Ok(home()?.join(SETTINGS))
}

/// The configuration to use: `path` as given (by --config or ATLAS_CONFIG),
/// otherwise the installed one. Only a development build, run from a
/// checkout, uses the checkout's own `atlas.local.toml`; installed Atlas
/// never picks up a file just because it is in the current folder.
pub fn effective(given: Option<PathBuf>) -> PathBuf {
    // A file asked for by --config or ATLAS_CONFIG is always the one used.
    if let Some(path) = given {
        return path;
    }

    let path = PathBuf::from(DEFAULT_PATH);
    let installed = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .and_then(|executable| crate::app_bundle::app_of(&executable))
        .is_some();
    if installed || !path.exists() {
        return installed_path().unwrap_or(path);
    }

    path
}

const LIMIT: u64 = 16_384;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub helper_uid: u32,
    pub socket_path: PathBuf,
    pub identity_probe: PathBuf,
    pub input_socket: PathBuf,
    #[serde(default = "default_output")]
    pub output_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Activity::is_default")]
    pub activity: Activity,
}

fn default_output() -> PathBuf {
    PathBuf::from("runs")
}

/// What the activity keeps, under `[activity]`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    /// Whether a picture of the helper's screen is kept right before each
    /// agent action, for the person to see in Atlas.app. On unless turned
    /// off.
    #[serde(default = "on")]
    pub pictures_before_actions: bool,
    /// How many days the pictures are kept: a week unless set.
    #[serde(default = "a_week")]
    pub keep_days: u32,
    /// The most characters of typed text a picture's facts keep; all of it
    /// unless set.
    #[serde(default)]
    pub typed_text_limit: Option<u32>,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            pictures_before_actions: true,
            keep_days: a_week(),
            typed_text_limit: None,
        }
    }
}

impl Activity {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// How long the pictures are kept.
    pub fn keep(&self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.keep_days) * 24 * 60 * 60)
    }

    /// Refuse a setting no picture could keep to.
    fn validate(&self) -> Result<()> {
        if !(1..=KEEP_DAYS_MAX).contains(&self.keep_days) {
            return Err(Error::new(
                Code::Config,
                format!("Pictures are kept 1 to {KEEP_DAYS_MAX} days"),
            ));
        }
        if self.typed_text_limit == Some(0) {
            return Err(Error::new(
                Code::Config,
                "A typed text limit keeps at least one character",
            ));
        }
        Ok(())
    }
}

/// The most days pictures may be kept.
pub const KEEP_DAYS_MAX: u32 = 365;

fn on() -> bool {
    true
}

fn a_week() -> u32 {
    7
}

/// Parse user-authored TOML. Unknown keys are rejected by the target type and
/// errors name the file and the offending line.
///
/// # Errors
///
/// Returns an error naming the file and the offending key.
pub fn parse_toml<T: DeserializeOwned>(bytes: &[u8], origin: &Path) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|error| {
        Error::new(
            Code::Config,
            format!("{} is not UTF-8 text", origin.display()),
        )
        .with_source(error)
    })?;

    toml::from_str(text).map_err(|error| {
        Error::new(
            Code::Config,
            format!("{}: {}", origin.display(), error.to_string().trim_end()),
        )
        .with_source(error)
    })
}

impl Config {
    /// The desktop these settings name: the hidden login of the helper
    /// account.
    pub fn desktop(&self) -> Desktop {
        Desktop::from(User {
            uid: self.helper_uid,
        })
    }

    /// Only output placement and what the activity keeps can change without
    /// ending control in the current helper.
    /// Destructuring makes every added setting choose its reload behavior here.
    pub fn same_helper(&self, other: &Self) -> bool {
        let Self {
            helper_uid,
            socket_path,
            identity_probe,
            input_socket,
            output_dir: _,
            activity: _,
        } = self;
        (*helper_uid, socket_path, identity_probe, input_socket)
            == (
                other.helper_uid,
                &other.socket_path,
                &other.identity_probe,
                &other.input_socket,
            )
    }

    /// Load and validate settings, resolving configured paths beside the settings file.
    ///
    /// # Errors
    ///
    /// Returns a missing-settings or invalid-settings error for unreadable files, malformed
    /// TOML, unknown keys, invalid helper identity or unusable paths.
    pub fn load(path: &Path) -> Result<Self> {
        let path = fs::canonicalize(path).map_err(|error| {
            // Only a path with nothing there is "not set up"; a dangling link
            // or an unreadable folder is a file to repair, which init would
            // not replace.
            if fs::symlink_metadata(path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                missing(path).with_source(error)
            } else {
                Error::new(
                    Code::ConfigInvalid,
                    format!(
                        "Atlas could not read its settings in {}: {error}. Repair or remove that \
                         file, then run atlas init.",
                        tilde(path)
                    ),
                )
                .with_source(error)
            }
        })?;

        // A file that exists but does not load is never "not set up": init
        // would refuse to replace it, so say which file and why.
        Self::read(&path).map_err(|error| {
            Error::new(
                Code::ConfigInvalid,
                format!(
                    "Atlas could not read its settings in {}: {}",
                    tilde(&path),
                    error.error
                ),
            )
            .with_source(error)
        })
    }

    fn read(path: &Path) -> Result<Self> {
        let file = fs::File::open(path).map_err(|error| {
            Error::new(Code::Io, format!("Open settings: {error}")).with_source(error)
        })?;
        let bytes = crate::files::read_up_to(file, LIMIT + 1).map_err(|error| {
            Error::new(Code::Io, format!("Read settings: {error}")).with_source(error)
        })?;

        if bytes.len() as u64 > LIMIT {
            return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
        }

        Self::parse(&bytes, path)
    }

    /// Parse configuration text as if it were the file at `path`: relative
    /// paths resolve against its directory and `~/` against the home.
    /// Destructuring makes every added setting choose its normalization here.
    ///
    /// # Errors
    ///
    /// Returns an error for text that is not a valid configuration.
    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self> {
        let mut config: Self = parse_toml(bytes, path)?;
        let Self {
            helper_uid,
            socket_path,
            identity_probe,
            input_socket,
            output_dir,
            activity,
        } = &mut config;
        crate::session::validate_uid(*helper_uid)?;
        activity.validate()?;

        let parent = path
            .parent()
            .ok_or_else(|| Error::new(Code::Config, "Configuration has no parent directory"))?;

        for (value, whole) in [
            (socket_path, true),
            (output_dir, true),
            // The probe's own entry is kept: it may be a link to the app that
            // install replaces, and must never resolve to (and overwrite) it.
            (identity_probe, false),
            (input_socket, true),
        ] {
            if value.as_os_str().is_empty() || value.to_string_lossy().contains('\0') {
                return Err(Error::new(
                    Code::Config,
                    "Paths must be nonempty and contain no NUL",
                ));
            }

            if let Some(rest) = value.to_string_lossy().strip_prefix("~/") {
                *value = home()?.join(rest);
            }

            if value.is_relative() {
                *value = parent.join(&*value);
            }

            *value = if whole {
                resolve_path(value)?
            } else {
                resolve_parent(value)?
            };
        }

        Ok(config)
    }

    /// The configuration as the TOML a user would save.
    ///
    /// # Errors
    ///
    /// Returns a configuration error if the current settings cannot be serialized as TOML. This
    /// does not write a settings file.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self)
            .map_err(|error| Error::new(Code::Config, error.to_string()).with_source(error))
    }
}

/// Turn pictures before actions `on` or off in the settings at `path`,
/// changing only that setting: every other line, comment and order stays
/// as the file has it. The file is replaced in one rename, only with
/// settings that load, and only while it still holds what was read.
///
/// # Errors
///
/// Returns the failure to read, parse or write the settings, or a refusal
/// when the file changed while it was being updated.
pub fn keep_pictures(path: &Path, on: bool) -> Result<()> {
    set_activity(path, "pictures_before_actions", (!on).then(|| false.into()))
}

/// Keep pictures `days` days in the settings at `path`, as
/// [`keep_pictures`] writes them; a week leaves the setting out.
///
/// # Errors
///
/// As [`keep_pictures`], and a refusal of a number of days out of range.
pub fn keep_days(path: &Path, days: u32) -> Result<()> {
    let set = (days != a_week()).then(|| i64::from(days).into());
    set_activity(path, "keep_days", set)
}

/// Keep at most `limit` characters of typed text in the settings at
/// `path`, or all of it with none, as [`keep_pictures`] writes them.
///
/// # Errors
///
/// As [`keep_pictures`].
pub fn typed_text_limit(path: &Path, limit: Option<u32>) -> Result<()> {
    set_activity(
        path,
        "typed_text_limit",
        limit.map(|limit| i64::from(limit).into()),
    )
}

/// How long a settings write waits for another to finish.
const WRITING: Duration = Duration::from_secs(5);

/// The settings file at `path`, locked for this writer alone, so two
/// settings writes never lose one another's change: the lock is on the file
/// in place, taken again when another writer replaced it meanwhile.
fn locked(path: &Path) -> Result<crate::files::LockFile> {
    let deadline = Instant::now() + WRITING;
    loop {
        let file = fs::File::open(path).map_err(context("Read settings in", path))?;
        let file = crate::files::LockFile(file);
        crate::files::lock_retrying(&file, crate::files::Lock::Exclusive, deadline, |deadline| {
            Instant::now() < deadline
        })
        .map_err(|_| {
            Error::new(
                Code::Config,
                format!("{} is being updated; try again", tilde(path)),
            )
        })?;
        let (held, placed) = (file.0.metadata()?, fs::metadata(path)?);
        if (held.dev(), held.ino()) == (placed.dev(), placed.ino()) {
            return Ok(file);
        }
    }
}

/// Set `key` of `[activity]` to `value`, or leave it out with none, in the
/// settings at `path`, changing nothing else in the file. Another settings
/// write waits its turn.
fn set_activity(path: &Path, key: &str, value: Option<toml_edit::Value>) -> Result<()> {
    let path = fs::canonicalize(path).map_err(context("Read settings in", path))?;
    let _writing = locked(&path)?;
    let read = || -> Result<Vec<u8>> {
        let file = fs::File::open(&path)?;
        let bytes = crate::files::read_up_to(file, LIMIT + 1)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
        }
        Ok(bytes)
    };
    let before = read()?;
    let invalid = |error: &dyn std::fmt::Display| {
        Error::new(Code::Config, format!("{}: {error}", tilde(&path)))
    };
    let mut document = std::str::from_utf8(&before)
        .map_err(|error| invalid(&error))?
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| invalid(&error))?;
    let not_a_table = || invalid(&"activity is not a table");
    match value {
        Some(mut value) => {
            let activity = document
                .entry("activity")
                .or_insert_with(toml_edit::table)
                .as_table_like_mut()
                .ok_or_else(not_a_table)?;
            match activity
                .get_mut(key)
                .and_then(toml_edit::Item::as_value_mut)
            {
                Some(written) => {
                    *value.decor_mut() = written.decor().clone();
                    *written = value;
                }
                None => {
                    activity.insert(key, toml_edit::Item::Value(value));
                }
            }
        }
        None => {
            if let Some(activity) = document.get_mut("activity") {
                let activity = activity.as_table_like_mut().ok_or_else(not_a_table)?;
                activity.remove(key);
                if activity.is_empty() {
                    document.remove("activity");
                }
            }
        }
    }
    let text = document.to_string();
    if text.as_bytes() == before {
        return Ok(());
    }
    // Only settings that load again are written.
    if text.len() as u64 > LIMIT {
        return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
    }
    Config::parse(text.as_bytes(), &path)?;

    let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
    crate::files::write_beside(
        &path,
        mode,
        |file| Ok(file.write_all(text.as_bytes())?),
        |temporary| {
            if read()? != before {
                return Err(Error::new(
                    Code::Config,
                    format!(
                        "{} changed while Atlas was updating it; try again",
                        tilde(&path)
                    ),
                ));
            }
            fs::rename(temporary, &path).map_err(context("Place settings at", &path))
        },
    )
}

fn missing(path: &Path) -> Error {
    Error::new(
        Code::Config,
        format!(
            "Atlas is not set up yet (no {}). Run atlas init.",
            tilde(path)
        ),
    )
}

/// A path with the home directory written as `~`.
pub fn tilde(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match path.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// This process's home: where its own settings, caches and records live.
/// The account's home as macOS knows it, so that two shells of one account
/// share one set of locks and records whatever `$HOME` says in either;
/// `$HOME` only when the account cannot be looked up.
///
/// # Errors
///
/// Returns an error when the account's home is unavailable.
pub fn home() -> Result<PathBuf> {
    let account = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.dir)
        .filter(|dir| dir.is_absolute());

    account
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| Error::new(Code::Config, "HOME is unavailable"))
}

/// Resolve the directory holding `path`, keeping its final entry as named.
pub(crate) fn resolve_parent(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(Code::Config, "Cannot resolve path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::new(Code::Config, "Cannot resolve path name"))?;

    Ok(resolve_path(parent)?.join(name))
}

/// Resolve `path`, and its existing ancestors when it does not exist yet, so
/// aliases of an absent socket share a lock.
pub(crate) fn resolve_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path).map_err(context("Resolve path", path));
    }

    resolve_parent(path)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
