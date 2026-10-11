use crate::error::{Error, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const DEFAULT_PATH: &str = "atlas.local.toml";

/// Where `atlas init` keeps the configuration: `~/.config/atlas.toml`.
/// Screenshots, logs and records stay in `~/Library/Application Support/Atlas`.
pub fn installed_path() -> Result<PathBuf> {
    Ok(home()?.join(".config/atlas.toml"))
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
        .and_then(|executable| crate::update::app_of(&executable))
        .is_some();
    // A legacy JSON twin keeps the local name, so loading reports it.
    let local = path.exists() || path.with_extension("json").exists();
    if installed || !local {
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
pub fn parse_toml<T: DeserializeOwned>(bytes: &[u8], origin: &Path) -> Result<T> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::new("config", format!("{} is not UTF-8 text", origin.display())))?;
    toml::from_str(text).map_err(|error| {
        Error::new(
            "config",
            format!("{}: {}", origin.display(), error.to_string().trim_end()),
        )
    })
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let path = fs::canonicalize(path).map_err(|error| {
            // Only a path with nothing there is "not set up"; a dangling link
            // or an unreadable folder is a file to repair, which init would
            // not replace.
            if fs::symlink_metadata(path).is_err() {
                missing(path)
            } else {
                Error::new(
                    "config-invalid",
                    format!(
                        "Atlas could not read its settings in {}: {error}. Repair or remove that \
                         file, then run atlas init.",
                        crate::ui::text::tilde(path)
                    ),
                )
            }
        })?;
        // A file that exists but does not load is never "not set up": init
        // would refuse to replace it, so say which file and why.
        Self::read(&path).map_err(|error| {
            Error::new(
                "config-invalid",
                format!(
                    "Atlas could not read its settings in {}: {}",
                    crate::ui::text::tilde(&path),
                    error.error
                ),
            )
        })
    }
    fn read(path: &Path) -> Result<Self> {
        let path = path.to_owned();
        // A legacy JSON twin may still drive an older running process. Refuse
        // to let new invocations silently diverge from it.
        let legacy = path.with_extension("json");
        if path.extension().is_some_and(|ext| ext == "toml") && legacy.exists() {
            return Err(Error::new(
                "config",
                format!(
                    "Both {} and the legacy {} exist. JSON configuration is no longer read; \
                     confirm the TOML matches, stop processes started with the JSON file, \
                     then remove it.",
                    path.display(),
                    legacy.display()
                ),
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::new("config", "Configuration exceeds 16 KiB"));
        }
        Self::parse(&bytes, &path)
    }
    /// Parse configuration text as if it were the file at `path`: relative
    /// paths resolve against its directory and `~/` against the home.
    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self> {
        let mut config: Self = parse_toml(bytes, path)?;
        crate::session::validate_uid(config.helper_uid)?;
        let parent = path
            .parent()
            .ok_or_else(|| Error::new("config", "Configuration has no parent directory"))?;
        for (value, whole) in [
            (&mut config.socket_path, true),
            (&mut config.output_dir, true),
            // The probe's own entry is kept: it may be a link to the app that
            // install replaces, and must never resolve to (and overwrite) it.
            (&mut config.identity_probe, false),
            (&mut config.input_socket, true),
        ] {
            if value.as_os_str().is_empty() || value.to_string_lossy().contains('\0') {
                return Err(Error::new(
                    "config",
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
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).map_err(|error| Error::new("config", error.to_string()))
    }
}

fn missing(path: &Path) -> Error {
    let legacy = path.with_extension("json");
    let hint = if legacy.exists() {
        format!(
            " Configuration is TOML; {} is no longer read. Copy its values into the TOML file \
             (atlas setup --helper NAME shows the key names with default values).",
            legacy.display()
        )
    } else {
        String::new()
    };
    Error::new(
        "config",
        format!(
            "Atlas is not set up yet (no {}). Run atlas init.{hint}",
            crate::ui::text::tilde(path)
        ),
    )
}

pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("config", "HOME is unavailable"))
}

/// Resolve the directory holding `path`, keeping its final entry as named.
pub fn resolve_parent(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("config", "Cannot resolve path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::new("config", "Cannot resolve path name"))?;
    Ok(resolve_path(parent)?.join(name))
}

// Resolve existing ancestors too, so aliases of an absent socket share a lock.
pub fn resolve_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("config", "Cannot resolve path"))?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::new("config", "Cannot resolve path name"))?;
    Ok(resolve_path(parent)?.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe that is a link keeps its own path, so install replaces the
    /// link and never the executable it points to.
    #[test]
    fn the_probe_path_is_not_followed_through_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("App/atlas");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, b"").unwrap();
        let link = dir.path().join("shared/atlas");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let text = format!(
            "helper_uid = 502\nsocket_path = \"s.sock\"\nidentity_probe = \"{}\"\n\
             input_socket = \"i.sock\"\noutput_dir = \"out\"\n",
            link.display()
        );
        let config = Config::parse(text.as_bytes(), &dir.path().join("c.toml")).unwrap();
        let shared = fs::canonicalize(link.parent().unwrap()).unwrap();
        assert_eq!(config.identity_probe, shared.join("atlas"));
    }
}
