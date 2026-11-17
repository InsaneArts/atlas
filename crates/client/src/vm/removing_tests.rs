use super::*;
use crate::{
    station::Station,
    vm::{Vms, lifecycle::Reservation, removal::removed, tests::Fake},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
};

pub(super) const WORK: &str = "a7a5000000000000000000f1";
pub(super) const SPARE: &str = "b7b5000000000000000000f2";
pub(super) const THIRD: &str = "c7c5000000000000000000f3";

pub(super) fn id(text: &str) -> DesktopId {
    DesktopId::parse(text).unwrap()
}

/// The change every grant here is for.
pub(super) fn change() -> Token {
    Token::parse(&"c".repeat(48)).unwrap()
}

pub(super) const SECOND_LOGIN: &str = "[[desktops]]\nname = \"second-login\"\nkind = \"user\"\n\
                            helper_uid = 60001\nsocket_path = \"c.sock\"\n\
                            identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";

pub(super) fn vm_entry(name: &str, vm: &str) -> String {
    format!("\n[[desktops]]\nname = \"{name}\"\nkind = \"vm\"\nid = \"{vm}\"\n")
}

pub(super) fn provenance() -> Provenance {
    Provenance {
        program: PathBuf::from("/Applications/Atlas.app/Contents/MacOS/atlas"),
        requirement: "identifier \"com.insanearts.atlas\"".to_owned(),
    }
}

/// What happens while a removal waits.
pub(super) type Meanwhile = Box<dyn FnOnce(&World)>;

/// The Mac one removal meets: a home of its own, its VMs, Atlas VM and
/// launchd as the station's stand-in gives them, and the person.
pub(super) struct World {
    pub(super) home: tempfile::TempDir,
    pub(super) vms: Vms,
    pub(super) fake: Fake,
    pub(super) allows: Cell<bool>,
    pub(super) grants: Cell<u32>,
    pub(super) removes: Cell<usize>,
    /// The removal call, counting from one, at which the command is
    /// killed, as `kill` would: every later call fails.
    pub(super) kill_at: Cell<Option<usize>>,
    /// What happens while the command waits the first time.
    pub(super) meanwhile: RefCell<Option<Meanwhile>>,
    /// The VM's lifecycle record as each removal call found it.
    pub(super) seen: RefCell<Vec<Option<Record>>>,
    /// What the removal answers in place of the station's own, when given.
    pub(super) answer: RefCell<Option<Value>>,
    /// Every operation the person was asked to allow.
    pub(super) operations: RefCell<Vec<grant::Operation>>,
    /// What happens while the person is asked.
    pub(super) while_asked: RefCell<Option<Meanwhile>>,
}

