use crate::error::{Code, Error, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Setup, message)
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    identifier: Option<String>,
    cdhash: Option<String>,
    kind: Option<String>,
    team: Option<String>,
}

/// Parse `codesign -dvvv` diagnostics (written to stderr).
fn parse_signature(text: &str) -> Signature {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };

    Signature {
        identifier: field("Identifier="),
        cdhash: field("CDHash="),
        kind: field("Signature="),
        team: field("TeamIdentifier=").filter(|team| team != "not set"),
    }
}

pub fn signature(path: &Path) -> Option<Signature> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/codesign").arg("-dvvv").arg(path),
        crate::bounded::FILE,
    )
    .ok()?;

    output
        .status
        .success()
        .then(|| parse_signature(&String::from_utf8_lossy(&output.stderr)))
}

/// The bundle identifier the helper daemon must be signed as.
pub const IDENTIFIER: &str = "com.insanearts.atlas";

/// The Developer ID team that signs and notarizes Atlas's releases.
const RELEASE_TEAM: &str = "539293JFA3";

/// The team this build trusts; consent follows it. A build made with
/// `ATLAS_LOCAL_TEAM` set trusts that team instead, to test on a Mac without
/// the release identity; scripts/release.sh refuses to release one.
const TEAM: &str = match option_env!("ATLAS_LOCAL_TEAM") {
    Some(team) => team,
    None => RELEASE_TEAM,
};

/// The identifier the app's executable inside Atlas.app is signed as.
pub const APP_IDENTIFIER: &str = "com.insanearts.atlas.bar";

/// The code requirement for code signed as `identifier`: a Developer ID
/// Application signature by Atlas's team. install.sh checks the same.
pub fn requirement_for(identifier: &str) -> String {
    format!(
        "anchor apple generic and identifier \"{identifier}\" \
         and certificate 1[field.1.2.840.113635.100.6.2.6] \
         and certificate leaf[field.1.2.840.113635.100.6.1.13] \
         and certificate leaf[subject.OU] = \"{TEAM}\""
    )
}

/// Whether `target` satisfies Atlas's signing requirement for `identifier`.
/// Setup reads this every second and a half, so `codesign` gets a time
/// limit; one that runs past it reads as not signed, as any failure does.
fn satisfies(target: &std::ffi::OsStr, identifier: &str) -> bool {
    crate::bounded::output(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "-R"])
            .arg(format!("={}", requirement_for(identifier)))
            .arg(target),
        crate::bounded::FILE,
    )
    .is_ok_and(|output| output.status.success())
}

/// Whether the file at `path` is Atlas signed by its Developer ID.
pub fn developer_id(path: &Path) -> bool {
    satisfies(path.as_os_str(), IDENTIFIER)
}

/// Whether the file at `path` is the app's executable signed by Atlas's
/// Developer ID.
pub(crate) fn app_signed(path: &Path) -> bool {
    satisfies(path.as_os_str(), APP_IDENTIFIER)
}

/// Whether process `pid` runs Atlas signed by its Developer ID, and
/// its loaded code is exactly the file now on disk. (`codesign -d` on a PID
/// describes the file at its path, so only verification proves this.)
fn running_developer_id(pid: u32) -> bool {
    satisfies(pid.to_string().as_ref(), IDENTIFIER)
}

/// Whether helper daemon `pid` runs `executable` exactly as it is on disk
/// now. False for a daemon still running a build an update replaced: its
/// file moved aside, or its loaded code no longer matches the file.
pub fn runs_current(pid: u32, executable: &Path) -> bool {
    executable_of(pid)
        .and_then(|path| fs::canonicalize(path).ok())
        .is_some_and(|running| {
            fs::canonicalize(executable).is_ok_and(|expected| running == expected)
                && running_developer_id(pid)
        })
}

#[allow(unsafe_code, reason = "FFI: proc_pidpath has no safe binding")]
pub fn executable_of(pid: u32) -> Option<PathBuf> {
    let size = u32::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).ok()?;
    let mut buffer = vec![0_u8; usize::try_from(size).ok()?];

    // SAFETY: the buffer is writable for the length passed.
    let length =
        unsafe { libc::proc_pidpath(i32::try_from(pid).ok()?, buffer.as_mut_ptr().cast(), size) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    buffer.truncate(length);

    Some(PathBuf::from(String::from_utf8_lossy(&buffer).into_owned()))
}

/// The code directory hash of `program`, once it is shown to be validly
/// signed as Atlas with a Developer ID.
///
/// # Errors
///
/// Returns an error when the signature can't be read or is not Atlas's.
pub fn signed_daemon(program: &Path) -> Result<String> {
    if !developer_id(program) {
        return Err(problem(format!(
            "{} is not Atlas signed with a Developer ID, so macOS consent would not \
             persist",
            program.display()
        )));
    }

    signature(program)
        .and_then(|signature| signature.cdhash)
        .ok_or_else(|| problem("The helper app's signature could not be read"))
}

/// Require the planned on-disk build before verifying its loaded Developer ID.
///
/// # Errors
///
/// Returns an error when process `pid` is not the expected signed build.
pub fn require_current(pid: u32, expected: &str) -> Result<()> {
    // The file it runs must be the planned build, and its loaded code
    // must be valid, Atlas's, and still that file.
    let installed = executable_of(pid).and_then(|path| signature(&path)?.cdhash);
    if installed.as_deref() != Some(expected) {
        return Err(problem(
            "The helper is not running the signed Atlas build that was expected. Run \
                 atlas restart.",
        ));
    }

    if !running_developer_id(pid) {
        return Err(problem(
            "The helper still runs an older build. Run atlas restart, then \
                 atlas setup --apply.",
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
