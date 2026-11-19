use super::*;
use crate::vm::lifecycle::{Provenance, Reservation, record::Step};
use atlas_core::config::write::Change;
use atlas_core::{files::Token, process::Instance};
use std::path::PathBuf;

const WORK: &str = "0123456789abcdef01234567";
const SPARE: &str = "89abcdef0123456789abcdef";

/// Atlas VM and the keepers, as a test says they stand.
struct Seen {
    power: Option<(&'static str, bool)>,
    reachable: bool,
    /// What Atlas VM declares beyond its power.
    declares: Vec<&'static str>,
    /// What its state says beyond the power.
    state: Value,
    /// The newest macOS it names.
    latest: Value,
    /// The keeper's hello.
    keeper: Value,
}

impl Seen {
    fn new(power: Option<(&'static str, bool)>, reachable: bool) -> Self {
        Self {
            power,
            reachable,
            declares: Vec::new(),
            state: json!({}),
            latest: Value::Null,
            keeper: Value::Null,
        }
    }
}

impl Looks for Seen {
    fn ask(&self, _vm: &DesktopId, operation: &Operation) -> Result<Reply> {
        let Some((power, confirmed)) = self.power else {
            return Err(Error::new(
                atlas_core::error::Code::Transport,
                "The VM's runner did not answer.",
            ));
        };
        let mut reply = json!({"ok": true, "op": operation.name()});
        match operation {
            Operation::Hello => {
                let mut capabilities = vec!["window-input", "void"];
                capabilities.extend(&self.declares);
                reply["hello"] = json!({"version": 1, "capabilities": capabilities});
            }
            Operation::State => {
                reply["power"] = json!(power);
                reply["confirmed"] = json!(confirmed);
                for (key, value) in self.state.as_object().into_iter().flatten() {
                    reply[key] = value.clone();
                }
            }
            Operation::Latest => reply["latest"] = self.latest.clone(),
            _ => panic!("the listing asks {} of Atlas VM", operation.name()),
        }
        Ok(serde_json::from_value(reply).unwrap())
    }

    fn reach(&self, _selected: &Selected, _vm: &DesktopId) -> (Value, Value) {
        let reach = if self.reachable {
            json!({"reachable": true})
        } else {
            json!({"reachable": false, "why": "work's session keeper is not running"})
        };
        (reach, self.keeper.clone())
    }
}

fn id(text: &str) -> DesktopId {
    DesktopId::parse(text).unwrap()
}

fn record(lifecycle: &Lifecycle, name: &str, vm: &str, state: State) -> Record {
    let reservation = Reservation {
        name: name.to_owned(),
        id: id(vm),
        named: true,
        resumed: false,
    };
    let mut record = Record::adding(
        &reservation,
        PathBuf::from("/Users/someone/.config/atlas.toml"),
        Provenance {
            program: PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas"),
            requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
        },
    );
    record.state = state;
    lifecycle.keep_record(&record).unwrap();
    record
}

fn listed(home: &Path, name: &str, vm: &str) -> PathBuf {
    let settings = home.join("atlas.toml");
    Change::AddVm {
        name: name.to_owned(),
        id: id(vm),
    }
    .apply(&settings)
    .unwrap();
    settings
}

#[test]
fn a_vms_power_is_said_apart_from_whether_its_keeper_reaches_it() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let settings = listed(home.path(), "work", WORK);
    record(&lifecycle, "work", WORK, State::Created);
    let look = |power, reachable| {
        list_with(&lifecycle, &Seen::new(power, reachable), &settings).unwrap()["desktops"][0]
            .clone()
    };

    let running = look(Some(("running", true)), false);
    let known = look(Some(("stopped", false)), true);
    let silent = look(None, true);

    assert_eq!(running["kind"], "vm");
    assert_eq!(running["lifecycle"], "created");
    assert_eq!(running["default"], true);
    assert_eq!(
        running["power"],
        json!({"state": "running", "confirmed": true})
    );
    assert_eq!(running["reach"]["reachable"], false);
    assert_eq!(
        known["power"],
        json!({"state": "stopped", "confirmed": false})
    );
    assert_eq!(known["reach"]["reachable"], true);
    assert_eq!(silent["power"]["state"], "unknown");
    assert_eq!(silent["reach"]["reachable"], true);
}

#[test]
fn a_vm_being_added_is_shown_as_adding_whether_or_not_the_settings_list_it_yet() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let settings = listed(home.path(), "work", WORK);
    record(&lifecycle, "work", WORK, State::Adding);
    record(&lifecycle, "spare", SPARE, State::Adding);
    let seen = Seen::new(Some(("running", true)), true);

