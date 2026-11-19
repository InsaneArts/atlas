use super::*;
use crate::vm::handover::hold;
use atlas_control::Controller;
use atlas_core::{desktop::DesktopId, files::Token, process::Instance};
use atlas_session::compatibility::{MACHINE, UPDATE_BOUND};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn vm() -> DesktopId {
    DesktopId::parse("0123456789abcdef01234567").unwrap()
}

/// This keeper's own build, and an older one.
fn own() -> String {
    "b1".repeat(32)
}

fn older() -> String {
    "a0".repeat(32)
}

/// A VM's keeper as a test stands it in: its hello, the work its guest
/// reports, and how it answers `update-guest`, each request heard kept.
struct Keeper {
    loaded: bool,
    hello: Mutex<Value>,
    work: Value,
    /// Its answers to `update-guest`, in turn; the last one stays.
    answers: Mutex<VecDeque<Value>>,
    asked: Mutex<Vec<Value>>,
}

impl Keeper {
    /// A keeper of this build serving a guest that runs `build`.
    fn of_guest(build: &str) -> Self {
        Self {
            loaded: true,
            hello: Mutex::new(json!({
                "ok": true,
                "op": "hello",
                "keeper": {"protocol": 1, "capabilities": ["read-v1", crate::vm::keeper::UPDATE_GUEST],
                    "build": atlas_core::VERSION},
                "guest": {"protocol": 1, "capabilities": MACHINE, "version": atlas_core::VERSION,
                    "build": build},
                "program": {"code": "c0de", "build": own(), "replaced": false},
                "instance": Instance::new(41, [100, 0]),
            })),
            work: json!({"state": "none", "jobs": 0, "changes": 0, "supervisors": 0}),
            answers: Mutex::new(VecDeque::from([
                json!({"ok": true, "op": "update-guest", "state": "updating"}),
            ])),
            asked: Mutex::default(),
        }
    }

    fn updates_asked(&self) -> usize {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| asked["op"] == "update-guest")
            .count()
    }
}

impl Keepers for Arc<Keeper> {
    fn loaded(&self, _: &DesktopId) -> Result<bool> {
        Ok(self.loaded)
    }

    fn runs(&self, _: &DesktopId) -> bool {
        self.loaded
    }

    fn ask(&self, _: &DesktopId, request: &Value, _: Duration) -> Result<Vec<Value>> {
        self.asked.lock().unwrap().push(request.clone());
        match request["op"].as_str() {
            Some("hello") => Ok(vec![self.hello.lock().unwrap().clone()]),
            Some("read") => Ok(vec![json!({"ok": true, "work": self.work})]),
            Some("update-guest") => {
                let mut answers = self.answers.lock().unwrap();
                let answer = if answers.len() > 1 {
                    answers.pop_front()
                } else {
                    answers.front().cloned()
                };
                Ok(vec![answer.unwrap_or_default()])
            }
            _ => Err(Error::new(Code::Transport, "The keeper did not answer.")),
        }
    }

    fn paired(&self, _: &DesktopId) -> Result<bool> {
        Ok(true)
    }

    fn ended(&self, _: &Instance) -> bool {
        false
    }

    fn stopped(&self, _: &DesktopId) -> bool {
        false
    }
}

/// A VM desktop named `work`, its exclusion, station folder and the
/// station's lifecycle records under `folder`.
struct Station {
    folder: tempfile::TempDir,
}

impl Station {
    fn new() -> Self {
        Self {
            folder: tempfile::tempdir().unwrap(),
        }
    }

    fn controller(&self) -> Controller {
        Controller::of_at(
            &atlas_core::desktop::Desktop::from(atlas_core::vm::Vm { id: vm() }),
            &self.folder.path().join("state"),
        )
    }

    fn station(&self) -> PathBuf {
        let station = self.folder.path().join("station");
        std::fs::create_dir_all(&station).unwrap();
        station
    }

    fn desk(&self) -> Desk {
        Desk::at("work", vm(), self.controller(), self.station())
    }

    fn lifecycle(&self) -> Lifecycle {
        Lifecycle::under(self.folder.path())
    }

    /// The VM's agents held, as `atlas update` holds them.
    fn held(&self, keeper: &Arc<Keeper>, allow_disrupt: bool) -> Result<Held> {
        hold(
            &self.lifecycle(),
            Box::new(keeper.clone()),
            vec![self.desk()],
            allow_disrupt,
        )
    }