impl World {
    pub(super) fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let vms = Vms::under(home.path(), nix::unistd::getuid().as_raw());
        std::fs::create_dir_all(&vms.parent).unwrap();
        Self {
            home,
            vms,
            fake: Fake::new(),
            allows: Cell::new(true),
            grants: Cell::new(0),
            removes: Cell::new(0),
            kill_at: Cell::new(None),
            meanwhile: RefCell::new(None),
            seen: RefCell::new(Vec::new()),
            answer: RefCell::new(None),
            operations: RefCell::new(Vec::new()),
            while_asked: RefCell::new(None),
        }
    }

    pub(super) fn lifecycle(&self) -> Lifecycle {
        Lifecycle::under(self.home.path())
    }

    pub(super) fn settings(&self) -> PathBuf {
        self.home.path().join("atlas.toml")
    }

    pub(super) fn write_settings(&self, text: &str) {
        std::fs::write(self.settings(), text).unwrap();
    }

    pub(super) fn read_settings(&self) -> String {
        std::fs::read_to_string(self.settings()).unwrap()
    }

    /// The VM `vm` as an add leaves it: named `name`, its record in
    /// `state`, and, when `made`, its folder and its station folder.
    pub(super) fn vm(&self, name: &str, vm: &str, state: State, made: bool) {
        let reservation = Reservation {
            name: name.to_owned(),
            id: id(vm),
            named: true,
            resumed: false,
        };
        let lifecycle = self.lifecycle();
        lifecycle.claim(&reservation).unwrap();
        let settings = atlas_core::config::resolve_path(&self.settings()).unwrap();
        let mut record = Record::adding(&reservation, settings, provenance());
        record.state = state;
        lifecycle.keep_record(&record).unwrap();
        if made {
            let folder = self.vms.folder(&id(vm));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("Disk.asif"), "disk").unwrap();
            let process =
                |pid: i32| json!({"pid": pid, "startSeconds": 100, "startMicroseconds": 0});
            let runs = json!({"run": process(41), "earlier": [], "machine": process(42)});
            atlas_core::files::write_private(
                &folder.join("Incarnation.json"),
                runs.to_string().as_bytes(),
            )
            .unwrap();
            let station = Station::under(self.home.path()).folder(&id(vm)).unwrap();
            std::fs::write(station.join("pairing.json"), "{}").unwrap();
        }
    }

    pub(super) fn station(&self, vm: &str) -> PathBuf {
        Station::under(self.home.path()).path_of(&id(vm))
    }

    /// Its runner answering `forget` as done.
    pub(super) fn runner_forgets(&self) {
        *self.fake.reply.lock().unwrap() = Some(json!({"ok": true, "outcome": {"state": "done"}}));
    }

    /// Atlas VM's deletion-only entry, when run, deleting `vm`'s passwords.
    pub(super) fn entry_deletes(&self, vm: &str) {
        let result = json!({"change": change(), "passwords": {
            vm: "deleted", format!("{vm}.admin"): "deleted"}});
        *self.fake.entry.lock().unwrap() = Some((self.vms.forgotten(&id(vm)), result));
    }

    /// Ask for the removal of `name`, as the person would; what it said is
    /// kept.
    pub(super) fn remove(&self, name: &str) -> (Result<Value>, Vec<String>) {
        self.remove_which(Which::Name(name))
    }

    /// Ask for the removal of the VM `which` names.
    pub(super) fn remove_which(&self, which: Which<'_>) -> (Result<Value>, Vec<String>) {
        self.removes.set(0);
        let said = RefCell::new(Vec::new());
        let settings = self.settings();
        let asked = Asked {
            which,
            settings: &settings,
        };
        let removed = remove_with(&self.lifecycle(), self, asked, &|line| {
            said.borrow_mut().push(line.to_owned());
        });
        (removed, said.into_inner())
    }

    pub(super) fn record(&self, vm: &str) -> Option<Record> {
        self.lifecycle().record(&id(vm)).unwrap()
    }

    pub(super) fn names(&self) -> Vec<String> {
        let folder = self
            .home
            .path()
            .join("Library/Application Support/Atlas/Station/lifecycle/names");
        let mut names: Vec<String> = std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Means for World {
    fn grant(&self, operation: &grant::Operation) -> Result<Token> {
        self.operations.borrow_mut().push(operation.clone());
        let meanwhile = self.while_asked.borrow_mut().take();
        if let Some(meanwhile) = meanwhile {
            meanwhile(self);
        }
        if !self.allows.get() {
            return Err(Error::new(
                Code::Denied,
                "The person did not allow it in Atlas.app; nothing was done.",
            ));
        }
        self.grants.set(self.grants.get() + 1);
        Ok(change())
    }

    fn provenance(&self) -> Result<Provenance> {
        Ok(provenance())
    }

    fn hold<'a>(&self, vm: &DesktopId, passed: &'a impl Passed) -> Result<Option<StationHold<'a>>> {
        crate::vm::hold_in(&self.vms, vm, passed)
    }

    fn remove(&self, hold: &StationHold<'_>, change: &Token) -> Result<Value> {
        let at = self.removes.get() + 1;
        self.removes.set(at);
        if self.kill_at.get().is_some_and(|kill| at >= kill) {
            return Err(Error::new(Code::Interrupted, "Killed."));
        }
        self.seen
            .borrow_mut()
            .push(self.lifecycle().record(hold.desktop()).unwrap());
        if let Some(answer) = self.answer.borrow().clone() {
            return Ok(answer);
        }
        removed(&self.vms, &self.fake, hold, change)
    }

    fn wait(&self, _how_long: Duration) {
        let meanwhile = self.meanwhile.borrow_mut().take();
        if let Some(meanwhile) = meanwhile {
            meanwhile(self);
        }
    }
}

