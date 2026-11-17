use super::*;
use crate::vm::lifecycle::Lifecycle;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering::SeqCst},
};

fn vm() -> DesktopId {
    DesktopId::parse("0123456789abcdef01234567").unwrap()
}

/// An add's or removal's pass of the barrier of a lifecycle in `home`.
fn admitted(home: &Path) -> crate::vm::lifecycle::barrier::Admitted {
    Lifecycle::under(home).admit().unwrap()
}

fn instance(pid: i32) -> Instance {
    Instance::new(pid, [100, 0])
}

/// A VM's keepers as a test stands them in: the one serving, the work its
/// guest reports, and what launchd does once one hands over.
struct Standing {
    loaded: bool,
    /// Whether launchd runs a process for the job.
    runs: bool,
    /// Whether the VM is paired; none when that cannot be told.
    paired: Option<bool>,
    /// The keeper serving now; none while none answers.
    serving: Mutex<Option<Instance>>,
    /// The keepers that have ended.
    ended: Mutex<Vec<Instance>>,
    /// The guest's hello, or none when the read fails.
    hello: Mutex<Option<Value>>,
    /// The keeper's answer to a handover; none when it ends before
    /// answering.
    answer: Mutex<Option<Value>>,
    /// The keeper launchd starts once one hands over; none when it starts
    /// none.
    next: Mutex<Option<Instance>>,
    /// Whether Atlas VM confirms the VM's power is stopped.
    stopped: bool,
    /// The station folder, to see the record as the keeper is asked.
    folder: PathBuf,
    /// The records kept when each handover was asked.
    recorded: Mutex<Vec<Option<Record>>>,
    handovers: AtomicUsize,
}

impl Standing {
    fn new(folder: &Path) -> Self {
        Self {
            loaded: true,
            runs: true,
            paired: Some(true),
            serving: Mutex::new(Some(instance(41))),
            ended: Mutex::default(),
            hello: Mutex::new(Some(json!({"ok": true, "work": {"state": "none",
                "jobs": 0, "changes": 0, "supervisors": 0}}))),
            answer: Mutex::new(Some(json!({"ok": true, "said": true, "seq": 1,
                "outcome": "stopped"}))),
            next: Mutex::new(Some(instance(42))),
            stopped: false,
            folder: folder.to_owned(),
            recorded: Mutex::default(),
            handovers: AtomicUsize::new(0),
        }
    }

    /// The keeper serving now ends, and the next one, if any, serves.
    fn replaced(&self) {
        let mut serving = self.serving.lock().unwrap();
        if let Some(old) = serving.take() {
            self.ended.lock().unwrap().push(old);
        }
        *serving = *self.next.lock().unwrap();
    }
}

impl Keepers for Standing {
    fn loaded(&self, _: &DesktopId) -> Result<bool> {
        Ok(self.loaded)
    }

    fn runs(&self, _: &DesktopId) -> bool {
        self.runs
    }

    fn ask(&self, _: &DesktopId, request: &Value, _: Duration) -> Result<Vec<Value>> {
        let unanswered = || Err(Error::new(Code::Transport, "The keeper did not answer."));
        let Some(serving) = *self.serving.lock().unwrap() else {
            return unanswered();
        };
        match request["op"].as_str() {
            Some("hello") => Ok(vec![json!({"ok": true, "instance": serving})]),
            Some("read") => self
                .hello
                .lock()
                .unwrap()
                .clone()
                .map_or_else(unanswered, |hello| Ok(vec![hello])),
            Some("handover") => {
                self.handovers.fetch_add(1, SeqCst);
                self.recorded
                    .lock()
                    .unwrap()
                    .push(Record::kept(&self.folder).unwrap());
                let answer = self.answer.lock().unwrap().clone();
                if answer.as_ref().is_none_or(|answer| answer["ok"] == true) {
                    self.replaced();
                }
                answer.map_or_else(unanswered, |answer| Ok(vec![answer]))
            }
            _ => unanswered(),
        }
    }