    fn stop_why(&self) -> Option<StopWhy> {
        self.controller()
            .stop_revision()
            .unwrap()
            .map(|revision| revision.why())
    }

    /// Keep an update of the VM's Atlas as its keeper does, its update
    /// asked of the guest and not yet answered.
    fn installing(&self) -> Token {
        let update = Token::new();
        self.keep(&json!({
            "target": {"build": own(), "version": atlas_core::VERSION},
            "phase": {"phase": "asked", "stage": Token::new(),
                "staged": {"path": "/Users/Shared/Atlas/copies/x/Atlas.app",
                    "digest": "d1".repeat(32)},
                "update": update},
            "earlier": [],
        }));
        update
    }

    /// Keep an update of the VM's Atlas to this keeper's build as its keeper
    /// does, done, its guest having rolled it back.
    fn rolled_back(&self) -> Value {
        self.done(json!({"ok": false, "op": "update", "update": "rolledBack",
            "why": "The new Atlas did not start, so the old one is back."}))
    }

    /// Keep an update of the VM's Atlas to this keeper's build as its keeper
    /// does, done, as `outcome` says.
    fn done(&self, outcome: Value) -> Value {
        self.keep(&json!({
            "target": {"build": own(), "version": atlas_core::VERSION},
            "phase": {"phase": "done", "issued": [Token::new(), Token::new()],
                "outcome": outcome},
            "earlier": [],
        }));
        outcome
    }

    fn keep(&self, record: &Value) {
        atlas_core::files::write_durable(
            &self.station().join("guest-update.json"),
            &serde_json::to_vec(record).unwrap(),
            0o600,
        )
        .unwrap();
    }
}

