//! How the daemon starts a supervisor, and what it hears back: one format,
//! written and read here alone.
use std::{
    ffi::OsString,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::ExitStatus,
    time::Duration,
};

/// The longest frame the supervisor says the program's end in.
pub const FRAME: usize = 32;

/// What a supervisor is asked to run, and how long its lease is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub lease: Duration,
    /// Where the supervisor leaves its proof, by its full path.
    pub proofs: PathBuf,
    /// The name the program is started under.
    pub argv0: OsString,
    /// The program, by its full path.
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
}

impl Invocation {
    /// The words the supervisor is started with, after its own command.
    pub fn words(&self) -> Vec<OsString> {
        let mut words = vec![
            OsString::from(self.lease.as_millis().to_string()),
            self.proofs.clone().into_os_string(),
            self.argv0.clone(),
            self.program.clone().into_os_string(),
        ];
        words.extend(self.arguments.iter().cloned());
        words
    }

    /// The invocation `words` give; none when they are not one.
    pub fn parse(words: &[OsString]) -> Option<Self> {
        let [lease, proofs, argv0, program, arguments @ ..] = words else {
            return None;
        };
        let lease = lease.to_str()?;
        if lease.is_empty() || !lease.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let lease = Duration::from_millis(lease.parse().ok().filter(|ms| *ms > 0)?);
        let (proofs, program) = (PathBuf::from(proofs), PathBuf::from(program));
        if !program.is_absolute() || !proofs.is_absolute() || argv0.is_empty() {
            return None;
        }
        Some(Self {
            lease,
            proofs,
            argv0: argv0.clone(),
            program,
            arguments: arguments.to_vec(),
        })
    }
}

/// How the daemon starts a supervisor: a program, and the words before
/// the invocation's own.
#[derive(Debug, Clone)]
pub struct Supervisor {
    program: PathBuf,
    before: Vec<OsString>,
}

impl Supervisor {
    /// This very program's hidden supervisor command.
    ///
    /// # Errors
    ///
    /// Returns the error of finding this program.
    pub fn atlas() -> std::io::Result<Self> {
        Ok(Self::new(
            std::env::current_exe()?,
            vec!["machine-supervise".into(), "--".into()],
        ))
    }

    /// A supervisor started as `program` with `before` ahead of the
    /// invocation's words.
    pub fn new(program: PathBuf, before: Vec<OsString>) -> Self {
        Self { program, before }
    }

    /// This test binary, running only the test that is a supervisor when
    /// given an invocation after `--`.
    #[cfg(test)]
    pub(crate) fn of_these_tests() -> Self {
        Self::new(
            std::env::current_exe().expect("the test binary"),
            [
                "--exact",
                "machine::supervise::tests::a_supervisor_runs_what_it_is_given_when_started_for_these_tests",
                "--test-threads=1",
                "-q",
                "--",
            ]
            .map(OsString::from)
            .to_vec(),
        )
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Every word the supervisor is started with for `invocation`.
    pub fn arguments(&self, invocation: &Invocation) -> Vec<OsString> {
        let mut arguments = self.before.clone();
        arguments.extend(invocation.words());
        arguments
    }
}

/// The frame that says the program ended with `status`.
pub fn told(status: ExitStatus) -> Vec<u8> {
    format!("status {}\n", status.into_raw()).into_bytes()
}

/// The frame that says the program started.
pub const STARTED: &[u8] = b"started\n";

/// The program's end a whole frame tells; none for anything else.
pub fn heard(frame: &[u8]) -> Option<ExitStatus> {
    let raw = frame.strip_prefix(b"status ")?.strip_suffix(b"\n")?;
    let raw = std::str::from_utf8(raw).ok()?;
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok().map(ExitStatus::from_raw)
}

#[cfg(test)]
#[path = "invocation_tests.rs"]
mod tests;