    fn paired(&self, _: &DesktopId) -> Result<bool> {
        self.paired
            .ok_or_else(|| Error::new(Code::State, "The pairing record cannot be read."))
    }

    fn ended(&self, instance: &Instance) -> bool {
        self.ended.lock().unwrap().contains(instance)
    }

    fn stopped(&self, _: &DesktopId) -> bool {
        self.stopped
    }
}

/// A VM desktop named `work`, its exclusion and station folder under
/// `folder`.
fn desk(folder: &Path) -> Desk {
    let controller = Controller::of_at(
        &atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: vm() }),
        &folder.join("state"),
    );
    let station = folder.join("station");
    std::fs::create_dir_all(&station).unwrap();
    Desk::at("work", vm(), controller, station)
}

#[test]
fn an_update_is_refused_while_work_runs_on_a_vm_unless_disrupting_it_is_allowed() {
    let folder = tempfile::tempdir().unwrap();
    let standing = Standing::new(folder.path());
    *standing.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "running",
        "jobs": 1, "changes": 0, "supervisors": 2}}));

    let refused = hold(
        &Lifecycle::under(folder.path()),
        Box::new(standing),
        vec![desk(folder.path())],
        false,
    )
    .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "Work runs on VM desktop work (1 background job, 2 unresolved supervisors). Atlas was \
         not updated."
    );
    assert_eq!(refused.details["running"][0]["desktop"], "work");
    assert!(
        refused.details["next"]
            .as_str()
            .unwrap()
            .contains("--allow-disrupt")
    );

    let standing = Standing::new(folder.path());
    *standing.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "running",
        "jobs": 1}}));
    let held = hold(
        &Lifecycle::under(folder.path()),
        Box::new(standing),
        vec![desk(folder.path())],
        true,
    )
    .unwrap();
    assert!(held.idle(true).is_ok());
    assert!(held.idle(false).is_err(), "checked again before the swap");
}

#[test]
fn work_that_cannot_be_told_counts_as_running_and_a_vm_never_paired_runs_none() {
    let said = |hello: Option<Value>| work_of(hello.as_ref());

    assert_eq!(
        said(Some(json!({"ok": true, "work": {"state": "none"}}))),
        None
    );
    for unknown in [
        None,
        Some(json!({"ok": false, "error": "The read failed."})),
        Some(json!({"ok": true})),
        Some(
            json!({"ok": true, "work": {"state": "unknown", "jobs": 0, "changes": "unknown",
            "supervisors": 0}}),
        ),
        Some(json!({"ok": true, "work": {"state": "running", "jobs": 3}})),
    ] {
        assert!(said(unknown.clone()).is_some(), "{unknown:?}");
    }
    assert_eq!(
        said(Some(
            json!({"ok": true, "work": {"state": "unknown", "jobs": 0,
            "changes": "unknown", "supervisors": 0}})
        )),
        Some("changes in flight unknown".to_owned())
    );

    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let mut standing = Standing::new(folder.path());
    *standing.hello.lock().unwrap() = None;
    assert_eq!(
        running(&standing, &desk),
        Some("what runs there cannot be told".to_owned()),
        "the keeper's read failed"
    );
    standing.paired = None;
    assert!(running(&standing, &desk).is_some());
    standing.paired = Some(false);
    assert_eq!(running(&standing, &desk), None);
}

#[test]
fn a_handover_is_recorded_while_asked_and_forgotten_once_the_old_keeper_is_gone_and_the_new_one_serves()
 {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let standing = Standing::new(&desk.folder);

    let handed = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(handed["handed"], true, "{handed}");
    assert_eq!(handed["from"], 41);
    assert_eq!(handed["to"], 42);
    assert_eq!(handed["outcome"], "stopped");
    let recorded = standing.recorded.lock().unwrap()[0].clone().unwrap();
    assert_eq!(recorded.from, instance(41));
    assert_eq!(json!(recorded.change), handed["change"]);
    assert_eq!(Record::kept(&desk.folder).unwrap(), None);
    // The desktop's exclusion is given back once it is done.
    assert!(desk.controller.acquire(false).is_ok());
}