#[test]
fn atlas_update_with_atlas_current_holds_and_updates_vms_not_on_this_build_unless_work_runs() {
    let station = Station::new();
    let keeper = Arc::new(Keeper::of_guest(&older()));

    let brought = station
        .held(&keeper, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();

    assert_eq!(
        brought,
        vec![json!({"ok": true, "op": "update-guest", "state": "updating", "desktop": "work"})]
    );
    assert_eq!(station.stop_why(), Some(StopWhy::Update));
    assert_eq!(keeper.updates_asked(), 1);

    // A VM whose Atlas is this build is left as it is, its agents never
    // stopped.
    let station = Station::new();
    let current = Arc::new(Keeper::of_guest(&own()));
    let brought = station
        .held(&current, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();
    assert_eq!(
        brought,
        vec![json!({"ok": true, "desktop": "work", "state": "current"})]
    );
    assert_eq!(station.stop_why(), None);
    assert_eq!(current.updates_asked(), 0);

    // Work running holds it all back, unless the person allowed disrupting it.
    let station = Station::new();
    let mut busy = Keeper::of_guest(&older());
    busy.work = json!({"state": "running", "jobs": 1, "changes": 0, "supervisors": 0});
    let busy = Arc::new(busy);
    let refused = station.held(&busy, false).unwrap_err();
    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "Work runs on VM desktop work (1 background job). Atlas was not updated."
    );
    assert_eq!(station.stop_why(), None);
    assert_eq!(busy.updates_asked(), 0);
    let brought = station
        .held(&busy, true)
        .unwrap()
        .brought(&station.lifecycle(), (true, false), Duration::ZERO)
        .unwrap();
    assert_eq!(brought[0]["state"], "updating");
    assert_eq!(station.stop_why(), Some(StopWhy::Update));
}

#[test]
fn a_guest_that_is_newer_not_known_or_takes_no_bound_update_is_left_as_it_is_and_never_stopped() {
    let station = Station::new();
    let newer = Keeper::of_guest(&older());
    newer.hello.lock().unwrap()["guest"]["version"] = json!("999.0.0");
    let unknown = Keeper::of_guest(&older());
    unknown.hello.lock().unwrap()["guest"] = Value::Null;
    let unbound = Keeper::of_guest(&older());
    let capabilities: Vec<&str> = MACHINE
        .into_iter()
        .filter(|capability| *capability != UPDATE_BOUND)
        .collect();
    unbound.hello.lock().unwrap()["guest"]["capabilities"] = json!(capabilities);

    let said: Vec<Value> = [newer, unknown, unbound]
        .into_iter()
        .map(|keeper| {
            let keeper = Arc::new(keeper);
            let brought = station
                .held(&keeper, false)
                .unwrap()
                .brought(&station.lifecycle(), (false, false), Duration::ZERO)
                .unwrap();
            assert_eq!(keeper.updates_asked(), 0);
            brought[0].clone()
        })
        .collect();

    assert_eq!(
        said,
        vec![
            json!({"ok": true, "desktop": "work", "state": "newer",
                "next": "Update Atlas on this Mac."}),
            json!({"ok": true, "desktop": "work", "state": "unknown"}),
            json!({"ok": true, "desktop": "work", "state": "available",
                "next": "work's Atlas cannot be updated from this Mac. Remove the VM and add it \
                         again."}),
        ]
    );
    assert_eq!(station.stop_why(), None);
}

#[test]
fn a_keeper_of_an_older_build_is_never_asked_and_the_person_is_told_to_hand_it_over_first() {
    let station = Station::new();
    let keeper = Keeper::of_guest(&older());
    keeper.hello.lock().unwrap()["keeper"]["capabilities"] = json!(["read-v1", "handover-v1"]);
    let keeper = Arc::new(keeper);
    let replaced = Keeper::of_guest(&older());
    replaced.hello.lock().unwrap()["program"]["replaced"] = json!(true);
    let replaced = Arc::new(replaced);
    let words = "work's session keeper is from an older build; run atlas update again once it \
                 is handed over.";

    for keeper in [&keeper, &replaced] {
        let refused = asked(
            keeper,
            &station.desk(),
            &station.lifecycle(),
            false,
            Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(refused.code, Code::Update);
        assert_eq!(refused.error, words);

        let brought = station
            .held(keeper, false)
            .unwrap()
            .brought(&station.lifecycle(), (false, false), Duration::ZERO)
            .unwrap();
        assert_eq!(brought[0]["error"], words, "{}", brought[0]);
        assert_eq!(brought[0]["desktop"], "work");
        assert_eq!(keeper.updates_asked(), 0);
    }
    assert_eq!(station.stop_why(), None, "no Stop kept for a VM not asked");
}

#[test]
fn the_stations_step_asks_again_while_the_keeper_waits_for_its_guest_to_confirm_the_updates_stop() {
    let station = Station::new();
    let keeper = Keeper::of_guest(&older());
    let not_yet = json!({"ok": false, "code": "stopped",
        "error": "Agents on work are not stopped for an update, so its Atlas is not updated.",
        "next": "Run atlas update to stop them for it."});
    *keeper.answers.lock().unwrap() = VecDeque::from([
        not_yet.clone(),
        not_yet.clone(),
        json!({"ok": true, "op": "update-guest", "state": "updating"}),
    ]);
    let keeper = Arc::new(keeper);
    station
        .controller()
        .note_stop_unless_noted(StopWhy::Update)
        .unwrap();

    let answered = asked(
        &keeper,
        &station.desk(),
        &station.lifecycle(),
        false,
        Duration::from_secs(10),
    )
    .unwrap();

    assert_eq!(answered["state"], "updating");
    assert_eq!(answered["desktop"], "work");
    assert_eq!(keeper.updates_asked(), 3);

    // Under another Stop, the person's own, it is asked once and refused in
    // the keeper's words.
    let station = Station::new();
    let keeper = Keeper::of_guest(&older());
    *keeper.answers.lock().unwrap() = VecDeque::from([not_yet]);
    let keeper = Arc::new(keeper);
    station.controller().note_stop(true).unwrap();

    let refused = asked(
        &keeper,
        &station.desk(),
        &station.lifecycle(),
        false,
        Duration::from_secs(10),
    )
    .unwrap_err();

    assert_eq!(refused.code, Code::Stopped);
    assert_eq!(
        refused.error,
        "Agents on work are not stopped for an update, so its Atlas is not updated."
    );
    assert_eq!(
        refused.details["next"],
        "Run atlas update to stop them for it."
    );
    assert_eq!(refused.details["desktop"], "work");
    assert_eq!(keeper.updates_asked(), 1);
}

#[test]
fn the_stations_update_is_refused_while_a_maintenance_is_kept_in_words_saying_to_end_it_first() {
    use crate::vm::lifecycle::record::Phase;
    for (phase, words, next) in [
        (
            Phase::Open,
            "work is under maintenance, so its Atlas is not updated.",
            "End it with atlas desktop maintain work --end, then run atlas update.",
        ),
        (
            Phase::Closing,
            "work's maintenance is still ending, so its Atlas is not updated.",
            "Run atlas desktop maintain work --end again, then run atlas update.",
        ),
    ] {
        let station = Station::new();
        crate::test_support::under_maintenance(station.folder.path(), &vm(), phase);
        let keeper = Arc::new(Keeper::of_guest(&older()));

        let refused = station.held(&keeper, true).unwrap_err();
        let step = asked(
            &keeper,
            &station.desk(),
            &station.lifecycle(),
            false,
            Duration::ZERO,
        )
        .unwrap_err();

        for refused in [refused, step] {
            assert_eq!(refused.code, Code::Busy);
            assert_eq!(refused.error, words);
            assert_eq!(refused.details["next"], next);
        }
        assert!(keeper.asked.lock().unwrap().is_empty(), "nothing asked");
        assert_eq!(station.stop_why(), None);
    }
}

#[test]
fn a_second_update_is_refused_in_plain_words_while_a_guests_update_is_asked() {
    let station = Station::new();
    station.installing();
    let keeper = Arc::new(Keeper::of_guest(&older()));

    let refused = station.held(&keeper, true).unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "work's Atlas is being updated now, so Atlas was not updated."
    );
    assert_eq!(
        refused.details["next"],
        "Run atlas update again once atlas status --desktop work no longer says it is updating."
    );
    assert!(keeper.asked.lock().unwrap().is_empty(), "nothing asked");

    // One whose record cannot be read may be asked too.
    std::fs::write(station.station().join("guest-update.json"), b"{").unwrap();
    let refused = station.held(&keeper, true).unwrap_err();
    assert_eq!(refused.code, Code::State);
    assert_eq!(
        refused.error,
        "Whether work's Atlas is being updated cannot be read, so Atlas was not updated."
    );
}

#[test]
fn nothing_the_station_says_of_a_vms_update_says_helper() {
    let station = Station::new();
    let keeper = Keeper::of_guest(&older());
    keeper.hello.lock().unwrap()["keeper"]["capabilities"] = json!([]);
    let keeper = Arc::new(keeper);
    let older_keeper = asked(
        &keeper,
        &station.desk(),
        &station.lifecycle(),
        false,
        Duration::ZERO,
    )
    .unwrap_err();
    station.installing();
    let installing = station.held(&keeper, true).unwrap_err();

    for said in [json!(older_keeper), json!(installing)] {
        assert!(
            !said.to_string().to_lowercase().contains("helper"),
            "{said}"
        );
    }
}

#[test]
fn a_vm_whose_stop_cannot_be_noted_says_so_and_every_vm_stopped_before_it_is_still_asked() {
    use std::os::unix::fs::PermissionsExt;
    let (work, play) = (Station::new(), Station::new());
    let keeper = Arc::new(Keeper::of_guest(&older()));
    let held = hold(
        &work.lifecycle(),
        Box::new(keeper.clone()),
        vec![
            work.desk(),
            Desk::at("play", vm(), play.controller(), play.station()),
        ],
        false,
    )
    .unwrap();
    // The second VM's Stop marker cannot be written once it is held.
    let state = play.folder.path().join("state");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();

    let brought = held
        .brought(&work.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(brought.len(), 2, "{brought:?}");
    assert_eq!(
        brought[0],
        json!({"ok": true, "op": "update-guest", "state": "updating", "desktop": "work"})
    );
    assert_eq!(brought[1]["ok"], false, "{}", brought[1]);
    assert_eq!(brought[1]["desktop"], "play");
    assert_eq!(work.stop_why(), Some(StopWhy::Update));
    assert_eq!(play.stop_why(), None);
    assert_eq!(keeper.updates_asked(), 1, "the VM stopped is asked");
}

#[test]
fn an_update_of_this_build_that_failed_never_stops_agents_again_unless_the_person_asks_to_retry() {
    let station = Station::new();
    let outcome = station.rolled_back();
    let keeper = Arc::new(Keeper::of_guest(&older()));

    let brought = station
        .held(&keeper, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();

    assert_eq!(
        brought,
        vec![json!({"ok": true, "desktop": "work", "state": "refused",
            "why": "The new Atlas did not start, so the old one is back.",
            "outcome": outcome, "next": "Run atlas update --retry-vms to try again."})]
    );
    assert_eq!(station.stop_why(), None, "its agents never stopped");
    assert_eq!(keeper.updates_asked(), 0);

    let brought = station
        .held(&keeper, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, true), Duration::ZERO)
        .unwrap();

    assert_eq!(brought[0]["state"], "updating", "{}", brought[0]);
    assert_eq!(station.stop_why(), Some(StopWhy::Update));
    let asked = keeper.asked.lock().unwrap();
    let retried: Vec<&Value> = asked
        .iter()
        .filter(|asked| asked["op"] == "update-guest")
        .collect();
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0]["retry"], true);
}

