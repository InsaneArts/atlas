use super::*;
use std::sync::Mutex;

/// The system as a test scripts it.
#[derive(Default)]
pub(super) struct Fake {
    /// The job launchd has, as `job` answers it.
    pub(super) job: Mutex<Option<atlas_core::launchd::Loaded>>,
    /// The VM's keeper's job, as `job` answers it for the keeper's label.
    pub(super) keeper_job: Mutex<Option<atlas_core::launchd::Loaded>>,
    /// A keeper's job that comes at launchd's lookup of the keeper of this
    /// number, counting from one, as a late bootstrap brings one.
    pub(super) keeper_late: Mutex<Option<(usize, atlas_core::launchd::Loaded)>>,
    /// How many times launchd was looked up for the keeper.
    pub(super) keeper_lookups: Mutex<usize>,
    /// Every `launchctl` asked, its arguments joined.
    pub(super) launched: Mutex<Vec<String>>,
    /// What the runner answers; none when nothing listens.
    pub(super) reply: Mutex<Option<Value>>,
    /// Every request the runner was asked.
    pub(super) asked: Mutex<Vec<Value>>,
    /// The passwords deleted.
    pub(super) deleted: Mutex<Vec<String>>,
    /// The enrolments deleted, by VM.
    pub(super) enrolments: Mutex<Vec<String>>,
    /// The processes still running, by PID.
    pub(super) running: Mutex<Vec<i32>>,
    /// The framework's VM processes; none when they cannot be told.
    pub(super) machines: Mutex<Option<Vec<Instance>>>,
    /// A job that comes, as a late bootstrap brings one, at launchd's
    /// lookup of this number, counting from one.
    pub(super) late: Mutex<Option<(usize, atlas_core::launchd::Loaded)>>,
    /// How many times launchd was looked up.
    pub(super) lookups: Mutex<usize>,
}

impl Fake {
    pub(super) fn new() -> Self {
        let fake = Self::default();
        *fake.machines.lock().unwrap() = Some(Vec::new());
        fake
    }
}

impl System for Fake {
    fn job(&self, _domain: &str, label: &str) -> Result<Job> {
        if label.starts_with("com.insanearts.atlas.station.") {
            let mut lookups = self.keeper_lookups.lock().unwrap();
            *lookups += 1;
            let mut late = self.keeper_late.lock().unwrap();
            if late.as_ref().is_some_and(|(at, _)| *at == *lookups) {
                *self.keeper_job.lock().unwrap() = late.take().map(|(_, loaded)| loaded);
            }
            let loaded = self.keeper_job.lock().unwrap().clone();
            return Ok(loaded.map_or(Job::Absent, Job::Loaded));
        }
        let mut lookups = self.lookups.lock().unwrap();
        *lookups += 1;
        let mut late = self.late.lock().unwrap();
        if late.as_ref().is_some_and(|(at, _)| *at == *lookups) {
            *self.job.lock().unwrap() = late.take().map(|(_, loaded)| loaded);
        }
        let loaded = self.job.lock().unwrap().clone();
        Ok(loaded.map_or(Job::Absent, Job::Loaded))
    }

    fn launchctl(&self, arguments: &[&str]) -> Result<bool> {
        self.launched.lock().unwrap().push(arguments.join(" "));
        if arguments.first() == Some(&"bootout") {
            if arguments
                .get(1)
                .is_some_and(|job| job.contains(".station."))
            {
                *self.keeper_job.lock().unwrap() = None;
            } else {
                *self.job.lock().unwrap() = None;
            }
        }
        Ok(true)
    }

    fn ask(&self, _socket: &Path, request: &Value) -> Result<Value> {
        self.asked.lock().unwrap().push(request.clone());
        self.reply
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::new(Code::Transport, "nothing listens"))
    }

    fn delete_password(&self, account: &str) -> Result<bool> {
        self.deleted.lock().unwrap().push(account.to_owned());
        Ok(true)
    }

    fn delete_enrolment(&self, desktop: &DesktopId) -> Result<bool> {
        self.enrolments
            .lock()
            .unwrap()
            .push(desktop.as_str().to_owned());
        Ok(true)
    }

    fn ended(&self, instance: &Instance) -> Option<bool> {
        Some(!self.running.lock().unwrap().contains(&instance.pid))
    }

    fn machines(&self) -> Option<Vec<Instance>> {
        self.machines.lock().unwrap().clone()
    }

    fn program(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(
            "/Applications/Atlas.app/Contents/Library/Atlas VM.app/Contents/MacOS/AtlasVM",
        ))
    }
}

/// A home of its own, with its VMs folder made.
pub(super) fn home() -> (tempfile::TempDir, Vms) {
    let home = tempfile::tempdir().unwrap();
    let vms = Vms::under(home.path(), nix::unistd::getuid().as_raw());
    std::fs::create_dir_all(&vms.parent).unwrap();
    (home, vms)
}

pub(super) fn desktop() -> DesktopId {
    DesktopId::parse("a7a5000000000000000000f1").unwrap()
}

#[test]
fn a_vms_lock_and_removal_record_are_kept_outside_its_folder() {
    let (_home, vms) = home();
    let folder = vms.folder(&desktop());

    assert!(!vms.lifecycle(&desktop()).starts_with(&folder));
    assert!(!vms.removal(&desktop()).starts_with(&folder));
    assert_eq!(
        vms.agent_plist(&desktop()).file_name().unwrap(),
        "com.insanearts.atlas.vm.a7a5000000000000000000f1.plist"
    );
}