#[test]
fn a_handover_cut_short_by_its_keeper_ending_is_finished_by_the_next_run_once_the_new_one_serves() {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let standing = Standing::new(&desk.folder);
    // The keeper ended before it answered: the record stays.
    *standing.answer.lock().unwrap() = None;
    *standing.next.lock().unwrap() = None;
    let cut = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_millis(300),
    )
    .unwrap_err();
    assert_eq!(cut.code, Code::Update, "{}", cut.error);
    let kept = Record::kept(&desk.folder).unwrap().unwrap();
    assert_eq!(kept.from, instance(41));

    // launchd starts the next keeper; the next run sees it serve.
    *standing.serving.lock().unwrap() = Some(instance(42));
    let finished = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(finished["handed"], true, "{finished}");
    assert_eq!(finished["change"], json!(kept.change));
    assert_eq!(finished["to"], 42);
    assert_eq!(standing.handovers.load(SeqCst), 1, "never asked again");
    assert_eq!(Record::kept(&desk.folder).unwrap(), None);
}

#[test]
fn a_handover_whose_keeper_still_serves_is_asked_again_under_its_recorded_change_once_no_work_runs()
{
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let standing = Standing::new(&desk.folder);
    let change = Token::new();
    Record {
        change: change.clone(),
        from: instance(41),
    }
    .keep(&desk.folder)
    .unwrap();
    // Work that began since the handover was recorded refuses it.
    *standing.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "running",
        "jobs": 1, "changes": 0, "supervisors": 0}}));
    let refused = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert_eq!(refused.code, Code::Busy, "{}", refused.error);
    assert_eq!(
        standing.handovers.load(SeqCst),
        0,
        "the keeper still running is not told"
    );
    assert!(Record::kept(&desk.folder).unwrap().is_some());

    *standing.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "none"}}));
    let handed = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(handed["change"], json!(change));
    assert_eq!(
        standing.recorded.lock().unwrap()[0]
            .as_ref()
            .map(|record| record.change.clone()),
        Some(change)
    );
}

#[test]
fn a_handover_with_work_running_is_refused_and_nothing_is_recorded_or_asked() {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let standing = Standing::new(&desk.folder);
    *standing.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "running",
        "jobs": 0, "changes": 1, "supervisors": 0}}));

    let refused = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "Work runs on VM desktop work (1 change in flight). Its session keeper was not handed \
         over."
    );
    assert_eq!(standing.handovers.load(SeqCst), 0);
    assert_eq!(Record::kept(&desk.folder).unwrap(), None);

    let allowed = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        true,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(allowed["handed"], true, "{allowed}");
}

#[test]
fn a_handover_the_keeper_refuses_or_whose_new_keeper_never_serves_stays_recorded() {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let standing = Standing::new(&desk.folder);
    *standing.answer.lock().unwrap() = Some(json!({"ok": false, "code": "stopped",
        "error": "The Stop cannot be kept."}));

    let refused = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(refused.code, Code::Update);
    assert_eq!(refused.error, "The Stop cannot be kept.");
    assert!(Record::kept(&desk.folder).unwrap().is_some());

    *standing.answer.lock().unwrap() = Some(json!({"ok": true, "said": false}));
    *standing.next.lock().unwrap() = None;
    let late = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_millis(300),
    )
    .unwrap_err();
    assert!(
        late.error.contains("did not start in time"),
        "{}",
        late.error
    );
    assert!(Record::kept(&desk.folder).unwrap().is_some());
}