    let listing = list_with(&lifecycle, &seen, &settings).unwrap();

    let desktops = listing["desktops"].as_array().unwrap();
    assert_eq!(desktops.len(), 2);
    let work = &desktops[0];
    assert_eq!(work["name"], "work");
    assert_eq!(work["lifecycle"], "adding");
    assert_eq!(work["listed"], true);
    assert_eq!(work["reach"]["reachable"], false, "only Stop reaches it");
    let why = work["reach"]["why"].as_str().unwrap();
    assert!(why.contains("still being added"), "{why}");
    let spare = &desktops[1];
    assert_eq!(spare["name"], "spare");
    assert_eq!(spare["lifecycle"], "adding");
    assert_eq!(spare["listed"], false);
    assert_eq!(spare["power"]["state"], "running");
}

#[test]
fn a_listing_with_no_settings_and_no_vm_shows_none_and_makes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let seen = Seen::new(None, false);

    let listing = list_with(&lifecycle, &seen, &home.path().join("atlas.toml")).unwrap();

    assert_eq!(listing["desktops"], json!([]));
    assert!(!lifecycle.kept_any().unwrap());
    assert!(!home.path().join("atlas.toml").exists());
}

#[test]
fn a_lifecycle_folder_out_of_reach_is_said_never_taken_for_no_vms() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    record(&lifecycle, "work", WORK, State::Adding);
    let station = home
        .path()
        .join("Library/Application Support/Atlas/Station");
    let seen = Seen::new(None, false);

    std::fs::set_permissions(&station, std::fs::Permissions::from_mode(0o600)).unwrap();
    let refused = list_with(&lifecycle, &seen, &home.path().join("atlas.toml"));
    std::fs::set_permissions(&station, std::fs::Permissions::from_mode(0o700)).unwrap();

    let refused = refused.unwrap_err();
    assert!(
        refused.error.contains("cannot be read"),
        "{}",
        refused.error
    );
    let listing = list_with(&lifecycle, &seen, &home.path().join("atlas.toml")).unwrap();
    assert_eq!(listing["desktops"][0]["lifecycle"], "adding");
}

/// The VM `work`, listed and being added, its make asked of Atlas VM as
/// `create`, kept in `lifecycle` with what `change` does to its record.
fn making(
    lifecycle: &Lifecycle,
    home: &Path,
    create: &Token,
    change: impl FnOnce(&mut Record),
) -> PathBuf {
    let settings = listed(home, "work", WORK);
    let mut kept = record(lifecycle, "work", WORK, State::Adding);
    kept.done(Step::Agent);
    kept.sending(&Operation::Create(create.clone())).unwrap();
    change(&mut kept);
    lifecycle.keep_record(&kept).unwrap();
    settings
}

/// What Atlas VM keeps of the make `create`, a download under way.
fn downloading(create: &Token) -> Value {
    json!({"making": {"change": create, "phase": "downloading", "received": 12_400_000_000_u64,
        "total": 25_600_000_000_u64, "fraction": 0.484_375}})
}

#[test]
fn a_vms_download_progress_is_listed_from_atlas_vms_word() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let create = Token::new();
    let settings = making(&lifecycle, home.path(), &create, |_| {});
    let mut seen = Seen::new(Some(("absent", true)), false);
    seen.declares = vec!["progress"];
    seen.state = downloading(&create);

    let listed = list_with(&lifecycle, &seen, &settings).unwrap();
    seen.state = json!({"making": {"change": create, "phase": "installing", "fraction": 0.25}});
    let installing = list_with(&lifecycle, &seen, &settings).unwrap();
    seen.state = downloading(&Token::new());
    let another = list_with(&lifecycle, &seen, &settings).unwrap();

    let work = &listed["desktops"][0];
    assert_eq!(
        work["progress"],
        json!({"phase": "downloading", "received": 12_400_000_000_u64,
            "total": 25_600_000_000_u64, "fraction": 0.484_375})
    );
    assert_eq!(work["steps"], json!(["agent"]));
    assert_eq!(
        installing["desktops"][0]["progress"],
        json!({"phase": "installing", "received": null, "total": null, "fraction": 0.25})
    );
    assert_eq!(
        another["desktops"][0]["progress"],
        Value::Null,
        "another make's"
    );
}

