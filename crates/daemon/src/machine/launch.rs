//! Whether this process is the guest's genuine daemon, as launchd started
//! it: the one way an agent, running as the same account, cannot be. An
//! agent can run Atlas's own signed binary, and can bootstrap a job of its
//! own with the same label; it cannot write a root-owned plist, nor make
//! launchd say a job it loaded came from one.
use atlas_core::error::{Code, Error, Result};
use std::{
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

/// The guest daemon's job.
pub const LABEL: &str = "com.insanearts.atlas.machine";

/// Where provisioning installs the job, owned by root.
pub const PLIST: &str = "/Library/LaunchAgents/com.insanearts.atlas.machine.plist";

/// What launchd says of a job: the process it runs and the plist it was
/// loaded from.
#[derive(Debug, PartialEq, Eq)]
pub struct Job {
    pub pid: Option<u32>,
    pub path: Option<String>,
}

impl Job {
    /// The job `printed` describes, as `launchctl print` writes it: its
    /// own top-level `pid = ` and `path = ` lines, never a nested one.
    /// launchd prints a job's arguments as they are, so a job can make a
    /// line of its own appear at the top level; one field named twice is
    /// such a job, and names nothing.
    #[must_use]
    pub fn parse(printed: &str) -> Self {
        let loaded = atlas_core::launchd::Loaded::parse(printed);
        Self {
            pid: loaded.pid,
            path: loaded.path.map(|path| path.to_string_lossy().into_owned()),
        }
    }
}

/// What the plist names: its label and the program with its arguments.
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Plist {
    pub label: String,
    pub program_arguments: Vec<String>,
}

/// Refuse unless launchd names process `pid` as the job loaded from the
/// root-owned plist installed at its path ([`PLIST`]), and that plist runs
/// this very program, installed root-owned, as the machine daemon.
///
/// # Errors
///
/// Refuses, with why, whenever anything does not hold; nothing is trusted
/// on a partial answer.
pub fn judge(job: &Job, pid: u32, installed: (&Path, &Plist), program: &Path) -> Result<()> {
    let (at, plist) = installed;
    if job.path.as_deref().map(Path::new) != Some(at) {
        return Err(refused(
            "launchd did not load it from Atlas's installed job",
        ));
    }
    if job.pid != Some(pid) {
        return Err(refused("launchd runs another process as Atlas's job"));
    }
    if plist.label != LABEL {
        return Err(refused("the installed job has another label"));
    }
    let arguments: Vec<&str> = plist.program_arguments.iter().map(String::as_str).collect();
    let [named, "daemon", "--machine", ..] = arguments.as_slice() else {
        return Err(refused("the installed job runs something else"));
    };
    if !same_file(Path::new(named), program) {
        return Err(refused("the installed job runs another program"));
    }
    root_owned(at)?;
    root_owned(program)
}

/// Refuse unless launchd names process `pid` as the job loaded from the
/// root-owned plist installed at its path, for the machine daemon: as
/// [`judge`] does, except for the program it runs, which an update may have
/// moved since that process started. What the root installer asks of a peer,
/// beside that peer's running code.
///
/// # Errors
///
/// Refuses, with why, whenever anything does not hold.
pub(in crate::machine) fn judge_job(job: &Job, pid: u32, installed: (&Path, &Plist)) -> Result<()> {
    let (at, plist) = installed;
    if job.path.as_deref().map(Path::new) != Some(at) {
        return Err(refused(
            "launchd did not load it from Atlas's installed job",
        ));
    }
    if job.pid != Some(pid) {
        return Err(refused("launchd runs another process as Atlas's job"));
    }
    if plist.label != LABEL {
        return Err(refused("the installed job has another label"));
    }
    if !matches!(
        plist
            .program_arguments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        [_, "daemon", "--machine", ..]
    ) {
        return Err(refused("the installed job runs something else"));
    }
    root_owned(at)
}

/// Refuse unless launchd names process `pid` as the machine job of the
/// account `uid`, as [`judge_job`] judges it.
///
/// # Errors
///
/// Refuses as [`judge_job`] does, or when launchd or the plist cannot be read.
pub(in crate::machine) async fn prove_job(uid: u32, pid: u32) -> Result<()> {
    let printed = asked("/bin/launchctl", &["print", &format!("gui/{uid}/{LABEL}")]).await?;
    let json = asked("/usr/bin/plutil", &["-convert", "json", "-o", "-", PLIST]).await?;
    let plist: Plist =
        serde_json::from_slice(&json).map_err(|_| refused("the installed job cannot be read"))?;
    judge_job(
        &Job::parse(&String::from_utf8_lossy(&printed)),
        pid,
        (Path::new(PLIST), &plist),
    )
}

/// The longest launchd or the plist reader may take to answer.
const ASKING: std::time::Duration = std::time::Duration::from_secs(5);

/// Refuse unless this process is the guest's genuine daemon, as launchd
/// says now: what loading the guest's key waits on.
///
/// # Errors
///
/// Refuses as [`judge`] does, or when launchd or the plist cannot be read.
pub async fn prove() -> Result<()> {
    proved(std::process::id()).await
}

/// Refuse unless this process's parent is the guest's genuine daemon, as
/// launchd says now: what a machine desktop's input service starts only
/// for, so a copy an agent started never serves.
///
/// # Errors
///
/// Refuses as [`judge`] does, or when launchd or the plist cannot be read.
pub async fn prove_parent() -> Result<()> {
    proved(std::os::unix::process::parent_id()).await
}

/// Refuse unless `pid` runs this very program as launchd started the
/// guest's daemon.
async fn proved(pid: u32) -> Result<()> {
    let uid = nix::unistd::getuid().as_raw();
    let printed = asked("/bin/launchctl", &["print", &format!("gui/{uid}/{LABEL}")]).await?;
    let json = asked("/usr/bin/plutil", &["-convert", "json", "-o", "-", PLIST]).await?;
    let plist: Plist =
        serde_json::from_slice(&json).map_err(|_| refused("the installed job cannot be read"))?;
    let program = std::env::current_exe().map_err(|_| refused("this program cannot be found"))?;
    let job = Job::parse(&String::from_utf8_lossy(&printed));
    judge(&job, pid, (Path::new(PLIST), &plist), &program)
}

/// What `program` with `arguments` printed, ended if it takes too long.
async fn asked(program: &str, arguments: &[&str]) -> Result<Vec<u8>> {
    let mut command = tokio::process::Command::new(program);
    command.args(arguments).kill_on_drop(true);
    let output = tokio::time::timeout(ASKING, atlas_core::spawn::output_async(&mut command))
        .await
        .map_err(|_| refused("launchd did not answer in time"))?
        .map_err(|_| refused("launchd could not be asked"))?;
    if !output.status.success() {
        return Err(refused("launchd knows no such job"));
    }
    Ok(output.stdout)
}

/// Whether `path` and `other` are one file.
fn same_file(path: &Path, other: &Path) -> bool {
    match (path.canonicalize(), other.canonicalize()) {
        (Ok(path), Ok(other)) => path == other,
        _ => false,
    }
}

/// Groups whose members may change installed files: the system's and
/// administrators', which the guest account is never one of.
const TRUSTED_GROUPS: [u32; 2] = [0, 80];

/// What decides whether an installed file could be changed by anyone but
/// root.
#[derive(Clone, Copy, Debug)]
struct Owned {
    uid: u32,
    gid: u32,
    mode: u32,
    symlink: bool,
    /// Whether its access list lets anyone write it, or cannot be read.
    listed: bool,
}

impl Owned {
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        Some(Self {
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.permissions().mode(),
            symlink: metadata.file_type().is_symlink(),
            // An access list can let anyone write what its mode does not.
            listed: atlas_core::acl::allows_write(path).unwrap_or(true),
        })
    }

    /// Whether only root, or a group the guest is never in, may change it.
    fn by_root(self) -> bool {
        let writable = self.mode & 0o002 != 0
            || (self.mode & 0o020 != 0 && !TRUSTED_GROUPS.contains(&self.gid));
        self.uid == 0 && !writable && !self.listed && !self.symlink
    }
}

/// Refuse unless `path` is a file owned by root that nobody else may
/// write, in a folder the same holds for, all the way up.
pub(in crate::machine) fn root_owned(path: &Path) -> Result<()> {
    let mut at = Some(path);
    while let Some(here) = at {
        let owned = Owned::of(here).ok_or_else(|| refused("an installed file cannot be read"))?;
        if !owned.by_root() {
            return Err(refused(
                "an installed file could be changed by someone else",
            ));
        }
        at = here
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
    }
    Ok(())
}

fn refused(why: &str) -> Error {
    Error::new(
        Code::Identity,
        format!("This is not the guest's own Atlas daemon: {why}."),
    )
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