#[test]
fn a_handover_is_refused_while_the_desktops_exclusion_is_held_and_none_runs_without_a_keeper() {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let mut standing = Standing::new(&desk.folder);
    let held = desk.controller.acquire(false).unwrap();

    assert_eq!(
        handed(
            &standing,
            &desk,
            &admitted(folder.path()),
            true,
            Duration::from_secs(5)
        )
        .unwrap_err()
        .code,
        Code::Busy
    );
    drop(held);
    standing.loaded = false;
    let none = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        true,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(none["handed"], false, "{none}");
    assert_eq!(standing.handovers.load(SeqCst), 0);
}

#[test]
fn a_damaged_record_is_never_taken_for_none() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join(RECORD), b"{").unwrap();

    assert_eq!(Record::kept(folder.path()).unwrap_err().code, Code::State);
}

/// Keepers running from the files a test names.
struct Running {
    /// The keepers running; none when that cannot be told.
    keepers: Option<Vec<u32>>,
    images: Vec<(u32, Option<PathBuf>)>,
    alive: bool,
}

impl Processes for Running {
    fn keepers(&self) -> Result<Vec<u32>> {
        self.keepers
            .clone()
            .ok_or_else(|| Error::new(Code::Platform, "launchctl did not answer."))
    }

    fn image(&self, pid: u32) -> Option<PathBuf> {
        self.images
            .iter()
            .find(|(of, _)| *of == pid)
            .and_then(|(_, image)| image.clone())
    }

    fn alive(&self, _: u32) -> bool {
        self.alive
    }
}

#[test]
fn a_bundle_is_kept_while_a_keeper_runs_from_it_and_not_after() {
    let folder = tempfile::tempdir().unwrap();
    let previous = folder.path().join(".Atlas.app.update-x");
    let installed = folder.path().join("Atlas.app");
    for bundle in [&previous, &installed] {
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/atlas"), b"atlas").unwrap();
    }
    let from = |bundle: &Path| Some(bundle.join("Contents/MacOS/atlas"));
    let mut running = Running {
        keepers: Some(vec![7, 8]),
        images: vec![(7, from(&installed)), (8, from(&previous))],
        alive: true,
    };

    let kept = running_from_with(&running, &previous).unwrap();
    assert!(kept.contains("process 8"), "{kept}");
    assert_eq!(
        running_from_with(&running, &installed)
            .as_deref()
            .map(|why| why.contains("process 7")),
        Some(true)
    );

    // The old keeper handed over: only the new one runs, from the new bundle.
    running.keepers = Some(vec![7]);
    assert_eq!(running_from_with(&running, &previous), None);
}

#[test]
fn a_bundle_is_kept_when_which_keepers_run_from_it_cannot_be_told() {
    let bundle = Path::new("/Applications/.Atlas.app.update-x");
    let mut running = Running {
        keepers: None,
        images: Vec::new(),
        alive: true,
    };
    assert!(running_from_with(&running, bundle).is_some());

    running.keepers = Some(vec![9]);
    assert!(
        running_from_with(&running, bundle).is_some(),
        "an image that cannot be told"
    );
    running.alive = false;
    assert_eq!(
        running_from_with(&running, bundle),
        None,
        "a keeper that ended"
    );
}

#[test]
fn a_vm_whose_keeper_is_not_live_runs_nothing_only_once_atlas_vm_confirms_its_power_is_stopped() {
    let folder = tempfile::tempdir().unwrap();
    let mut standing = Standing::new(folder.path());
    // The keeper is not live: the guest's report cannot be read.
    *standing.hello.lock().unwrap() = Some(json!({"ok": false, "code": "transport",
        "error": "The VM's session is not live."}));
    assert!(
        running(&standing, &desk(folder.path())).is_some(),
        "unknown"
    );

    standing.stopped = true;
    assert_eq!(running(&standing, &desk(folder.path())), None);
    let held = hold(
        &Lifecycle::under(folder.path()),
        Box::new(standing),
        vec![desk(folder.path())],
        false,
    );
    assert!(held.is_ok(), "a stopped VM does not block the update");

    // A guest that reports work runs it, whatever its power is said to be.
    let mut busy = Standing::new(folder.path());
    busy.stopped = true;
    *busy.hello.lock().unwrap() = Some(json!({"ok": true, "work": {"state": "running",
        "jobs": 1}}));
    assert!(running(&busy, &desk(folder.path())).is_some());
}

