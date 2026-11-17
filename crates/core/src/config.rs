//! Atlas's settings: which account is the helper, where its sockets, the
//! shared command and the screenshots live, and what the activity keeps,
//! read from a small TOML file whose relative paths resolve against the
//! file itself.
use crate::context::context;
use crate::desktop::Desktop;
use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub mod desktops;
pub mod write;

use desktops::Settings;

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

/// `value`, a path the settings file at `path` gives: its `~/` expanded,
/// taken beside the file when relative, and resolved, its last entry kept
/// unless `whole`.
fn settle_path(value: &Path, path: &Path, whole: bool) -> Result<PathBuf> {
    if value.as_os_str().is_empty() || value.to_string_lossy().contains('\0') {
        return Err(Error::new(
            Code::Config,
            "Paths must be nonempty and contain no NUL",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::new(Code::Config, "Configuration has no parent directory"))?;
    let mut value = match value.to_string_lossy().strip_prefix("~/") {
        Some(rest) => home()?.join(rest),
        None => value.to_path_buf(),
    };
    if value.is_relative() {
        value = parent.join(&value);
    }
    if whole {
        resolve_path(&value)
    } else {
        resolve_parent(&value)
    }
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
/// errors name the file, the line and column, and why, but never the file's
/// text: a parser's own message can quote a key or a value it read, and the
/// file may be one an agent named that holds secrets.
///
/// # Errors
///
/// Returns an error naming the file, where in it and why.
pub fn parse_toml<T: DeserializeOwned>(bytes: &[u8], origin: &Path) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        Error::new(
            Code::Config,
            format!("{} is not UTF-8 text", origin.display()),
        )
    })?;

    toml::from_str(text).map_err(|error| {
        // Whether the text is TOML at all decides what can be said of it.
        let why = match toml::from_str::<toml::Table>(text) {
            Err(_) => NOT_TOML.to_owned(),
            Ok(_) => unusable(error.message()),
        };
        unparsed(&origin.display().to_string(), text, error.span(), &why)
    })
}

/// Why text that is not TOML at all does not parse.
pub(crate) const NOT_TOML: &str = "this is not valid TOML";

/// The failure to parse `text`, the file shown as `shown`, at the bytes
/// `span` for the reason `why`, which must not quote the text.
pub(crate) fn unparsed(
    shown: &str,
    text: &str,
    span: Option<std::ops::Range<usize>>,
    why: &str,
) -> Error {
    let at = span
        .and_then(|span| text.get(..span.start))
        .map(|before| {
            let line = before.matches('\n').count() + 1;
            let column = before
                .rsplit('\n')
                .next()
                .map_or(0, |last| last.chars().count())
                + 1;
            format!(", line {line}, column {column}")
        })
        .unwrap_or_default();
    Error::new(Code::Config, format!("{shown}{at}: {why}"))
}

