use super::*;
use crate::vm::lifecycle::{Provenance, Reservation};
use atlas_core::config::write::Change;
use std::path::PathBuf;

const WORK: &str = "0123456789abcdef01234567";
const SPARE: &str = "89abcdef0123456789abcdef";

/// Atlas VM and the keepers, as a test says they stand.
struct Seen {
    power: Option<(&'static str, bool)>,
    reachable: bool,
}

impl Looks for Seen {
    fn power(&self, _vm: &DesktopId) -> Result<Reply> {
        let Some((power, confirmed)) = self.power else {
            return Err(Error::new(
                atlas_core::error::Code::Transport,
                "The VM's runner did not answer.",
            ));
        };
        Ok(serde_json::from_value(
            json!({"ok": true, "op": "state", "power": power, "confirmed": confirmed}),
        )
        .unwrap())
    }

    fn reach(&self, _selected: &Selected, _vm: &DesktopId) -> Value {
        if self.reachable {
            json!({"reachable": true})
        } else {
            json!({"reachable": false, "why": "work's session keeper is not running"})
        }
    }
}

fn id(text: &str) -> DesktopId {
    DesktopId::parse(text).unwrap()
}

fn record(lifecycle: &Lifecycle, name: &str, vm: &str, state: State) {
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
        list_with(&lifecycle, &Seen { power, reachable }, &settings).unwrap()["desktops"][0].clone()
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
    let seen = Seen {
        power: Some(("running", true)),
        reachable: true,
    };

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
    let seen = Seen {
        power: None,
        reachable: false,
    };

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
    let seen = Seen {
        power: None,
        reachable: false,
    };

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

/// Atlas VM, as the listing asks the runner of each VM in `vms`.
struct Asking(crate::vm::Vms);

impl Looks for Asking {
    fn power(&self, vm: &DesktopId) -> Result<Reply> {
        Runner::in_vms(&self.0, vm).read(&Operation::State)
    }

    fn reach(&self, _selected: &Selected, _vm: &DesktopId) -> Value {
        json!({"reachable": false})
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