#[test]
fn nothing_is_touched_without_the_persons_grant() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.allows.set(false);
    let before = world.read_settings();

    let (refused, _) = world.remove("work");

    assert_eq!(refused.unwrap_err().code, Code::Denied);
    assert_eq!(world.record(WORK).unwrap().state, State::Created);
    assert_eq!(world.record(WORK).unwrap().removal, None);
    assert_eq!(world.read_settings(), before);
    assert_eq!(world.names(), ["work"]);
    assert!(world.vms.folder(&id(WORK)).join("Disk.asif").exists());
    assert!(!world.vms.removal(&id(WORK)).exists());
    assert_eq!(world.removes.get(), 0);
}

#[test]
fn the_last_desktop_is_refused_with_advice_before_the_person_is_asked_or_anything_touched() {
    let world = World::new();
    world.write_settings(&format!("default = \"work\"\n{}", vm_entry("work", WORK)));
    world.vm("work", WORK, State::Created, true);
    let before = world.read_settings();

    let (refused, _) = world.remove("work");

    let refused = refused.unwrap_err();
    assert_eq!(refused.code, Code::Config);
    assert!(
        refused.error.contains("atlas uninstall"),
        "{}",
        refused.error
    );
    assert_eq!(world.grants.get(), 0);
    assert_eq!(world.read_settings(), before);
    assert_eq!(world.record(WORK).unwrap().state, State::Created);
    assert_eq!(world.removes.get(), 0);
}

#[test]
fn a_default_vm_removed_hands_the_default_to_second_login_else_to_the_first_desktop_left() {
    for (listed, becomes) in [
        (
            format!(
                "default = \"work\"\n{SECOND_LOGIN}{}{}",
                vm_entry("work", WORK),
                vm_entry("spare", SPARE)
            ),
            "second-login",
        ),
        (
            format!(
                "default = \"work\"\n{}{}",
                vm_entry("work", WORK),
                vm_entry("spare", SPARE)
            ),
            "spare",
        ),
    ] {
        let world = World::new();
        world.write_settings(&listed);
        world.vm("work", WORK, State::Created, true);
        world.runner_forgets();

        let (removed, said) = world.remove("work");

        let removed = removed.unwrap();
        assert_eq!(removed["default"], becomes, "{removed}");
        let after = Settings::load(&world.settings()).unwrap();
        assert_eq!(after.default_desktop().name, becomes);
        assert!(after.desktops().iter().all(|listed| listed.name != "work"));
        assert!(
            said.iter()
                .any(|line| line.contains(&format!("default desktop is now \u{201c}{becomes}"))),
            "{said:?}"
        );
    }
}

#[test]
fn a_created_vm_is_removed_whole_then_its_settings_entry_its_name_and_its_record_last() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, false);
    world.runner_forgets();

    let (removed, said) = world.remove("work");

    let removed = removed.unwrap();
    assert_eq!(removed["removed"]["id"], WORK, "{removed}");
    assert_eq!(removed["kept"], json!([]));
    assert_eq!(removed["left"], json!([]));
    let seen = world.seen.borrow();
    let before = seen[0].as_ref().unwrap();
    assert_eq!(before.state, State::Removing, "kept before the removal ran");
    assert_eq!(before.removal, Some(change()));
    assert!(!world.vms.folder(&id(WORK)).exists());
    assert!(!world.station(WORK).exists());
    assert!(!world.read_settings().contains(WORK));
    assert_eq!(world.names(), ["spare"]);
    assert_eq!(world.record(WORK), None);
    assert_eq!(world.record(SPARE).unwrap().state, State::Created);
    assert!(
        said[0].starts_with("Asking you in Atlas.app to allow removing"),
        "{said:?}"
    );
}

#[test]
fn a_half_made_vm_never_listed_is_removed_the_same_way_with_its_passwords_gone_through_atlas_vm() {
    let world = World::new();
    // An add cut short before its settings entry, and before any file.
    world.vm("work", WORK, State::Adding, false);
    world.entry_deletes(WORK);

    let (removed, _) = world.remove("work");

    let removed = removed.unwrap();
    assert_eq!(removed["removed"]["name"], "work", "{removed}");
    assert_eq!(removed["default"], Value::Null);
    assert_eq!(removed["left"], json!([]), "{removed}");
    assert_eq!(
        world.seen.borrow()[0].as_ref().unwrap().state,
        State::Removing
    );
    assert_eq!(
        world.fake.ran.lock().unwrap().len(),
        1,
        "the runner never answered"
    );
    assert!(!world.settings().exists(), "no settings file is made");
    assert!(world.names().is_empty());
    assert_eq!(world.record(WORK), None);
}

