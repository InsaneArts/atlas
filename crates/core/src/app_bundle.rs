//! Where Atlas is installed: the app bundle's layout, whether other accounts
//! can reach it, and who signed it.
pub mod signature;

use signature::app_signed;
pub use signature::{APP_IDENTIFIER, IDENTIFIER, developer_id};
pub use signature::{
    executable_of, require_current, requirement_for, runs_current, signature, signed_daemon,
};

use crate::error::{Code, Error, Result};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const BUNDLE_EXECUTABLE: &str = "Contents/MacOS/atlas";

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
pub fn app_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let app = macos.parent()?.parent()?;
    (macos.ends_with("Contents/MacOS") && app.extension().is_some_and(|ext| ext == "app"))
        .then(|| app.to_owned())
}

/// Whether other accounts (the helper) can reach and run `executable`:
/// every directory above it searchable and the file readable and executable
/// by others. ACLs are not counted, so this errs on the side of refusing.
pub fn reachable_by_others(executable: &Path) -> bool {
    // These callers ask only for classification. Replacement uses the fallible
    // gate directly so filesystem causes are retained rather than projected away.
    require_helper_access(executable).is_ok()
}

/// The same conservative mode policy for an update candidate, before exchange.
///
/// # Errors
///
/// Returns an error when the helper account can't read and run it.
pub fn require_helper_access(executable: &Path) -> Result<()> {
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

pub const EXECUTABLE: &str = "/Applications/Atlas.app/Contents/MacOS/atlas";

/// The stable installed identity, including when a temporary worker calls us.
///
/// # Errors
///
/// Returns an error when Atlas.app is not where the helper account can run it.
pub fn app_executable() -> Result<PathBuf> {
    app_executable_using(reachable_by_others)
}

/// # Errors
///
/// Returns an error when Atlas.app is not where the helper account can run it.
pub fn app_executable_using(reachable: impl FnOnce(&Path) -> bool) -> Result<PathBuf> {
    let executable = PathBuf::from(EXECUTABLE);
    if !reachable(&executable) {
        return Err(Error::new(
            Code::Init,
            "Atlas.app must be in /Applications so the helper account can run it",
        ));
    }

    Ok(executable)
}

/// The installed Atlas.app's executable, refused unless its developer signed it.
///
/// # Errors
///
/// Returns an error when Atlas.app is not installed there or its developer did
/// not sign it.
pub fn signed_executable() -> Result<PathBuf> {
    signed_executable_using(app_executable()?, developer_id)
}

/// # Errors
///
/// Returns an error when Atlas.app is not installed there or its developer did
/// not sign it.
pub fn signed_executable_using(
    executable: PathBuf,
    developer_id: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf> {
    // Installation location for the shared app. The approved executor
    // independently protects and verifies its own copy before using it.
    if !executable.starts_with("/Applications") {
        return Err(Error::new(
            Code::Onboarding,
            "Atlas.app must be in Applications to set up the helper. Move it there, then open it again.",
        ));
    }

    if !developer_id(&executable) {
        return Err(Error::new(
            Code::Onboarding,
            "This Atlas.app is not signed by Atlas's developer. Install it again.",
        ));
    }

    Ok(executable)
}

/// The app shipped beside `executable` in Atlas.app, if this is the
/// bundle's `atlas` and the app is there as a regular file.
fn bundled(executable: &Path) -> Option<PathBuf> {
    let directory = executable.parent()?;
    let app = directory.join(APP_EXECUTABLE);
    let regular = fs::symlink_metadata(&app).is_ok_and(|metadata| metadata.is_file());
    (directory.ends_with("Contents/MacOS") && regular).then_some(app)
}

/// The repository this binary was built from: `crates/core` is two
/// folders down from it.
pub fn checkout() -> &'static Path {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(2).unwrap_or(manifest)
}

/// The package in the checkout this binary was built from, built with SwiftPM.
fn development() -> Result<PathBuf> {
    let package = checkout().join("app");
    if !package.join("Package.swift").is_file() {
        return Err(Error::new(
            Code::Menubar,
            format!(
                "Run the menu bar from Atlas.app; install it with: {}",
                INSTALL_COMMAND
            ),
        ));
    }

    let swift = |args: &[&str]| {
        Command::new("/usr/bin/swift")
            .args(args)
            .arg("--package-path")
            .arg(&package)
            .stdin(Stdio::null())
            .output()
            .map_err(|_| {
                Error::new(
                    Code::Menubar,
                    "Building the menu bar needs the Xcode Command Line Tools; run xcode-select --install",
                )
            })
    };

    let built = swift(&["build", "-c", "release", "--product", APP_EXECUTABLE])?;
    if !built.status.success() {
        return Err(Error::new(
            Code::Menubar,
            format!(
                "The menu bar did not build: {}",
                crate::name::first_line(&built.stderr, 300)
                    .unwrap_or_else(|| "no output".to_owned())
            ),
        ));
    }

    let bin = swift(&["build", "-c", "release", "--show-bin-path"])?;
    let directory = String::from_utf8_lossy(&bin.stdout).trim().to_owned();

    Ok(Path::new(&directory).join(APP_EXECUTABLE))
}

/// The app's executable: the signed one shipped in Atlas.app, or in a
/// development checkout the package built here.
///
/// # Errors
///
/// Returns an error when the supported Atlas app executable cannot be located or validated.
pub fn app_program() -> Result<PathBuf> {
    if let Some(app) = std::env::current_exe()
        .and_then(fs::canonicalize)
        .ok()
        .as_deref()
        .and_then(bundled)
    {
        // It receives the viewer's private URL: run only the app Atlas
        // signed, never a file merely placed there.
        if !app_signed(&app) {
            return Err(Error::new(
                Code::Menubar,
                "The app inside Atlas.app is not signed by Atlas",
            ));
        }

        return Ok(app);
    }

    development()
}

#[cfg(test)]
mod tests;
