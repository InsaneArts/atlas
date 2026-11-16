//! What launchd says of one job, as `launchctl print` writes it. That is no
//! stable interface, so only the job's own top-level fields are read, a
//! field named twice names nothing, and anything launchctl cannot answer is
//! an error, never absent.
use crate::error::{Code, Error, Result};
use std::{collections::HashMap, path::PathBuf, process::Command};

/// launchctl's exit status for a user with no domain at all.
const NO_SUCH_USER_DOMAIN: i32 = 112;
/// launchctl's exit status for a service its domain does not have.
const NO_SUCH_SERVICE: i32 = 113;
/// launchctl's exit status for a domain that does not exist.
const NO_SUCH_DOMAIN: i32 = 125;

/// A job as launchd loaded it, each field none when it does not say it
/// once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    /// The process it runs now.
    pub pid: Option<u32>,
    /// The plist it was loaded from.
    pub path: Option<PathBuf>,
    /// The program it runs.
    pub program: Option<String>,
    /// That program's arguments, the program's own name first.
    pub arguments: Option<Vec<String>>,
    /// Its properties, as launchd lists them (`keepalive`, …).
    pub properties: Option<Vec<String>>,
    /// The seconds it is given to end after TERM before KILL.
    pub exit_timeout: Option<u64>,
}

/// A job in a domain.
#[derive(Debug, PartialEq, Eq)]
pub enum Job {
    /// The domain has no such job, or the domain itself does not exist.
    Absent,
    /// Loaded, as launchd says.
    Loaded(Loaded),
}

impl Loaded {
    /// The job `printed` describes: its own fields, each indented by one
    /// tab, and its `arguments` block, each argument by two. A job's
    /// arguments are printed as they are, so one could make a line of its
    /// own look like a field: any line indented by one tab counts as a
    /// field wherever it is, one named twice names nothing, and an
    /// `arguments` block holding such a line is not read.
    #[must_use]
    pub fn parse(printed: &str) -> Self {
        let mut job = Self::default();
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut arguments: Option<Vec<String>> = None;
        let mut whole = true;
        for line in printed.lines() {
            if let Some(reading) = arguments.as_mut() {
                if line == "\t}" {
                    if whole {
                        job.arguments = arguments.take();
                    }
                    arguments = None;
                    continue;
                }
                if let Some(argument) = line.strip_prefix("\t\t") {
                    reading.push(argument.to_owned());
                    continue;
                }
                whole = false;
            }
            let Some(field) = line
                .strip_prefix('\t')
                .filter(|rest| !rest.starts_with('\t'))
            else {
                continue;
            };
            let Some((name, value)) = field.split_once(" = ") else {
                continue;
            };
            *seen.entry(name).or_default() += 1;
            let value = value.trim();
            match name {
                "pid" => job.pid = value.parse().ok(),
                "path" => job.path = Some(PathBuf::from(value)),
                "program" => job.program = Some(value.to_owned()),
                "exit timeout" => job.exit_timeout = value.parse().ok(),
                "properties" => {
                    job.properties = Some(
                        value
                            .split('|')
                            .map(|property| property.trim().to_owned())
                            .filter(|property| !property.is_empty())
                            .collect(),
                    );
                }
                "arguments" if value == "{" => {
                    arguments = Some(Vec::new());
                    whole = true;
                }
                _ => {}
            }
        }
        let twice = |name: &str| seen.get(name).is_some_and(|count| *count > 1);
        if twice("pid") {
            job.pid = None;
        }
        if twice("path") {
            job.path = None;
        }
        if twice("program") {
            job.program = None;
        }
        if twice("arguments") {
            job.arguments = None;
        }
        if twice("properties") {
            job.properties = None;
        }
        if twice("exit timeout") {
            job.exit_timeout = None;
        }
        job
    }
}

/// The job `label` in `domain` (`gui/<uid>`, …), as launchd says now.
///
/// # Errors
///
/// Fails when launchctl cannot be asked, does not answer in time, or fails
/// otherwise than by naming no such job or domain.
pub fn job(domain: &str, label: &str) -> Result<Job> {
    let output = crate::bounded::output(
        Command::new("/bin/launchctl").args(["print", &format!("{domain}/{label}")]),
        crate::bounded::FILE,
    )?;
    match output.status.code() {
        Some(0) => Ok(Job::Loaded(Loaded::parse(&String::from_utf8_lossy(
            &output.stdout,
        )))),
        Some(NO_SUCH_SERVICE | NO_SUCH_DOMAIN | NO_SUCH_USER_DOMAIN) => Ok(Job::Absent),
        _ => Err(Error::new(
            Code::Platform,
            format!(
                "launchctl could not inspect {label}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )),
    }
}

/// The labels of every job in `domain` whose label starts with `prefix`, as
/// launchd lists them now.
///
/// # Errors
///
/// Fails when launchctl cannot be asked, does not answer in time, fails
/// otherwise than by naming no such domain, or lists its jobs in a shape
/// not understood: a list that cannot be read is never taken for empty.
pub fn labels(domain: &str, prefix: &str) -> Result<Vec<String>> {
    let output = crate::bounded::output(
        Command::new("/bin/launchctl").args(["print", domain]),
        crate::bounded::FILE,
    )?;
    match output.status.code() {
        Some(0) => services(&String::from_utf8_lossy(&output.stdout))
            .map(|listed| {
                listed
                    .into_iter()
                    .filter(|label| label.starts_with(prefix))
                    .collect()
            })
            .ok_or_else(|| {
                Error::new(
                    Code::Platform,
                    format!("launchctl listed the jobs of {domain} in a shape not understood"),
                )
            }),
        Some(NO_SUCH_DOMAIN | NO_SUCH_USER_DOMAIN) => Ok(Vec::new()),
        _ => Err(Error::new(
            Code::Platform,
            format!(
                "launchctl could not list the jobs of {domain}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        )),
    }
}

/// The label of every job `printed`, a domain as `launchctl print` writes
/// it, lists in its own `services` block: one job a line, indented by two
/// tabs, its PID and last status before its label. None unless there is
/// exactly one such block and every line in it is one job.
fn services(printed: &str) -> Option<Vec<String>> {
    let opening = "\tservices = {";
    if printed.lines().filter(|line| *line == opening).count() != 1 {
        return None;
    }
    let mut labels = Vec::new();
    for line in printed.lines().skip_while(|line| *line != opening).skip(1) {
        if line == "\t}" {
            return Some(labels);
        }
        let mut fields = line.strip_prefix("\t\t")?.split_whitespace();
        let (_pid, _status, label) = (fields.next()?, fields.next()?, fields.next()?);
        if fields.next().is_some() {
            return None;
        }
        labels.push(label.to_owned());
    }
    None
}

/// Run `launchctl` with `arguments`, bounded: whether it succeeded.
///
/// # Errors
///
/// Fails when launchctl cannot be run or does not end in time, which says
/// nothing of whether it took effect.
pub fn launchctl(arguments: &[&str]) -> Result<bool> {
    let output = crate::bounded::output(
        Command::new("/bin/launchctl").args(arguments),
        crate::bounded::FILE,
    )?;
    Ok(output.status.success())
}

#[cfg(test)]
#[path = "launchd_tests.rs"]
mod tests;
