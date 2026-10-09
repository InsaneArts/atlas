use crate::error::{Error, Result};
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
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

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let path = fs::canonicalize(path).map_err(|_| {
            Error::new(
                "config",
                "Missing configuration. Copy understudy.example.json to understudy.local.json \
                 and set the helper UID and socket.",
            )
        })?;
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(16_385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16_384 {
            return Err(Error::new("config", "Configuration exceeds 16 KiB"));
        }
        let mut config: Self = serde_json::from_slice(&bytes)?;
        crate::session::validate_uid(config.helper_uid)?;
        let parent = path
            .parent()
            .ok_or_else(|| Error::new("config", "Configuration has no parent directory"))?;
        for value in [
            &mut config.socket_path,
            &mut config.output_dir,
            &mut config.identity_probe,
            &mut config.input_socket,
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
            *value = resolve_path(value)?;
        }
        Ok(config)
    }
}

pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("config", "HOME is unavailable"))
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