#[test]
fn the_record_outlives_the_station_folder_and_the_settings_entry_until_the_removal_is_done() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    // A late bootstrap of the keeper's keeps the first answer from done,
    // and the command is killed before it asks again.
    *world.fake.keeper_late.lock().unwrap() = Some((2, atlas_core::launchd::Loaded::default()));
    world.kill_at.set(Some(2));

    let (killed, said) = world.remove("work");

    assert_eq!(killed.unwrap_err().code, Code::Interrupted);
    assert!(said.iter().any(|line| line.contains("keeper")), "{said:?}");
    assert!(!world.station(WORK).exists(), "the station folder went");
    assert!(!world.vms.folder(&id(WORK)).exists());
    let record = world.record(WORK).unwrap();
    assert_eq!(record.state, State::Removing);
    assert_eq!(record.removal, Some(change()));
    assert!(record.failure.is_some());
    assert!(world.read_settings().contains(WORK), "listed until done");
    assert_eq!(world.names(), ["work"]);

    world.kill_at.set(None);
    let (removed, _) = world.remove("work");

    assert!(removed.unwrap()["removed"]["id"] == WORK);
    assert!(!world.read_settings().contains(WORK));
    assert!(world.names().is_empty());
    assert_eq!(world.record(WORK), None);
    assert!(
        world
            .fake
            .asked
            .lock()
            .unwrap()
            .iter()
            .all(|asked| asked["change"] == change().as_str()),
        "one removal, its change kept"
    );
}

#[test]
fn paths_a_removal_kept_keep_the_record_removing_and_are_said_and_never_deleted() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    // A runner holds the VM at first; while the command waits, it goes and
    // another folder takes the VM's place.
    let runner = crate::vm::try_lock(&world.vms.lifecycle(&id(WORK)))
        .unwrap()
        .unwrap();
    let runner = RefCell::new(Some(runner));
    *world.meanwhile.borrow_mut() = Some(Box::new(move |world: &World| {
        runner.borrow_mut().take();
        let folder = world.vms.folder(&id(WORK));
        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("other"), "other").unwrap();
    }));

    let (removed, _) = world.remove("work");

    let removed = removed.unwrap();
    let folder = world.vms.folder(&id(WORK)).to_string_lossy().into_owned();
    assert_eq!(removed["kept"], json!([folder]), "{removed}");
    assert!(
        removed["keptWhy"]
            .as_str()
            .unwrap()
            .contains("never deletes")
    );
    let record = world.record(WORK).unwrap();
    assert_eq!(record.state, State::Removing);
    assert_eq!(record.kept, std::slice::from_ref(&folder));
    assert!(!world.read_settings().contains(WORK));
    assert_eq!(world.names(), ["work"], "its name stays taken");
    let reused = world.lifecycle().prospect(Some("work"), &[]);
    assert!(
        reused.unwrap_err().error.contains("already a VM named"),
        "no other add takes the name"
    );

    let (again, _) = world.remove("work");

    assert_eq!(again.unwrap()["kept"], json!([folder]));
    assert!(world.vms.folder(&id(WORK)).join("other").exists());
    assert_eq!(world.record(WORK).unwrap().state, State::Removing);

    // The person moved the other folder aside; asked by its id, the
    // removal finishes and gives the name up.
    let aside = world.home.path().join("moved aside");
    std::fs::rename(world.vms.folder(&id(WORK)), &aside).unwrap();
    let (finished, _) = world.remove_which(Which::Id(&id(WORK)));

    assert_eq!(finished.unwrap()["kept"], json!([]));
    assert!(aside.join("other").exists(), "never deleted");
    assert_eq!(world.record(WORK), None);
    assert!(world.names().is_empty());
    assert!(world.lifecycle().prospect(Some("work"), &[]).is_ok());
}

