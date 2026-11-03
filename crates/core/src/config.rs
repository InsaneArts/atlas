//! Atlas's settings: which account is the helper and where its sockets, the
//! shared command and the screenshots live, read from a small TOML file
//! whose relative paths resolve against the file itself.
use crate::context::context;
use crate::error::{Code, Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    path::{Path, PathBuf},
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
}

fn default_output() -> PathBuf {
    PathBuf::from("runs")
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
    /// Only output placement can change without ending control in the current helper.
    /// Destructuring makes every added setting choose its reload behavior here.
    pub fn same_helper(&self, other: &Self) -> bool {
        let Self {
            helper_uid,
            socket_path,
            identity_probe,
            input_socket,
            output_dir: _,
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
        } = &mut config;
        crate::session::validate_uid(*helper_uid)?;

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
mod tests;
