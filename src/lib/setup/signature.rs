use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Signature {
    identifier: Option<String>,
    pub(super) cdhash: Option<String>,
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

pub(super) fn signature(path: &Path) -> Option<Signature> {
    let output = Command::new("/usr/bin/codesign")
        .arg("-dvvv")
        .arg(path)
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| parse_signature(&String::from_utf8_lossy(&output.stderr)))
}

/// The bundle identifier the helper daemon must be signed as.
pub const IDENTIFIER: &str = "com.insanearts.atlas";

/// The Developer ID team that signs Atlas; consent follows it.
pub const TEAM: &str = "A474HM679A";

/// The identifier the app's executable inside Atlas.app is signed as.
pub const APP_IDENTIFIER: &str = "com.insanearts.atlas.bar";

/// The code requirement for code signed as `identifier`: a Developer ID
/// Application signature by Atlas's team. install.sh checks the same.
pub(crate) fn requirement_for(identifier: &str) -> String {
    format!(
        "anchor apple generic and identifier \"{identifier}\" \
         and certificate 1[field.1.2.840.113635.100.6.2.6] \
         and certificate leaf[field.1.2.840.113635.100.6.1.13] \
         and certificate leaf[subject.OU] = \"{TEAM}\""
    )
}

/// Whether `target` (a path, or a PID as text) validly satisfies a
/// Developer ID Application signature for Atlas by its team, the
/// requirement under which macOS consent persists. For a running process
/// this also checks that its loaded code still matches the file on disk.
fn satisfies_developer_id(target: &std::ffi::OsStr) -> bool {
    satisfies(target, IDENTIFIER)
}

/// Whether `target` validly satisfies the requirement for `identifier`.
fn satisfies(target: &std::ffi::OsStr, identifier: &str) -> bool {
    Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R"])
        .arg(format!("={}", requirement_for(identifier)))
        .arg(target)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Whether the file at `path` is Atlas signed by its Developer ID.
pub fn developer_id(path: &Path) -> bool {
    satisfies_developer_id(path.as_os_str())
}

/// Whether the file at `path` is the app's executable signed by Atlas's
/// Developer ID.
pub fn app_signed(path: &Path) -> bool {
    satisfies(path.as_os_str(), APP_IDENTIFIER)
}

/// Whether process `pid` runs Atlas signed by its Developer ID, and
/// its loaded code is exactly the file now on disk. (`codesign -d` on a PID
/// describes the file at its path, so only verification proves this.)
pub(super) fn running_developer_id(pid: u32) -> bool {
    satisfies_developer_id(pid.to_string().as_ref())
}

/// Whether helper daemon `pid` runs `executable` exactly as it is on disk
/// now. False for a daemon still running a build an update replaced: its
/// file moved aside, or its loaded code no longer matches the file.
pub fn runs_current(pid: u32, executable: &Path) -> bool {
    let running = executable_of(pid).and_then(|path| fs::canonicalize(path).ok());

    running.is_some() && running == fs::canonicalize(executable).ok() && running_developer_id(pid)
}

#[allow(unsafe_code, reason = "FFI: proc_pidpath has no safe binding")]
pub(super) fn executable_of(pid: u32) -> Option<PathBuf> {
    let size = u32::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).ok()?;
    let mut buffer = vec![0_u8; usize::try_from(size).ok()?];

    // SAFETY: the buffer is writable for the length passed.
    let length =
        unsafe { libc::proc_pidpath(i32::try_from(pid).ok()?, buffer.as_mut_ptr().cast(), size) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    buffer.truncate(length);

    Some(PathBuf::from(String::from_utf8_lossy(&buffer).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The installer verifies downloads against exactly setup's requirement.
    #[test]
    fn the_installer_checks_the_same_requirement() {
        let script = include_str!("../../../install.sh");
        let line = script
            .lines()
            .find_map(|line| line.strip_prefix("REQUIREMENT=\""))
            .and_then(|line| line.strip_suffix('"'))
            .expect("install.sh sets REQUIREMENT");

        let expanded = line
            .replace("\\\"", "\"")
            .replace("$IDENTIFIER", IDENTIFIER)
            .replace("$TEAM", TEAM);
        assert_eq!(expanded, requirement_for(IDENTIFIER));
        assert!(script.contains(&format!("IDENTIFIER=\"{IDENTIFIER}\"")));
        assert!(script.contains(&format!("TEAM=\"{TEAM}\"")));
    }

    /// Only Atlas signed by its own Developer ID team passes, as a file
    /// or as a running process.
    #[test]
    fn signatures_are_checked_against_atlass_identity() {
        assert!(
            !developer_id(Path::new("/bin/ls")),
            "Apple-signed, not ours"
        );

        let test = std::env::current_exe().unwrap();
        assert!(!developer_id(&test), "ad-hoc signed test build");
        assert!(
            !running_developer_id(std::process::id()),
            "this ad-hoc test process is not Atlas's Developer ID"
        );
        assert!(
            !runs_current(std::process::id(), &test),
            "only Atlas's own signed build is current"
        );
        assert!(!runs_current(std::process::id(), Path::new("/bin/ls")));
    }

    #[test]
    fn codesign_diagnostics_parse_without_inventing_fields() {
        let text = "Executable=/x\nIdentifier=com.example.sessiond\nCDHash=0c61eed3\n\
                    Signature=adhoc\nTeamIdentifier=not set\n";
        assert_eq!(
            parse_signature(text),
            Signature {
                identifier: Some("com.example.sessiond".into()),
                cdhash: Some("0c61eed3".into()),
                kind: Some("adhoc".into()),
                team: None,
            }
        );
        assert_eq!(parse_signature(""), Signature::default());
    }
}