#[test]
fn a_guest_missing_any_capability_the_keepers_update_needs_or_of_another_protocol_is_never_stopped()
{
    use atlas_session::compatibility::{CHANGES, COPIES, STAGE};
    for missing in [COPIES, STAGE, CHANGES] {
        let station = Station::new();
        let keeper = Keeper::of_guest(&older());
        let declared: Vec<&str> = MACHINE
            .into_iter()
            .filter(|capability| *capability != missing)
            .collect();
        keeper.hello.lock().unwrap()["guest"]["capabilities"] = json!(declared);
        let keeper = Arc::new(keeper);

        let brought = station
            .held(&keeper, false)
            .unwrap()
            .brought(&station.lifecycle(), (false, false), Duration::ZERO)
            .unwrap();

        assert_eq!(
            brought[0]["next"],
            "work's Atlas cannot be updated from this Mac. Remove the VM and add it again.",
            "{missing}"
        );
        assert_eq!(station.stop_why(), None, "{missing}");
        assert_eq!(keeper.updates_asked(), 0, "{missing}");
    }

    let station = Station::new();
    let keeper = Keeper::of_guest(&older());
    keeper.hello.lock().unwrap()["guest"]["protocol"] = json!(2);
    let keeper = Arc::new(keeper);
    let brought = station
        .held(&keeper, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();
    assert_eq!(brought[0]["ok"], false, "{}", brought[0]);
    assert_eq!(brought[0]["desktop"], "work");
    assert_eq!(station.stop_why(), None);
    assert_eq!(keeper.updates_asked(), 0);
}

#[test]
fn an_update_installing_now_is_refused_in_its_own_words_over_its_pending_claim() {
    let station = Station::new();
    let update = station.installing();
    drop(
        station
            .controller()
            .quarantine_by(update.as_str(), atlas_control::executors::Executor::Guest)
            .unwrap(),
    );
    let keeper = Arc::new(Keeper::of_guest(&older()));

    let refused = station.held(&keeper, true).unwrap_err();

    assert_eq!(refused.code, Code::Busy);
    assert_eq!(
        refused.error,
        "work's Atlas is being updated now, so Atlas was not updated."
    );
    assert!(keeper.asked.lock().unwrap().is_empty(), "nothing asked");
    assert_eq!(
        station.controller().pending().unwrap(),
        vec![update.as_str().to_owned()],
        "its claim kept"
    );
}

#[test]
fn an_update_never_tried_in_the_guest_is_brought_again_by_a_plain_update_and_a_release_elsewhere_is_not()
 {
    for never in [
        json!({"ok": false, "op": "update", "update": "notAsked",
            "why": "Agents resumed before the update was asked."}),
        json!({"ok": false, "op": "update", "update": "notStaged",
            "why": "The guest never took it."}),
    ] {
        let station = Station::new();
        station.done(never.clone());
        let keeper = Arc::new(Keeper::of_guest(&older()));

        let brought = station
            .held(&keeper, false)
            .unwrap()
            .brought(&station.lifecycle(), (false, false), Duration::ZERO)
            .unwrap();

        assert_eq!(brought[0]["state"], "updating", "{never}: {}", brought[0]);
        assert_eq!(station.stop_why(), Some(StopWhy::Update), "{never}");
        let asked = keeper.asked.lock().unwrap();
        let updates: Vec<&Value> = asked
            .iter()
            .filter(|asked| asked["op"] == "update-guest")
            .collect();
        assert_eq!(updates.len(), 1, "{never}");
        assert_eq!(updates[0]["retry"], false, "{never}");
    }

    // Released, and the guest still not on this build: released elsewhere.
    let station = Station::new();
    let released = station.done(json!({"ok": true, "op": "update", "update": "released"}));
    let keeper = Arc::new(Keeper::of_guest(&older()));
    let brought = station
        .held(&keeper, false)
        .unwrap()
        .brought(&station.lifecycle(), (false, false), Duration::ZERO)
        .unwrap();
    assert_eq!(
        brought,
        vec![json!({"ok": true, "desktop": "work", "state": "refused",
            "why": "work's Atlas was updated, but it does not say it runs this Mac's build.",
            "outcome": released, "next": "Run atlas update --retry-vms to try again."})]
    );
    assert_eq!(station.stop_why(), None);
    assert_eq!(keeper.updates_asked(), 0);
}