#[test]
fn only_a_confirmed_stopped_power_says_nothing_runs() {
    assert!(powered_off(
        &json!({"ok": true, "op": "state", "power": "stopped", "confirmed": true})
    ));
    for unknown in [
        json!({"ok": true, "power": "stopped", "confirmed": false}),
        json!({"ok": true, "power": "stopped"}),
        json!({"ok": true, "power": "running", "confirmed": true}),
        json!({"ok": true, "power": "stopping", "confirmed": true}),
        json!({"ok": true, "power": "unknown", "confirmed": true}),
        json!({"ok": false, "error": "That is not a request a VM's runner serves."}),
        json!({}),
    ] {
        assert!(!powered_off(&unknown), "{unknown}");
    }
}

#[test]
fn an_unfinished_handover_is_said_plainly_until_it_is_forgotten() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(unfinished_in(folder.path(), "work"), None);

    Record::new(Token::new(), instance(41))
        .keep(folder.path())
        .unwrap();

    assert_eq!(
        unfinished_in(folder.path(), "work").as_deref(),
        Some(
            "An Atlas update is still handing work's session to the new build; run atlas update \
             again."
        )
    );
    std::fs::write(folder.path().join(RECORD), b"{").unwrap();
    assert!(unfinished_in(folder.path(), "work").is_some(), "unreadable");
    Record::forget(folder.path()).unwrap();
    assert_eq!(unfinished_in(folder.path(), "work"), None);
}