#[test]
fn passwords_left_keep_the_record_are_named_and_are_recorded_gone_once_deleted_by_hand() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    // The runner cannot run, and Atlas VM's entry fails too.
    *world.fake.entry_fails.lock().unwrap() = true;
    *world.fake.passwords.lock().unwrap() = vec![WORK.to_owned(), format!("{WORK}.admin")];

    let (left, _) = world.remove("work");

    let left = left.unwrap();
    assert_eq!(
        left["left"],
        json!([
            {"service": "com.insanearts.atlas.vm", "account": WORK},
            {"service": "com.insanearts.atlas.vm", "account": format!("{WORK}.admin")},
        ]),
        "{left}"
    );
    assert!(
        left["leftNext"]
            .as_str()
            .unwrap()
            .contains("Keychain Access")
    );
    assert_eq!(world.fake.ran.lock().unwrap().len(), 1);
    assert_eq!(world.record(WORK).unwrap().state, State::Removing);
    assert!(!world.read_settings().contains(WORK));
    assert_eq!(world.names(), ["work"], "its name stays taken");
    assert!(
        left["leftNext"]
            .as_str()
            .unwrap()
            .contains(&format!("--id {WORK}"))
    );

    // Asked again with both still there: still named, the record kept.
    let (still, _) = world.remove("work");
    assert_eq!(still.unwrap()["left"].as_array().unwrap().len(), 2);
    assert!(world.record(WORK).is_some());

    // The person deleted both in Keychain Access.
    world.fake.passwords.lock().unwrap().clear();
    let (removed, _) = world.remove("work");

    assert_eq!(removed.unwrap()["left"], json!([]));
    assert_eq!(world.record(WORK), None);
    assert!(world.names().is_empty(), "the name is given up once done");
    let recorded = crate::vm::removal::recorded(&world.vms, &id(WORK))
        .unwrap()
        .unwrap();
    assert_eq!(recorded.passwords[WORK], "gone");
    assert_eq!(recorded.passwords[&format!("{WORK}.admin")], "gone");
    assert_eq!(
        world.fake.ran.lock().unwrap().len(),
        1,
        "the entry ran once"
    );
}

#[test]
fn another_settings_file_than_the_vms_is_refused_before_the_person_is_asked() {
    let world = World::new();
    world.vm("work", WORK, State::Adding, false);
    let elsewhere = world.home.path().join("other.toml");
    std::fs::write(
        &elsewhere,
        format!(
            "default = \"second-login\"\n{SECOND_LOGIN}{}",
            vm_entry("work", WORK)
        ),
    )
    .unwrap();

    let said = RefCell::new(Vec::new());
    let refused = remove_with(
        &world.lifecycle(),
        &world,
        Asked {
            which: Which::Name("work"),
            settings: &elsewhere,
        },
        &|line| said.borrow_mut().push(line.to_owned()),
    );

    assert!(refused.unwrap_err().error.contains("--config"));
    assert_eq!(world.grants.get(), 0);
    assert_eq!(world.record(WORK).unwrap().state, State::Adding);
}

#[test]
fn a_name_no_vm_has_and_the_second_login_are_refused() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));

    let (none, _) = world.remove("spare");
    let (user, _) = world.remove("second-login");

    assert!(none.unwrap_err().error.contains("no VM named"));
    assert!(user.unwrap_err().error.contains("not a VM"));
    assert_eq!(world.grants.get(), 0);
}

#[test]
fn an_uninstall_under_way_refuses_the_removal_before_anything_is_touched() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    let uninstalling = world.lifecycle().uninstalling(&Token::new(), &[]).unwrap();

    let (refused, _) = world.remove("work");

    assert!(
        refused.unwrap_err().error.contains("being uninstalled"),
        "refused at the barrier"
    );
    assert_eq!(world.record(WORK).unwrap().state, State::Created);
    assert!(world.read_settings().contains(WORK));
    assert_eq!(world.removes.get(), 0);
    drop(uninstalling);
}

