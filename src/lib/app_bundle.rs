//! Where Atlas is installed: the app bundle's layout and whether other
//! accounts can reach it.
use crate::error::{Code, Error, Result};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub(crate) const BUNDLE_EXECUTABLE: &str = "Contents/MacOS/atlas";

/// The app's executable, shipped beside `atlas` in Atlas.app.
pub const APP_EXECUTABLE: &str = "AtlasApp";

/// The command that installs Atlas, served by its website.
pub const INSTALL_COMMAND: &str = "curl -fsSL https://atlas.insanearts.io/install.sh | sh";

/// The Atlas.app this executable belongs to.
///
/// # Errors
///
/// Returns an error when Atlas is not running from the supported installed app layout.
pub fn installed_app() -> Result<PathBuf> {
    let executable = std::env::current_exe().and_then(fs::canonicalize)?;
    app_of(&executable).ok_or_else(|| {
        Error::new(
            Code::Update,
            format!("This atlas is not inside Atlas.app; install it with: {INSTALL_COMMAND}"),
        )
    })
}

/// The `atlas` executable inside the installed Atlas.app.
///
/// # Errors
///
/// Returns an error when the installed app or its command executable cannot be located
/// safely.
pub fn installed_executable() -> Result<PathBuf> {
    Ok(installed_app()?.join(BUNDLE_EXECUTABLE))
}

/// The app bundle `executable` is the main executable of, if any.
pub(crate) fn app_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let app = macos.parent()?.parent()?;
    (macos.ends_with("Contents/MacOS") && app.extension().is_some_and(|ext| ext == "app"))
        .then(|| app.to_owned())
}

/// Whether other accounts (the helper) can reach and run `executable`:
/// every directory above it searchable and the file readable and executable
/// by others. ACLs are not counted, so this errs on the side of refusing.
pub(crate) fn reachable_by_others(executable: &Path) -> bool {
    // These callers ask only for classification. Replacement uses the fallible
    // gate directly so filesystem causes are retained rather than projected away.
    require_helper_access(executable).is_ok()
}

/// The same conservative mode policy for an update candidate, before exchange.
pub(crate) fn require_helper_access(executable: &Path) -> Result<()> {
    let refused = || {
        Error::new(
            Code::Update,
            "Atlas.app must be readable and executable by the helper account",
        )
    };
    let mode = |path: &Path| {
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode())
            .map_err(|error| refused().with_source(error))
    };
    if mode(executable)? & 0o005 != 0o005 {
        return Err(refused());
    }
    for directory in executable.ancestors().skip(1) {
        if mode(directory)? & 0o001 == 0 {
            return Err(refused());
        }
    }
    Ok(())
}
