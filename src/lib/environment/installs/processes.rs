//! Which of the helper's processes run from an install: from its path, or
//! perhaps from a copy macOS moved it to (App Translocation). The path
//! macOS names for such a copy is never proof of which bundle it came
//! from, as that path may since hold another: a process that may be the
//! install's keeps the install in place, and is never signalled.
use super::super::{Result, problem};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// What macOS's process list and App Translocation say; tests stand in for
/// both.
pub(in crate::environment) struct Looks<'a> {
    /// The helper's processes, each with the executable it runs.
    pub processes: &'a dyn Fn() -> Result<Vec<(u32, String)>>,
    /// The bundle a translocated bundle was opened from, when macOS can
    /// say.
    pub original: &'a dyn Fn(&Path) -> Option<PathBuf>,
    /// The bundle identifier a bundle's Info.plist names.
    pub bundle_id: &'a dyn Fn(&Path) -> Option<String>,
}

impl Looks<'static> {
    /// What macOS itself says.
    pub fn system() -> Self {
        Self {
            processes: &helper_processes,
            original: &super::translocate::original,
            bundle_id: &|bundle| {
                super::bundle_value(&bundle.join("Contents/Info.plist"), "CFBundleIdentifier")
            },
        }
    }
}

/// The helper's own processes, from `ps`.
fn helper_processes() -> Result<Vec<(u32, String)>> {
    let uid = nix::unistd::getuid().as_raw().to_string();
    let output = Command::new("/bin/ps")
        .args(["-U", &uid, "-o", "pid=,comm="])
        .output()?;
    if !output.status.success() {
        return Err(problem("Could not list the helper's processes"));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, executable) = line.trim_start().split_once(' ')?;
            Some((pid.parse().ok()?, executable.trim().to_owned()))
        })
        .collect())
}

/// How one process stands to an install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment) enum Runs {
    /// It runs from the install's own path.
    From,
    /// It runs from a copy macOS moved an app to that macOS traces to the
    /// install's path, or whose bundle identifier is the install's: it may
    /// be the install's own.
    Maybe,
}

/// Where translocated apps run from.
const TRANSLOCATED: &str = "/AppTranslocation/";

/// The processes of `processes` that run, or may run, from the install at
/// `destination` with bundle identifier `bundle_id`, each with how.
pub(in crate::environment) fn running_from(
    looks: &Looks,
    processes: &[(u32, String)],
    destination: &Path,
    bundle_id: Option<&str>,
) -> Vec<(u32, String, Runs)> {
    processes
        .iter()
        .filter_map(|(pid, executable)| {
            let runs = runs(looks, executable, destination, bundle_id)?;
            Some((*pid, executable.clone(), runs))
        })
        .collect()
}

fn runs(
    looks: &Looks,
    executable: &str,
    destination: &Path,
    bundle_id: Option<&str>,
) -> Option<Runs> {
    let path = Path::new(executable);
    if path.starts_with(destination) && path != destination {
        return Some(Runs::From);
    }
    if !executable.contains(TRANSLOCATED) {
        return None;
    }
    // A translocated copy is never proof of which bundle it came from: the
    // path macOS names may since hold another. One that may be the
    // install's keeps the install in place, and is never signalled.
    let bundle = translocated_bundle(path)?;
    let traced = (looks.original)(&bundle).is_some_and(|original| original == destination);
    let same_id = bundle_id
        .zip((looks.bundle_id)(&bundle))
        .is_some_and(|(install, translocated)| install == translocated);
    (traced || same_id).then_some(Runs::Maybe)
}

/// The bundle a translocated executable lies in: the first folder named
/// `*.app` after the translocation folder.
fn translocated_bundle(executable: &Path) -> Option<PathBuf> {
    let text = executable.to_str()?;
    let after = text.find(TRANSLOCATED)? + TRANSLOCATED.len();
    let mut bundle = PathBuf::from(&text[..after]);
    for part in Path::new(&text[after..]).components() {
        bundle.push(part);
        if part
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.ends_with(".app"))
        {
            return Some(bundle);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks<'a>(
        original: &'a dyn Fn(&Path) -> Option<PathBuf>,
        bundle_id: &'a dyn Fn(&Path) -> Option<String>,
    ) -> Looks<'a> {
        Looks {
            processes: &|| Ok(Vec::new()),
            original,
            bundle_id,
        }
    }

    const TRANSLOCATED_PROBE: &str =
        "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe";

    #[test]
    fn a_translocated_process_that_may_be_the_installs_keeps_it_and_is_never_proven() {
        let destination = Path::new("/Users/helper/Applications/Probe.app");
        let traced = |_: &Path| Some(destination.to_path_buf());
        let elsewhere = |_: &Path| Some(PathBuf::from("/Users/helper/Downloads/Probe.app"));
        let untraced = |_: &Path| None;
        let ours = |_: &Path| Some("test.atlas.install-probe".to_owned());
        let theirs = |_: &Path| Some("com.example.other".to_owned());
        let processes = vec![
            (1, format!("{}/Contents/MacOS/Probe", destination.display())),
            (2, TRANSLOCATED_PROBE.to_owned()),
            (3, "/Applications/Probe.app/Contents/MacOS/Probe".to_owned()),
            (
                4,
                format!("{}-old/Contents/MacOS/Probe", destination.display()),
            ),
        ];
        let id = Some("test.atlas.install-probe");
        let pids = |looks: &Looks| -> Vec<(u32, Runs)> {
            running_from(looks, &processes, destination, id)
                .into_iter()
                .map(|(pid, _, runs)| (pid, runs))
                .collect()
        };

        assert_eq!(
            pids(&looks(&traced, &theirs)),
            [(1, Runs::From), (2, Runs::Maybe)]
        );
        assert_eq!(
            pids(&looks(&elsewhere, &ours)),
            [(1, Runs::From), (2, Runs::Maybe)]
        );
        assert_eq!(pids(&looks(&elsewhere, &theirs)), [(1, Runs::From)]);
        assert_eq!(
            pids(&looks(&untraced, &ours)),
            [(1, Runs::From), (2, Runs::Maybe)]
        );
        assert_eq!(pids(&looks(&untraced, &theirs)), [(1, Runs::From)]);
        assert_eq!(
            translocated_bundle(Path::new(TRANSLOCATED_PROBE)),
            Some(PathBuf::from(
                "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app"
            ))
        );
    }
}