/// A removal of `vm` begun by the hidden repair command and cut short at
/// its start, recorded under `change` with the VM's folder as it is now.
pub(super) fn begun_by_hand(world: &World, vm: &str, change: &str) {
    use std::os::unix::fs::MetadataExt;
    let folder = std::fs::symlink_metadata(world.vms.folder(&id(vm))).unwrap();
    let record = json!({
        "change": change, "step": "marked",
        "folder": {"device": folder.dev(), "inode": folder.ino()},
        "forgot": null, "proved": null, "passwords": {}, "kept": [],
    });
    atlas_core::files::private_folder(&world.vms.removals(), Code::State).unwrap();
    atlas_core::files::write_private(&world.vms.removal(&id(vm)), record.to_string().as_bytes())
        .unwrap();
}

#[test]
fn two_removals_at_once_never_take_the_last_desktop() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"work\"\n{}{}",
        vm_entry("work", WORK),
        vm_entry("spare", SPARE)
    ));
    world.vm("work", WORK, State::Created, true);
    world.vm("spare", SPARE, State::Created, true);
    world.runner_forgets();
    // Work's runner holds its VM at first, so its removal waits; meanwhile
    // the person allows the removal of spare too.
    let runner = crate::vm::try_lock(&world.vms.lifecycle(&id(WORK)))
        .unwrap()
        .unwrap();
    let runner = RefCell::new(Some(runner));
    let other = std::rc::Rc::new(RefCell::new(None));
    let seen = other.clone();
    *world.meanwhile.borrow_mut() = Some(Box::new(move |world: &World| {
        let (spare, _) = world.remove("spare");
        *seen.borrow_mut() = Some(spare.map_err(|error| error.error));
        runner.borrow_mut().take();
    }));

    let (work, _) = world.remove("work");

    let spare = other.borrow_mut().take().unwrap();
    assert!(
        spare.as_ref().unwrap_err().contains("last desktop"),
        "{spare:?}"
    );
    assert!(spare.unwrap_err().contains("work being removed"));
    assert_eq!(work.unwrap()["default"], "spare");
    assert_eq!(world.record(SPARE).unwrap().state, State::Created);
    assert!(world.vms.folder(&id(SPARE)).join("Disk.asif").exists());
    assert!(!world.vms.removal(&id(SPARE)).exists());
    let after = Settings::load(&world.settings()).unwrap();
    assert_eq!(after.default_desktop().name, "spare");
}

#[test]
fn a_removal_cut_short_or_begun_by_hand_counts_as_under_way() {
    for case in ["cut short", "begun by hand"] {
        let world = World::new();
        world.write_settings(&format!(
            "default = \"work\"\n{}{}",
            vm_entry("work", WORK),
            vm_entry("spare", SPARE)
        ));
        world.vm("work", WORK, State::Created, true);
        if case == "cut short" {
            world.vm("spare", SPARE, State::Removing, true);
        } else {
            world.vm("spare", SPARE, State::Created, true);
            begun_by_hand(&world, SPARE, &"d".repeat(48));
        }

        let (refused, _) = world.remove("work");

        let refused = refused.unwrap_err();
        assert!(
            refused.error.contains("last desktop"),
            "{case}: {}",
            refused.error
        );
        assert_eq!(world.grants.get(), 0, "{case}");
        assert_eq!(world.record(WORK).unwrap().state, State::Created, "{case}");
    }
}

#[test]
fn a_removal_waits_its_turn_behind_another_being_admitted_from_the_same_settings() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    let settings = atlas_core::config::resolve_path(&world.settings()).unwrap();
    let other = world.lifecycle().admitting_removal(&settings).unwrap();

    let (busy, _) = world.remove("work");

    assert_eq!(busy.unwrap_err().code, Code::Busy);
    let record = world.record(WORK).unwrap();
    assert_eq!((record.state, record.removal), (State::Created, None));
    assert_eq!(world.removes.get(), 0);
    drop(other);
}

#[test]
fn a_removal_the_hidden_command_began_is_gone_on_with_under_its_own_change() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    let earlier = "d".repeat(48);
    begun_by_hand(&world, WORK, &earlier);

    let (removed, _) = world.remove("work");

    assert_eq!(removed.unwrap()["removed"]["id"], WORK);
    let kept = world.seen.borrow()[0].as_ref().unwrap().removal.clone();
    assert_eq!(
        kept.unwrap().as_str(),
        earlier,
        "kept before the removal ran"
    );
    assert!(
        world
            .fake
            .asked
            .lock()
            .unwrap()
            .iter()
            .all(|asked| asked["change"] == earlier.as_str())
    );
    assert!(!world.vms.folder(&id(WORK)).exists());
    assert_eq!(world.record(WORK), None);
}