/// Why TOML that parsed does not fit the settings it is read as, from the
/// deserializer's `message`, in Atlas's own words: the message itself can
/// quote what the file says. A missing field is named by the settings' own
/// type, never by the file.
fn unusable(message: &str) -> String {
    if let Some(field) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.strip_suffix('`'))
        .filter(|field| {
            field
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
    {
        return format!("{field} is missing");
    }
    let why = if message.starts_with("unknown field") {
        "a setting there is not one Atlas knows"
    } else if message.starts_with("unknown variant") {
        "a kind there is not one Atlas knows"
    } else if message.starts_with("invalid type") {
        "a value there has the wrong type"
    } else if message.starts_with("duplicate") {
        "a setting there is given twice"
    } else {
        "a value there is not one Atlas can use"
    };
    why.to_owned()
}

impl Config {
    /// The desktop these settings name: the hidden login of the helper
    /// account.
    pub fn desktop(&self) -> Desktop {
        Desktop::from(self.user())
    }

    /// The hidden user these settings name: a helper's settings are always
    /// a Second Login's.
    pub fn user(&self) -> crate::user::User {
        crate::user::User {
            uid: self.helper_uid,
        }
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

    /// Load and validate settings, resolving configured paths beside the settings file, as
    /// the desktop this process chose ([`desktops::choose_for_process`]).
    ///
    /// # Errors
    ///
    /// Returns a missing-settings or invalid-settings error for unreadable files, malformed
    /// TOML, unknown keys, invalid helper identity or unusable paths, and a refusal of a
    /// desktop the settings do not list.
    pub fn load(path: &Path) -> Result<Self> {
        let selected = Self::load_settings(path)?.chosen()?;
        match selected.desktop {
            desktops::Chosen::User(config) => Ok(config),
            desktops::Chosen::Vm(_) => Err(selected.unreachable()),
        }
    }

    /// Load the settings at `path` and take their Second Login: the user
    /// desktop's own setup, update and removal work on it, whichever
    /// desktop is the default.
    ///
    /// # Errors
    ///
    /// As [`Config::load`], and a refusal of settings with no Second Login.
    pub fn load_user(path: &Path) -> Result<Self> {
        Self::load_settings(path)?.user()
    }

    fn load_settings(path: &Path) -> Result<Settings> {
        // Atlas's own folder is refused whether or not a file is there, so a
        // path into it is never read nor taken for "not set up".
        crate::own_folder::refuse(path)?;
        // A path with nothing there is "not set up"; a dangling link, a loop
        // or an unreadable folder is a file to repair, which init would not
        // replace, so it says which file and why.
        let resolved = fs::canonicalize(path).map_err(|error| {
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
        // Read the resolved settings through the one guarded reader: its
        // folders proven unchangeable by any other account, opened without
        // following a link, read through that descriptor. A security refusal
        // stays as it is; anything else is a file to repair.
        let repair = |error: Error| {
            if error.code == Code::Credentials {
                return error;
            }
            Error::new(
                Code::ConfigInvalid,
                format!(
                    "Atlas could not read its settings in {}: {}",
                    tilde(&resolved),
                    error.error
                ),
            )
            .with_source(error)
        };
        let reading = crate::checked::read(&resolved).map_err(repair)?;
        Self::read(reading).map_err(repair)
    }

    fn read(reading: crate::checked::Reading) -> Result<Settings> {
        let path = reading.path().to_path_buf();
        let bytes = reading.read_up_to(LIMIT + 1).map_err(|error| {
            Error::new(Code::Io, format!("Read settings: {error}")).with_source(error)
        })?;

        if bytes.len() as u64 > LIMIT {
            return Err(Error::new(Code::Config, "Configuration exceeds 16 KiB"));
        }

        Settings::parse(&bytes, &path)
    }

    /// Parse settings text of the form that names one helper as if it were
    /// the file at `path`: relative paths resolve against its directory and
    /// `~/` against the home.
    ///
    /// # Errors
    ///
    /// Returns an error for text that is not a valid configuration.
    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self> {
        let config: Self = parse_toml(bytes, path)?;
        config.settled(path)
    }

    /// These settings validated, with their paths resolved as the file at
    /// `path` writes them.
    /// Destructuring makes every added setting choose its normalization here.
    fn settled(mut self, path: &Path) -> Result<Self> {
        let Self {
            helper_uid,
            socket_path,
            identity_probe,
            input_socket,
            output_dir,
            activity,
        } = &mut self;
        crate::session::validate_uid(*helper_uid)?;
        activity.validate()?;

        for (value, whole) in [
            (socket_path, true),
            (output_dir, true),
            // The probe's own entry is kept: it may be a link to the app that
            // install replaces, and must never resolve to (and overwrite) it.
            (identity_probe, false),
            (input_socket, true),
        ] {
            *value = settle_path(value, path, whole)?;
        }

        Ok(self)
    }

    /// The whole Mac's settings as a VM desktop takes them from the list
    /// file at `path`: its output folder settled as a helper's is, and what
    /// the activity keeps validated.
    pub(super) fn settled_shared(
        output_dir: &Path,
        activity: &Activity,
        path: &Path,
    ) -> Result<PathBuf> {
        activity.validate()?;
        settle_path(output_dir, path, true)
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
    write::activity(path, "pictures_before_actions", (!on).then(|| false.into()))
}

/// Keep pictures `days` days in the settings at `path`, as
/// [`keep_pictures`] writes them; a week leaves the setting out.
///
/// # Errors
///
/// As [`keep_pictures`], and a refusal of a number of days out of range.
pub fn keep_days(path: &Path, days: u32) -> Result<()> {
    let set = (days != a_week()).then(|| i64::from(days).into());
    write::activity(path, "keep_days", set)
}

/// Keep at most `limit` characters of typed text in the settings at
/// `path`, or all of it with none, as [`keep_pictures`] writes them.
///
/// # Errors
///
/// As [`keep_pictures`].
pub fn typed_text_limit(path: &Path, limit: Option<u32>) -> Result<()> {
    write::activity(
        path,
        "typed_text_limit",
        limit.map(|limit| i64::from(limit).into()),
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

/// Where Atlas keeps its runs unless the settings say otherwise: in its own
/// folder in this account's home.
///
/// # Errors
///
/// Returns an error when this account's home is unavailable.
pub fn runs_folder() -> Result<PathBuf> {
    Ok(home()?.join("Library/Application Support/Atlas/runs"))
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
///
/// # Errors
///
/// Fails when neither it nor the folder holding it can be resolved.
pub fn resolve_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path).map_err(context("Resolve path", path));
    }

    resolve_parent(path)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