#[test]
fn a_handover_with_no_keeper_running_keeps_the_updates_stop_only_once_the_vm_is_known_idle() {
    let folder = tempfile::tempdir().unwrap();
    let desk = desk(folder.path());
    let mut standing = Standing::new(&desk.folder);
    standing.runs = false;
    // No keeper answers, so what runs in the guest cannot be told.
    *standing.serving.lock().unwrap() = None;

    let refused = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap_err();
    assert_eq!(refused.code, Code::Busy, "{}", refused.error);
    assert!(!desk.controller.stop_noted(), "nothing done");

    // Atlas VM confirms the VM is powered off: nothing runs there.
    standing.stopped = true;
    let handed = handed(
        &standing,
        &desk,
        &admitted(folder.path()),
        false,
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(handed["handed"], false, "{handed}");
    assert_eq!(handed["noted"], true);
    assert_eq!(
        desk.controller.stop_revision().unwrap().unwrap().why(),
        StopWhy::Update
    );
    assert_eq!(standing.handovers.load(SeqCst), 0);
}

/// launchd's keeper jobs as a test names them: each label, and what it runs.
struct Loaded(Vec<(String, Option<Vec<String>>)>);

impl Jobs for Loaded {
    fn labels(&self) -> Result<Vec<String>> {
        Ok(self.0.iter().map(|(label, _)| label.clone()).collect())
    }

    fn job(&self, label: &str) -> Result<Job> {
        Ok(self
            .0
            .iter()
            .find(|(of, _)| of == label)
            .map_or(Job::Absent, |(_, arguments)| {
                Job::Loaded(atlas_core::launchd::Loaded {
                    pid: Some(812),
                    arguments: arguments.clone(),
                    ..atlas_core::launchd::Loaded::default()
                })
            }))
    }
}

/// A keeper's job for `vm`, its agent pinning `settings`.
fn keeping(vm: &DesktopId, settings: &Path) -> (String, Option<Vec<String>>) {
    let key = RecordKey::vm(vm).as_str().to_owned();
    let arguments = [
        "/Applications/Atlas.app/Contents/MacOS/atlas",
        "station",
        "keep",
        "--key",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([key, "--settings".to_owned(), settings.display().to_string()])
    .collect();
    (format!("{LABELS}{}", vm.as_str()), Some(arguments))
}

/// Settings at `path` that list the VM `vm` as `work`.
fn listing(path: &Path, vm: &DesktopId) {
    std::fs::write(
        path,
        format!(
            "default = \"work\"\n\n[[desktops]]\nkind = \"vm\"\nname = \"work\"\nid = \"{}\"\n",
            vm.as_str()
        ),
    )
    .unwrap();
}

#[test]
fn every_keeper_launchd_runs_is_found_by_the_settings_its_agent_pins() {
    let folder = tempfile::tempdir().unwrap();
    let pinned = folder.path().join("pinned.toml");
    listing(&pinned, &vm());

    let found = discover_with(&Loaded(vec![keeping(&vm(), &pinned)])).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key(), format!("vm-{}", vm().as_str()));
    assert_eq!(found[0].settings(), pinned);
    assert_eq!(found[0].selected.name, "work");
    assert!(discover_with(&Loaded(Vec::new())).unwrap().is_empty());
}

#[test]
fn an_update_whose_settings_omit_a_running_keepers_vm_still_finds_it_and_refuses_one_unaccounted_for()
 {
    let folder = tempfile::tempdir().unwrap();
    // The update was given these settings, which list no VM at all.
    let given = folder.path().join("given.toml");
    std::fs::write(
        &given,
        "helper_uid = 60001\nsocket_path = \"s.sock\"\nidentity_probe = \"atlas\"\n\
         input_socket = \"i.sock\"\noutput_dir = \"runs\"\n",
    )
    .unwrap();
    let pinned = folder.path().join("pinned.toml");
    listing(&pinned, &vm());
    let found = discover_with(&Loaded(vec![keeping(&vm(), &pinned)])).unwrap();
    assert_eq!(found[0].vm, vm(), "found whatever the update was given");

    let other = DesktopId::parse("ffffffffffffffffffffffff").unwrap();
    for (job, why) in [
        (keeping(&other, &pinned), "list no such VM"),
        (
            keeping(&vm(), &folder.path().join("gone.toml")),
            "cannot be read",
        ),
        (keeping(&vm(), &given), "list no such VM"),
        (
            (format!("{LABELS}{}", vm().as_str()), None),
            "runs no keeper",
        ),
        (
            (
                format!("{LABELS}{}", vm().as_str()),
                Some(vec!["/bin/sh".to_owned()]),
            ),
            "runs no keeper",
        ),
        ((format!("{LABELS}not-a-vm"), None), "names no VM"),
    ] {
        let label = job.0.clone();
        let refused = discover_with(&Loaded(vec![job])).unwrap_err();

        assert_eq!(refused.code, Code::Update);
        assert!(refused.error.contains(&label), "{}", refused.error);
        assert!(refused.error.contains(why), "{why}: {}", refused.error);
        assert!(refused.error.contains("Atlas was not updated"));
    }
    // A keeper whose agent keeps another VM than its label names.
    let mut crossed = keeping(&vm(), &pinned);
    crossed.0 = format!("{LABELS}{}", other.as_str());
    assert!(
        discover_with(&Loaded(vec![crossed]))
            .unwrap_err()
            .error
            .contains("keeps another VM")
    );
}

#[test]
fn an_update_and_a_handover_are_refused_while_an_uninstalls_vm_phase_is_under_way() {
    let folder = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(folder.path());
    let uninstalling = lifecycle.uninstalling(&Token::new(), &[]).unwrap();
    drop(uninstalling);

    let update = hold(
        &lifecycle,
        Box::new(Standing::new(folder.path())),
        vec![desk(folder.path())],
        true,
    )
    .unwrap_err();
    let handover = lifecycle.admit().unwrap_err();
    let alone = hold(
        &lifecycle,
        Box::new(Standing::new(folder.path())),
        Vec::new(),
        true,
    );

    for refused in [update, handover] {
        assert!(
            refused.error.contains("being uninstalled"),
            "{}",
            refused.error
        );
    }
    assert!(alone.is_ok(), "an update with no VM touches none");
}