#[test]
fn a_runner_that_declares_no_progress_lists_none_and_never_a_guess() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let create = Token::new();
    let settings = making(&lifecycle, home.path(), &create, |record| {
        record.done(Step::Created);
    });
    let mut undeclared = Seen::new(Some(("stopped", true)), false);
    undeclared.state = downloading(&create);
    let silent = Seen::new(None, false);

    let undeclared = list_with(&lifecycle, &undeclared, &settings).unwrap();
    let silent = list_with(&lifecycle, &silent, &settings).unwrap();

    for listing in [&undeclared, &silent] {
        let work = &listing["desktops"][0];
        assert_eq!(work["progress"], Value::Null, "{work}");
        assert_eq!(work["disk"], Value::Null, "{work}");
        assert_eq!(work["macos"]["latest"], Value::Null, "{work}");
        assert_eq!(work["macos"]["newer"], Value::Null, "{work}");
        assert_eq!(work["steps"], json!(["agent", "created"]), "{work}");
    }
}

#[test]
fn an_add_whose_process_died_is_listed_as_stopped_not_running() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let create = Token::new();
    let this = Instance::current().unwrap();
    let mut gone = this;
    gone.start[1] = gone.start[1].wrapping_add(1);
    let settings = making(&lifecycle, home.path(), &create, |_| {});
    let seen = Seen::new(Some(("absent", true)), false);
    let adder = |state: State, adder: Option<Instance>| {
        let mut kept = lifecycle.record(&id(WORK)).unwrap().unwrap();
        kept.state = state;
        kept.adder = adder;
        lifecycle.keep_record(&kept).unwrap();
        list_with(&lifecycle, &seen, &settings).unwrap()["desktops"][0]["adder"].clone()
    };

    assert_eq!(adder(State::Adding, Some(gone)), json!({"running": false}));
    assert_eq!(adder(State::Adding, Some(this)), json!({"running": true}));
    assert_eq!(
        adder(State::Adding, None),
        json!({"running": null}),
        "an add's from before it was kept"
    );
    assert_eq!(adder(State::Created, Some(gone)), Value::Null, "added");
}

#[test]
fn a_listed_vm_says_its_macos_and_its_atlas_from_its_keepers_report() {
    let home = tempfile::tempdir().unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let settings = listed(home.path(), "work", WORK);
    record(&lifecycle, "work", WORK, State::Created);
    let mut seen = Seen::new(Some(("running", true)), true);
    seen.declares = vec!["latest-macos", "disk"];
    seen.latest = json!({"version": "27.3", "build": "27D50"});
    seen.state = json!({"disk": {"used": 28_000_000_000_u64, "size": 128_000_000_000_u64}});
    seen.keeper = json!({"live": true, "report": {"system": {"version": "27.2.1", "build": "27C101"}},
        "guest": {"version": "0.1.3", "build": null}});

    let work = list_with(&lifecycle, &seen, &settings).unwrap()["desktops"][0].clone();

    assert_eq!(
        work["macos"],
        json!({"version": "27.2.1", "build": "27C101",
            "latest": {"version": "27.3", "build": "27D50"}, "newer": true})
    );
    assert_eq!(
        work["disk"],
        json!({"used": 28_000_000_000_u64, "size": 128_000_000_000_u64})
    );
    assert!(work["atlas"]["state"].is_string(), "{work}");
    assert_eq!(work["maintenance"]["state"], "none");
    assert_eq!(work["can"], json!(["stop", "maintain", "remove"]));
}

/// Atlas VM, as the listing asks the runner of each VM in `vms`.
struct Asking(crate::vm::Vms);

impl Looks for Asking {
    fn ask(&self, vm: &DesktopId, operation: &Operation) -> Result<Reply> {
        Runner::in_vms(&self.0, vm).read(operation)
    }

    fn reach(&self, _selected: &Selected, _vm: &DesktopId) -> (Value, Value) {
        (json!({"reachable": false}), Value::Null)
    }
}

#[test]
fn a_listing_reads_a_stopped_vms_power_from_atlas_vm_and_never_starts_it() {
    let home = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
    let lifecycle = Lifecycle::under(home.path());
    let settings = listed(home.path(), "work", WORK);
    record(&lifecycle, "work", WORK, State::Created);
    let runner = crate::test_support::StandInRunner::at(
        &crate::vm::runner::socket_in(home.path(), &id(WORK)),
        "stopped",
    );

    let listing = list_with(&lifecycle, &Asking(lifecycle.vms()), &settings).unwrap();

    assert_eq!(
        listing["desktops"][0]["power"],
        json!({"state": "stopped", "confirmed": true})
    );
    assert_eq!(runner.asked_for("state").len(), 1);
    assert!(runner.asked_for("start").is_empty());
    assert_eq!(runner.power(), "stopped");
}