#[test]
fn a_removal_recorded_under_another_change_than_the_kept_one_is_refused_before_the_record_changes()
{
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    let lifecycle = world.lifecycle();
    let mut record = world.record(WORK).unwrap();
    record.removal = Some(Token::parse(&"e".repeat(48)).unwrap());
    lifecycle.keep_record(&record).unwrap();
    begun_by_hand(&world, WORK, &"d".repeat(48));

    let (refused, _) = world.remove("work");

    let refused = refused.unwrap_err();
    assert_eq!(refused.code, Code::State);
    assert!(
        refused.error.contains("Another removal"),
        "{}",
        refused.error
    );
    assert_eq!(world.record(WORK).unwrap(), record, "left as it was");
    assert_eq!(world.removes.get(), 0);
    assert!(world.vms.folder(&id(WORK)).join("Disk.asif").exists());
}

#[test]
fn the_default_is_never_handed_to_a_vm_whose_removal_was_cut_short() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"a\"\n{}{}{}",
        vm_entry("a", WORK),
        vm_entry("b", SPARE),
        vm_entry("c", THIRD)
    ));
    world.vm("a", WORK, State::Created, true);
    world.vm("c", THIRD, State::Created, true);
    // B's removal destroyed it and was cut short before its entry went.
    world.vm("b", SPARE, State::Removing, true);
    begun_by_hand(&world, SPARE, &"d".repeat(48));
    world.runner_forgets();

    let (removed, said) = world.remove("a");

    let removed = removed.unwrap();
    assert_eq!(removed["default"], "c", "{removed}");
    assert_eq!(
        Settings::load(&world.settings())
            .unwrap()
            .default_desktop()
            .name,
        "c"
    );
    assert!(
        said.iter()
            .any(|line| line.contains("default desktop is now \u{201c}c\u{201d}")),
        "{said:?}"
    );
}

#[test]
fn a_vm_renamed_while_its_removal_waits_is_taken_off_the_settings_by_its_id() {
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, true);
    world.runner_forgets();
    let runner = crate::vm::try_lock(&world.vms.lifecycle(&id(WORK)))
        .unwrap()
        .unwrap();
    let runner = RefCell::new(Some(runner));
    *world.meanwhile.borrow_mut() = Some(Box::new(move |world: &World| {
        let renamed = world
            .read_settings()
            .replace("name = \"work\"", "name = \"job\"");
        world.write_settings(&renamed);
        runner.borrow_mut().take();
    }));

    let (removed, _) = world.remove("work");

    assert_eq!(removed.unwrap()["removed"]["id"], WORK);
    assert!(!world.read_settings().contains(WORK));
    assert!(!world.read_settings().contains("job"));
    assert_eq!(world.record(WORK), None);
    assert!(world.names().is_empty());
}

#[test]
fn a_kept_path_that_cannot_be_looked_at_keeps_the_record_and_only_one_truly_gone_is_cleared() {
    use std::os::unix::fs::PermissionsExt;
    let world = World::new();
    world.write_settings(&format!(
        "default = \"second-login\"\n{SECOND_LOGIN}{}",
        vm_entry("work", WORK)
    ));
    world.vm("work", WORK, State::Created, false);
    let shut = world.home.path().join("shut");
    std::fs::create_dir(&shut).unwrap();
    let inside = shut.join("other").to_string_lossy().into_owned();
    std::fs::write(&inside, "other").unwrap();
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    let gone = world
        .home
        .path()
        .join("gone")
        .to_string_lossy()
        .into_owned();
    *world.answer.borrow_mut() =
        Some(json!({"ok": true, "removal": "done", "kept": [inside.clone(), gone], "left": []}));

    let (removed, _) = world.remove("work");
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(removed.unwrap()["kept"], json!([inside.clone()]));
    let record = world.record(WORK).unwrap();
    assert_eq!(record.state, State::Removing);
    assert_eq!(record.kept, [inside]);
    assert_eq!(world.names(), ["work"]);
}
