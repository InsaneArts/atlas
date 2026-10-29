use super::{print::print as print_path, *};
use std::os::unix::fs::{PermissionsExt, symlink};

const BUNDLE_ID: &str = "test.atlas.install-probe";

/// A helper home, and a place outside it apps are installed from.
struct Homes {
    _directory: tempfile::TempDir,
    home: PathBuf,
    outside: PathBuf,
}

impl Homes {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(directory.path()).unwrap();
        let (home, outside) = (base.join("home"), base.join("outside"));
        for folder in [&home, &outside] {
            fs::create_dir(folder).unwrap();
            fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            _directory: directory,
            home,
            outside,
        }
    }

    /// A small app bundle called `name` outside the home.
    fn app(&self, name: &str) -> PathBuf {
        bundle(&self.outside.join(name))
    }

    fn destination(&self, name: &str) -> PathBuf {
        self.home.join(APPLICATIONS).join(name)
    }

    /// Where install `id` of Probe.app is held once taken away.
    fn held(&self, id: &str) -> PathBuf {
        self.home
            .join(INSTALLS)
            .join(HELD)
            .join(id)
            .join("Probe.app")
    }

    fn store(&self) -> Store {
        Store::open(&self.home, Lock::Exclusive).unwrap().unwrap()
    }

    fn record(&self, id: &str) -> Install {
        match self.store().load(id) {
            Found::Record(record) => *record,
            Found::Missing | Found::Unreadable => panic!("no record of {id}"),
        }
    }

    /// Install `name` from outside as install `id`, registering nothing.
    fn install(&self, id: &str, name: &str) -> Result<Value> {
        let source = self.app(name);
        install_unregistered(&self.home, id, &source, name)
    }

    fn remove(&self, id: &str) -> Value {
        remove(&self.home, id, &|| Ok(()), &looks(&|| Ok(Vec::new()))).unwrap()
    }

    /// [`Homes::remove`], moving with `mover`.
    fn remove_with(&self, id: &str, mover: Mover) -> Value {
        self.remove_on(id, mover, &volume)
    }

    /// [`Homes::remove`], moving with `mover` and telling volumes apart
    /// by `volume`.
    fn remove_on(&self, id: &str, mover: Mover, volume: Volume) -> Value {
        remove_moving(
            &self.home,
            id,
            &|| Ok(()),
            &looks(&|| Ok(Vec::new())),
            mover,
            volume,
        )
        .unwrap()
    }

    /// Set install `id`'s record to `phase`, as a worker cut short leaves
    /// it.
    fn cut(&self, id: &str, phase: InstallPhase) {
        let mut record = self.record(id);
        record.phase = phase;
        self.store().save(&record).unwrap();
    }

    /// Install `id`'s own folder among the held ones, made as a move aside
    /// makes it.
    fn hold_folder(&self, id: &str) -> PathBuf {
        drop(self.store().hold_folder(id).unwrap());
        self.home.join(INSTALLS).join(HELD).join(id)
    }

    /// Say install `id` was held `days` days ago.
    fn held_days_ago(&self, id: &str, days: u64) {
        let mut record = self.record(id);
        record.held_at = Some(now_ms() - days * 24 * 60 * 60 * 1000);
        self.store().save(&record).unwrap();
    }
}

/// [`install`], registering nothing with LaunchServices.
fn install_unregistered(home: &Path, id: &str, source: &Path, name: &str) -> Result<Value> {
    install_moving(home, id, source, name, &|| Ok(()), &move_between, &|_| true)
}

/// What macOS would say, with `processes` as the helper's process list and
/// no translocated app traced anywhere.
fn looks<'a>(processes: &'a dyn Fn() -> Result<Vec<(u32, String)>>) -> Looks<'a> {
    Looks {
        processes,
        original: &|_| None,
        bundle_id: &|_| None,
    }
}

/// A bundle at `path` with an Info.plist naming its executable, the
/// executable and a resource.
fn bundle(path: &Path) -> PathBuf {
    let contents = path.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("Resources")).unwrap();
    fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{BUNDLE_ID}</string>
<key>CFBundleExecutable</key><string>Probe</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>"#
        ),
    )
    .unwrap();
    fs::write(contents.join("MacOS/Probe"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(
        contents.join("MacOS/Probe"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(contents.join("Resources/strings.txt"), "hello").unwrap();
    path.to_owned()
}

fn id() -> String {
    crate::files::token()
}

/// The real move.
fn moved(
    from_dir: &fs::File,
    from: &FileName,
    to_dir: &fs::File,
    to: &FileName,
) -> std::io::Result<Moved> {
    move_between(from_dir, from, to_dir, to)
}

/// Every path under `path` with its bytes, for a byte-for-byte comparison.
fn contents(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut folders = vec![path.to_owned()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            found.push((folder.clone(), fs::read(&folder).unwrap_or_default()));
            continue;
        };
        for entry in entries.flatten() {
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_dir() {
                folders.push(entry.path());
            }
            found.push((entry.path(), fs::read(entry.path()).unwrap_or_default()));
        }
    }
    found.sort();
    found
}

#[test]
fn an_app_already_in_applications_is_never_installed_over_or_taken() {
    let homes = Homes::new();
    fs::create_dir(homes.home.join(APPLICATIONS)).unwrap();
    let file = homes.destination("File.app");
    fs::write(&file, "a file").unwrap();
    fs::create_dir(homes.destination("Folder.app")).unwrap();
    symlink(&homes.outside, homes.destination("Link.app")).unwrap();
    let other = bundle(&homes.destination("Other.app"));
    let before = contents(&homes.home.join(APPLICATIONS));

    for name in ["File.app", "Folder.app", "Link.app", "Other.app"] {
        let id = id();
        let error = homes.install(&id, name).unwrap_err();
        assert_eq!(error.code, "input", "{name}");
        assert!(error.error.contains("already has"), "{error}");
        assert!(matches!(homes.store().load(&id), Found::Missing), "{name}");
        assert_eq!(homes.remove(&id)["outcome"], "alreadyGone");
    }
    assert_eq!(contents(&homes.home.join(APPLICATIONS)), before);
    assert!(other.exists());
}

#[test]
fn an_install_record_never_names_a_destination_that_existed_before() {
    let homes = Homes::new();
    let id = id();
    let reply = homes.install(&id, "Probe.app").unwrap();
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["bundleId"], BUNDLE_ID);
    assert_eq!(reply["version"], "1.0 (1)");
    assert_eq!(reply["path"], json!(homes.destination("Probe.app")));

    let record = homes.record(&id);
    assert_eq!(record.phase, InstallPhase::Installed);
    assert!(!record.existed);
    assert!(record.made_folder);
    let placed = fs::symlink_metadata(homes.destination("Probe.app")).unwrap();
    assert_eq!(record.print.unwrap().root, node(&placed));
    assert!(
        !homes
            .store()
            .folder(&id)
            .join("staging")
            .join("Probe.app")
            .exists(),
        "nothing is left staged"
    );
}

#[test]
fn an_unchanged_install_is_moved_aside_whole_in_the_helper_and_nothing_is_deleted() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let placed = node(&fs::symlink_metadata(homes.destination("Probe.app")).unwrap());

    let reply = homes.remove(&id);
    assert_eq!(reply["outcome"], "held");
    let held = homes.held(&id);
    assert_eq!(reply["heldPath"], json!(held));
    assert!(!homes.destination("Probe.app").exists());
    assert_eq!(node(&fs::symlink_metadata(&held).unwrap()), placed);
    assert_eq!(
        fs::read_to_string(held.join("Contents/Resources/strings.txt")).unwrap(),
        "hello"
    );
    for folder in [
        held.parent().unwrap(),
        held.parent().unwrap().parent().unwrap(),
    ] {
        let mode = fs::metadata(folder).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o700, "{}", folder.display());
    }
    let record = homes.record(&id);
    assert_eq!(record.phase, InstallPhase::Held);
    assert!(record.held_at.is_some_and(|at| now_ms() - at < 60_000));
    let again = homes.remove(&id);
    assert_eq!(
        (&again["outcome"], &again["heldPath"]),
        (&json!("held"), &json!(held)),
        "the same, again"
    );
}

#[test]
fn an_install_removed_by_hand_is_already_gone() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    fs::remove_dir_all(homes.destination("Probe.app")).unwrap();

    assert_eq!(homes.remove(&id)["outcome"], "alreadyGone");
    assert_eq!(homes.record(&id).phase, InstallPhase::Removed);
    assert_eq!(homes.remove(&id)["outcome"], "alreadyGone", "and stays so");

    // Removed by hand once it was being moved aside, before it got there.
    let holding = Homes::new();
    let cut = crate::files::token();
    holding.install(&cut, "Probe.app").unwrap();
    holding.cut(&cut, InstallPhase::Holding);
    fs::remove_dir_all(holding.destination("Probe.app")).unwrap();
    assert_eq!(holding.remove(&cut)["outcome"], "alreadyGone");
    assert_eq!(holding.record(&cut).phase, InstallPhase::Removed);
}

#[test]
fn a_placed_install_is_registered_with_launch_services() {
    let homes = Homes::new();
    let source = homes.app("Probe.app");
    let asked = std::cell::RefCell::new(Vec::new());
    let register = |path: &Path| {
        asked.borrow_mut().push(path.to_owned());
        false
    };

    let id = id();
    let reply = install_moving(
        &homes.home,
        &id,
        &source,
        "Probe.app",
        &|| Ok(()),
        &move_between,
        &register,
    )
    .unwrap();
    assert_eq!(*asked.borrow(), [homes.destination("Probe.app")]);
    assert_eq!(
        (&reply["installed"], &reply["registered"]),
        (&json!(true), &json!(false)),
        "installed all the same, and openable by its path"
    );

    let refused = install_moving(
        &homes.home,
        &crate::files::token(),
        &source,
        "Probe.app",
        &|| Ok(()),
        &move_between,
        &register,
    );
    assert!(refused.is_err());
    assert_eq!(
        asked.borrow().len(),
        1,
        "nothing is registered for a refusal"
    );
}

#[test]
fn a_changed_bundle_is_kept_and_reported() {
    type Change = fn(&Path);
    let changes: [(&str, Change); 4] = [
        ("a resource rewritten", |app| {
            fs::write(app.join("Contents/Resources/strings.txt"), "changed!").unwrap();
        }),
        ("a file added", |app| {
            fs::write(app.join("Contents/Resources/new.txt"), "new").unwrap();
        }),
        ("a mode changed", |app| {
            fs::set_permissions(
                app.join("Contents/Resources/strings.txt"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }),
        ("the executable replaced", |app| {
            let executable = app.join("Contents/MacOS/Probe");
            let beside = app.join("Contents/MacOS/Probe.new");
            fs::write(&beside, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&beside, fs::Permissions::from_mode(0o755)).unwrap();
            fs::rename(&beside, &executable).unwrap();
        }),
    ];
    for (what, change) in changes {
        let homes = Homes::new();
        let id = id();
        homes.install(&id, "Probe.app").unwrap();
        change(&homes.destination("Probe.app"));

        let reply = homes.remove(&id);
        assert_eq!(
            (&reply["outcome"], &reply["reason"]),
            (&json!("kept"), &json!("changed")),
            "{what}"
        );
        assert!(homes.destination("Probe.app").exists(), "{what}");
        assert_eq!(homes.remove(&id)["reason"], "changed", "kept for good");
    }
}

#[test]
fn a_bundle_made_writable_by_others_is_kept() {
    for mode in [0o775, 0o777] {
        let homes = Homes::new();
        let id = id();
        homes.install(&id, "Probe.app").unwrap();
        let destination = homes.destination("Probe.app");
        fs::set_permissions(&destination, fs::Permissions::from_mode(mode)).unwrap();

        let reply = homes.remove(&id);
        assert_eq!(reply["reason"], "changed", "{mode:o}");
        assert!(destination.join("Contents/MacOS/Probe").exists());
    }
}

#[test]
fn a_replaced_bundle_is_kept() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let destination = homes.destination("Probe.app");
    // The bundle Atlas placed is kept under another name, so the one made
    // in its place cannot reuse its node.
    fs::rename(&destination, homes.destination("Probe.app.original")).unwrap();
    bundle(&destination);

    let reply = homes.remove(&id);
    assert_eq!(reply["reason"], "replaced");
    assert!(destination.exists());
    assert!(homes.destination("Probe.app.original").exists());
}

#[test]
fn a_bundle_that_cannot_be_read_to_its_end_is_kept() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let locked = homes.destination("Probe.app/Contents/Resources");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let reply = homes.remove(&id);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(reply["reason"], "changed", "{reply}");
    assert!(homes.destination("Probe.app/Contents/MacOS/Probe").exists());
}

#[test]
fn attributes_macos_adds_on_first_launch_do_not_count_as_changes() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let destination = homes.destination("Probe.app");
    for (name, value) in [
        ("com.apple.quarantine", "0081;00000000;Atlas;"),
        ("com.apple.provenance", "01"),
    ] {
        for path in [
            destination.clone(),
            destination.join("Contents/MacOS/Probe"),
        ] {
            let written = Command::new("/usr/bin/xattr")
                .args(["-w", name, value])
                .arg(&path)
                .status()
                .unwrap();
            assert!(written.success());
        }
    }
    let touched = Command::new("/usr/bin/touch")
        .arg(&destination)
        .arg(destination.join("Contents"))
        .status()
        .unwrap();
    assert!(touched.success());

    let print = print_path(&destination, Some("Probe"), Limits::default()).unwrap();
    assert_eq!(Some(print), homes.record(&id).print);
    assert_eq!(homes.remove(&id)["outcome"], "held");
    assert!(!destination.exists());
}

#[test]
fn an_install_cut_at_each_step_is_finished_without_deleting_it() {
    let staged = |homes: &Homes, id: &str| {
        let staging = homes.store().folder(id).join("staging");
        fs::create_dir_all(&staging).unwrap();
        staging.join("Probe.app")
    };
    let homes = Homes::new();

    // Cut after copying: only Atlas's own staged copy, never placed.
    let cut = id();
    homes.install(&cut, "Probe.app").unwrap();
    fs::rename(homes.destination("Probe.app"), staged(&homes, &cut)).unwrap();
    let mut record = homes.record(&cut);
    record.phase = InstallPhase::Copying;
    record.print = None;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&cut)["outcome"], "alreadyGone");
    assert_eq!(homes.record(&cut).phase, InstallPhase::Removed);

    // Cut after printing, before the move into place.
    let before_move = id();
    homes.install(&before_move, "Probe.app").unwrap();
    fs::rename(homes.destination("Probe.app"), staged(&homes, &before_move)).unwrap();
    homes.cut(&before_move, InstallPhase::Placing);
    assert_eq!(homes.remove(&before_move)["outcome"], "alreadyGone");

    // Cut after the move into place, before it was written installed.
    let after_move = id();
    homes.install(&after_move, "Probe.app").unwrap();
    homes.cut(&after_move, InstallPhase::Placing);
    assert_eq!(homes.remove(&after_move)["outcome"], "held");
    assert!(homes.held(&after_move).exists());

    // Cut after the move aside, before it was written held: found there,
    // it is done, never moved again.
    let moved_aside = id();
    homes.install(&moved_aside, "Probe.app").unwrap();
    homes.hold_folder(&moved_aside);
    fs::rename(homes.destination("Probe.app"), homes.held(&moved_aside)).unwrap();
    homes.cut(&moved_aside, InstallPhase::Holding);
    assert_eq!(homes.remove(&moved_aside)["outcome"], "held");
    assert!(
        homes
            .held(&moved_aside)
            .join("Contents/MacOS/Probe")
            .exists()
    );

    // Cut before the move aside: every check runs again.
    let unmoved = id();
    homes.install(&unmoved, "Probe.app").unwrap();
    homes.cut(&unmoved, InstallPhase::Holding);
    assert_eq!(homes.remove(&unmoved)["outcome"], "held");

    for id in [&after_move, &moved_aside, &unmoved] {
        assert_eq!(homes.record(id).phase, InstallPhase::Held);
        assert!(homes.held(id).join("Contents/MacOS/Probe").exists());
    }
    assert!(!homes.destination("Probe.app").exists());
    assert!(homes.home.join(APPLICATIONS).is_dir(), "never removed");
    assert!(homes.outside.join("Probe.app").exists(), "the source stays");
}

#[test]
fn a_move_aside_not_written_through_is_found_held_next_time() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let unsynced = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        moved(from, a, to, b).map(|_| Moved::Unsynced(std::io::Error::other("no sync")))
    };

    let first = homes.remove_with(&id, &unsynced);
    assert_eq!(first["ok"], false, "unfinished, never done: {first}");
    assert_eq!(homes.record(&id).phase, InstallPhase::Holding);
    assert!(homes.held(&id).exists());

    let second = homes.remove(&id);
    assert_eq!(second["outcome"], "held");
    assert_eq!(homes.record(&id).phase, InstallPhase::Held);
}

#[test]
fn an_install_whose_holding_folder_is_on_another_volume_is_kept_never_copied() {
    // The volume says so before anything moves.
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let holding = homes.home.join(INSTALLS).join(HELD);
    let elsewhere = |folder: &fs::File| {
        let held = fs::metadata(&holding).map(|metadata| metadata.ino());
        Ok(u64::from(held.ok() == Some(folder.metadata()?.ino())))
    };
    let nothing_moves =
        |_: &fs::File, _: &FileName, _: &fs::File, _: &FileName| -> std::io::Result<Moved> {
            panic!("nothing is moved to another volume")
        };

    let reply = homes.remove_on(&id, &nothing_moves, &elsewhere);
    assert_eq!(
        (&reply["outcome"], &reply["reason"]),
        (&json!("kept"), &json!("other-volume"))
    );
    assert_eq!(homes.record(&id).phase, InstallPhase::Kept);
    assert!(homes.destination("Probe.app/Contents/MacOS/Probe").exists());
    assert!(!homes.held(&id).exists());

    // The move says so.
    let homes = Homes::new();
    let id = crate::files::token();
    homes.install(&id, "Probe.app").unwrap();
    let crossing = |_: &fs::File, _: &FileName, _: &fs::File, _: &FileName| {
        Err(std::io::Error::from_raw_os_error(libc::EXDEV))
    };
    let reply = homes.remove_with(&id, &crossing);
    assert_eq!(
        (&reply["outcome"], &reply["reason"]),
        (&json!("kept"), &json!("other-volume"))
    );
    assert!(homes.destination("Probe.app/Contents/MacOS/Probe").exists());
    assert!(!homes.held(&id).exists());
}

#[test]
fn an_install_held_seven_days_is_deleted_and_a_newer_one_kept() {
    let homes = Homes::new();
    let (old, new) = (id(), id());
    for id in [&old, &new] {
        homes.install(id, "Probe.app").unwrap();
        assert_eq!(homes.remove(id)["outcome"], "held");
    }
    homes.held_days_ago(&old, 7);
    homes.held_days_ago(&new, 6);

    homes.store().prune(now_ms());
    assert!(!homes.held(&old).exists());
    assert!(!homes.held(&old).parent().unwrap().exists());
    assert_eq!(homes.record(&old).phase, InstallPhase::Removed);
    assert_eq!(homes.remove(&old)["outcome"], "alreadyGone");
    assert!(homes.held(&new).join("Contents/MacOS/Probe").exists());
    assert_eq!(homes.record(&new).phase, InstallPhase::Held);
}

#[test]
fn removing_or_installing_deletes_the_installs_held_long_enough() {
    let homes = Homes::new();
    let (held, other) = (id(), id());
    homes.install(&held, "Probe.app").unwrap();
    homes.remove(&held);
    homes.held_days_ago(&held, 8);

    assert_eq!(homes.remove(&other)["outcome"], "alreadyGone");
    assert!(!homes.held(&held).exists(), "deleted by a removal");

    let again = id();
    homes.install(&again, "Probe.app").unwrap();
    homes.remove(&again);
    homes.held_days_ago(&again, 8);
    homes.install(&id(), "Probe.app").unwrap();
    assert!(!homes.held(&again).exists(), "deleted by an install");
}

#[test]
fn the_prune_deletes_only_what_a_record_names_as_held_and_only_its_own_folder() {
    let homes = Homes::new();
    let holding = homes.home.join(INSTALLS).join(HELD);

    // Not named by any record.
    let stranger = id();
    let foreign = bundle(&homes.hold_folder(&stranger).join("Probe.app"));
    // Named, but another bundle took its place since.
    let replaced = id();
    homes.install(&replaced, "Probe.app").unwrap();
    homes.remove(&replaced);
    fs::rename(
        homes.held(&replaced),
        holding.join(&replaced).join("Kept.app"),
    )
    .unwrap();
    bundle(&homes.held(&replaced));
    homes.held_days_ago(&replaced, 30);
    // Named, but not held: installed, with a held time it never had.
    let installed = id();
    homes.install(&installed, "Other.app").unwrap();
    let mut record = homes.record(&installed);
    record.held_at = Some(0);
    homes.store().save(&record).unwrap();
    bundle(&holding.join(&installed).join("Other.app"));
    // Named and held, beside a file of someone else's in its folder.
    let beside = id();
    homes.install(&beside, "Probe.app").unwrap();
    homes.remove(&beside);
    fs::write(holding.join(&beside).join("note.txt"), "mine").unwrap();
    homes.held_days_ago(&beside, 30);

    homes.store().prune(now_ms());
    assert!(foreign.join("Contents/MacOS/Probe").exists(), "no record");
    assert!(homes.held(&replaced).join("Contents/MacOS/Probe").exists());
    assert!(holding.join(&replaced).join("Kept.app").exists());
    assert_eq!(homes.record(&replaced).phase, InstallPhase::Held);
    assert!(homes.destination("Other.app/Contents/MacOS/Probe").exists());
    assert!(holding.join(&installed).join("Other.app").exists());
    assert_eq!(homes.record(&installed).phase, InstallPhase::Installed);
    assert!(!homes.held(&beside).exists());
    assert_eq!(
        fs::read_to_string(holding.join(&beside).join("note.txt")).unwrap(),
        "mine",
        "only the bundle goes"
    );
}

#[test]
fn a_holding_folder_others_may_open_is_never_pruned() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    homes.remove(&id);
    homes.held_days_ago(&id, 30);
    let holding = homes.home.join(INSTALLS).join(HELD);
    fs::set_permissions(&holding, fs::Permissions::from_mode(0o755)).unwrap();

    homes.store().prune(now_ms());
    assert!(homes.held(&id).join("Contents/MacOS/Probe").exists());
    assert_eq!(homes.record(&id).phase, InstallPhase::Held);
}

#[test]
fn a_placement_not_written_through_stays_placing() {
    let homes = Homes::new();
    let id = id();
    let source = homes.app("Probe.app");
    let unsynced = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        moved(from, a, to, b).map(|_| Moved::Unsynced(std::io::Error::other("no sync")))
    };

    let reply = install_moving(
        &homes.home,
        &id,
        &source,
        "Probe.app",
        &|| Ok(()),
        &unsynced,
        &|_| true,
    )
    .unwrap();
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(homes.record(&id).phase, InstallPhase::Placing);
    assert!(homes.destination("Probe.app").exists());
    assert_eq!(homes.remove(&id)["outcome"], "held", "the print proves it");
}

#[test]
fn a_record_atlas_cannot_read_is_kept_untouched_and_never_acted_on() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let path = homes.store().folder(&id).join("record.json");
    let unreadable = b"{\"version\": 2, \"id\": \"elsewhere\"}";
    fs::write(&path, unreadable).unwrap();

    let reply = homes.remove(&id);
    assert_eq!(
        (&reply["outcome"], &reply["reason"]),
        (&json!("kept"), &json!("unreadable"))
    );
    assert_eq!(fs::read(&path).unwrap(), unreadable);
    assert!(homes.destination("Probe.app/Contents/MacOS/Probe").exists());
    let listed = states(
        &homes.home,
        std::slice::from_ref(&id),
        &|| Ok(()),
        &looks(&|| Ok(Vec::new())),
    )
    .unwrap();
    assert_eq!(listed["installs"][0]["phase"], "unreadable");
    assert_eq!(listed["installs"][0]["verified"], false);
}

#[test]
fn a_bundle_a_process_runs_from_is_not_taken() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let executable = homes.destination("Probe.app/Contents/MacOS/Probe");
    let running = executable.to_string_lossy().into_owned();
    let processes = move || Ok(vec![(812, running.clone())]);

    let reply = remove(&homes.home, &id, &|| Ok(()), &looks(&processes)).unwrap();
    assert_eq!(reply["outcome"], "inUse");
    assert!(executable.exists());
    assert_eq!(homes.record(&id).phase, InstallPhase::Installed);

    let listed = states(
        &homes.home,
        std::slice::from_ref(&id),
        &|| Ok(()),
        &looks(&processes),
    )
    .unwrap();
    assert_eq!(listed["installs"][0]["processes"][0]["pid"], 812);
    assert_eq!(listed["installs"][0]["unchanged"], true);
}

#[test]
fn a_translocated_process_keeps_the_install_and_is_never_signalled() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let translocated =
        "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe"
            .to_owned();
    let processes = move || Ok(vec![(900, translocated.clone())]);
    let destination = homes.destination("Probe.app");
    let traced = move |_: &Path| Some(destination.clone());
    let ours = |_: &Path| Some(BUNDLE_ID.to_owned());

    for looks in [
        Looks {
            processes: &processes,
            original: &|_| None,
            bundle_id: &ours,
        },
        Looks {
            processes: &processes,
            original: &traced,
            bundle_id: &|_| None,
        },
    ] {
        let reply = remove(&homes.home, &id, &|| Ok(()), &looks).unwrap();
        assert_eq!(reply["outcome"], "inUse");
        let listed = states(&homes.home, std::slice::from_ref(&id), &|| Ok(()), &looks).unwrap();
        assert_eq!(
            listed["installs"][0]["processes"],
            json!([]),
            "never signalled"
        );
        assert_eq!(listed["installs"][0]["unproven"][0]["pid"], 900);
    }
}

#[test]
fn the_bundle_identifier_alone_never_selects_anything() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let twin = bundle(&homes.home.join("Desktop/Probe.app"));
    let other = bundle(&homes.destination("Probe Copy.app"));

    assert_eq!(homes.remove(&id)["outcome"], "held");
    assert!(twin.join("Contents/MacOS/Probe").exists());
    assert!(other.join("Contents/MacOS/Probe").exists());
}

#[test]
fn side_effect_files_are_reported_and_never_touched() {
    let homes = Homes::new();
    let earlier = homes.home.join(format!("Library/Caches/{BUNDLE_ID}"));
    fs::create_dir_all(&earlier).unwrap();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let preferences = homes
        .home
        .join(format!("Library/Preferences/{BUNDLE_ID}.plist"));
    fs::create_dir_all(preferences.parent().unwrap()).unwrap();
    fs::write(&preferences, "written by the app").unwrap();

    let reply = homes.remove(&id);
    assert_eq!(reply["outcome"], "held");
    assert_eq!(
        reply["sideEffects"]["kept"],
        json!([
            {"path": format!("Library/Preferences/{BUNDLE_ID}.plist"), "existedBefore": false},
            {"path": format!("Library/Caches/{BUNDLE_ID}"), "existedBefore": true},
        ])
    );
    assert!(preferences.exists() && earlier.exists());
}

#[test]
fn links_inside_a_bundle_are_never_followed() {
    let homes = Homes::new();
    let outside = homes.outside.join("keep.txt");
    fs::write(&outside, "keep me").unwrap();
    let source = homes.app("Probe.app");
    symlink(&outside, source.join("Contents/Resources/link.txt")).unwrap();
    symlink(&homes.outside, source.join("Contents/Resources/folder")).unwrap();
    let id = id();
    install_unregistered(&homes.home, &id, &source, "Probe.app").unwrap();

    assert_eq!(homes.remove(&id)["outcome"], "held");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "keep me");
    assert!(homes.outside.join("Probe.app").exists());
}

#[test]
fn an_oversized_bundle_is_refused_before_it_is_placed() {
    let homes = Homes::new();
    let app = homes.app("Probe.app");
    let few_entries = Limits {
        entries: 3,
        ..Limits::default()
    };
    let refused = |limits| print::refusal(print_path(&app, Some("Probe"), limits).unwrap_err());
    assert_eq!(refused(few_entries).code, "input");
    let few_bytes = Limits {
        bytes: 4,
        ..Limits::default()
    };
    assert_eq!(refused(few_bytes).code, "input");
    let late = Limits {
        until: Some(Instant::now()),
        ..Limits::default()
    };
    assert!(matches!(
        print_path(&app, Some("Probe"), late),
        Err(Unprinted::TooSlow)
    ));
    assert!(print_path(&app, Some("Probe"), Limits::default()).is_ok());
}

#[test]
fn an_install_that_cannot_be_copied_leaves_nothing_but_a_removed_record() {
    let homes = Homes::new();
    let missing = id();
    let error = install(
        &homes.home,
        &missing,
        &homes.outside.join("Missing.app"),
        "Missing.app",
        &|| Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.code, "input");
    assert!(matches!(homes.store().load(&missing), Found::Missing));

    let source = homes.app("Locked.app");
    let locked = source.join("Contents/Resources");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let id = id();
    let error = install(&homes.home, &id, &source, "Locked.app", &|| Ok(())).unwrap_err();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(error.code, "input", "{error}");
    assert_eq!(homes.record(&id).phase, InstallPhase::Removed);
    assert!(
        !homes
            .store()
            .folder(&id)
            .join("staging/Locked.app")
            .exists()
    );
    assert!(!homes.destination("Locked.app").exists());
}

#[test]
fn an_install_read_says_where_it_is_only_while_its_own_folder_is_there() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let destination = homes.destination("Probe.app");
    let running = format!("{}/Contents/MacOS/Probe", destination.display());
    let processes = move || Ok(vec![(812, running.clone())]);
    let read = || {
        states(
            &homes.home,
            std::slice::from_ref(&id),
            &|| Ok(()),
            &looks(&processes),
        )
        .unwrap()
    };

    assert_eq!(read()["installs"][0]["verified"], true);
    fs::rename(&destination, homes.destination("Probe.app.original")).unwrap();
    bundle(&destination);
    let replaced = read();
    let entry = &replaced["installs"][0];
    assert_eq!(entry["verified"], false);
    assert!(
        entry.get("processes").is_none() && entry.get("path").is_none(),
        "{entry}"
    );
}

/// `reply` as text, with what differs between runs named: the homes'
/// folders, install `id` and the node of the folder it placed.
fn shape(
    reply: &Value,
    homes: &Homes,
    id: &str,
    root: Option<&super::super::model::Node>,
) -> String {
    let mut text = serde_json::to_string(reply).unwrap();
    if let Some(root) = root {
        text = text.replace(&serde_json::to_string(root).unwrap(), "\"ROOT\"");
    }
    text.replace(&homes.home.display().to_string(), "HOME")
        .replace(id, "ID")
}

#[test]
fn the_install_replies_keep_their_wire_shape() {
    let mut said = Vec::new();

    let homes = Homes::new();
    let id = id();
    let installed = homes.install(&id, "Probe.app").unwrap();
    said.push(shape(&installed, &homes, &id, None));
    let root = homes.record(&id).print.unwrap().root;
    let executable = homes.destination("Probe.app/Contents/MacOS/Probe");
    let running = executable.to_string_lossy().into_owned();
    let translocated =
        "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe";
    let processes = move || Ok(vec![(812, running.clone()), (900, translocated.to_owned())]);
    let ours = |_: &Path| Some(BUNDLE_ID.to_owned());
    let seen = Looks {
        processes: &processes,
        original: &|_| None,
        bundle_id: &ours,
    };
    let missing = crate::files::token();
    let read = states(
        &homes.home,
        &[id.clone(), missing.clone()],
        &|| Ok(()),
        &seen,
    )
    .unwrap();
    said.push(shape(&read, &homes, &id, Some(&root)).replace(&missing, "MISSING"));
    let in_use = remove(&homes.home, &id, &|| Ok(()), &seen).unwrap();
    said.push(shape(&in_use, &homes, &id, None));
    let preferences = homes
        .home
        .join(format!("Library/Preferences/{BUNDLE_ID}.plist"));
    fs::create_dir_all(preferences.parent().unwrap()).unwrap();
    fs::write(&preferences, "written by the app").unwrap();
    said.push(shape(&homes.remove(&id), &homes, &id, None));
    let read = states(&homes.home, std::slice::from_ref(&id), &|| Ok(()), &seen).unwrap();
    said.push(shape(&read, &homes, &id, None));
    said.push(shape(&homes.remove(&missing), &homes, &missing, None));

    let homes = Homes::new();
    let id = crate::files::token();
    homes.install(&id, "Probe.app").unwrap();
    fs::write(
        homes.destination("Probe.app/Contents/Resources/strings.txt"),
        "changed!",
    )
    .unwrap();
    said.push(shape(&homes.remove(&id), &homes, &id, None));

    let homes = Homes::new();
    let id = crate::files::token();
    homes.install(&id, "Probe.app").unwrap();
    let destination = homes.destination("Probe.app");
    fs::rename(&destination, homes.destination("Probe.app.original")).unwrap();
    bundle(&destination);
    said.push(shape(&homes.remove(&id), &homes, &id, None));

    let homes = Homes::new();
    let id = crate::files::token();
    homes.install(&id, "Probe.app").unwrap();
    let crossing = |_: &fs::File, _: &FileName, _: &fs::File, _: &FileName| {
        Err(std::io::Error::from_raw_os_error(libc::EXDEV))
    };
    said.push(shape(&homes.remove_with(&id, &crossing), &homes, &id, None));
    fs::remove_dir_all(homes.destination("Probe.app")).unwrap();
    said.push(shape(&homes.remove(&id), &homes, &id, None));

    let homes = Homes::new();
    let id = crate::files::token();
    homes.install(&id, "Probe.app").unwrap();
    fs::remove_dir_all(homes.destination("Probe.app")).unwrap();
    said.push(shape(&homes.remove(&id), &homes, &id, None));
    let path = homes.store().folder(&id).join("record.json");
    fs::write(&path, b"{\"version\": 2}").unwrap();
    said.push(shape(&homes.remove(&id), &homes, &id, None));
    let read = states(&homes.home, std::slice::from_ref(&id), &|| Ok(()), &seen).unwrap();
    said.push(shape(&read, &homes, &id, None));

    let homes = Homes::new();
    let id = crate::files::token();
    let source = homes.app("Probe.app");
    let unsynced = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        moved(from, a, to, b).map(|_| Moved::Unsynced(std::io::Error::other("no sync")))
    };
    let cut = install_moving(
        &homes.home,
        &id,
        &source,
        "Probe.app",
        &|| Ok(()),
        &unsynced,
        &|_| false,
    )
    .unwrap();
    said.push(shape(&cut, &homes, &id, None));

    assert_eq!(
        said,
        [
            r#"{"app":"Probe","bundleId":"test.atlas.install-probe","installId":"ID","installed":true,"ok":true,"path":"HOME/Applications/Probe.app","registered":true,"version":"1.0 (1)"}"#,
            r#"{"installs":[{"app":"Probe","bundleId":"test.atlas.install-probe","installId":"ID","path":"HOME/Applications/Probe.app","phase":"installed","present":true,"processes":[{"executable":"HOME/Applications/Probe.app/Contents/MacOS/Probe","pid":812}],"root":"ROOT","unchanged":true,"unproven":[{"executable":"/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe","pid":900}],"verified":true},{"installId":"MISSING","phase":"missing"}],"ok":true}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"inUse","sideEffects":{"kept":[]}}"#,
            r#"{"app":"Probe","heldPath":"HOME/.config/atlas/installs/held/ID/Probe.app","installId":"ID","ok":true,"outcome":"held","sideEffects":{"kept":[{"existedBefore":false,"path":"Library/Preferences/test.atlas.install-probe.plist"}]}}"#,
            r#"{"installs":[{"app":"Probe","bundleId":"test.atlas.install-probe","installId":"ID","phase":"held","verified":false}],"ok":true}"#,
            r#"{"installId":"ID","ok":true,"outcome":"alreadyGone"}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"changed","sideEffects":{"kept":[]}}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"replaced","sideEffects":{"kept":[]}}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"other-volume","sideEffects":{"kept":[]}}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"kept","reason":"other-volume","sideEffects":{"kept":[]}}"#,
            r#"{"app":"Probe","installId":"ID","ok":true,"outcome":"alreadyGone","sideEffects":{"kept":[]}}"#,
            r#"{"installId":"ID","ok":true,"outcome":"kept","reason":"unreadable"}"#,
            r#"{"installs":[{"installId":"ID","phase":"unreadable","verified":false}],"ok":true}"#,
            r#"{"error":"no sync","installId":"ID","ok":false,"phase":"placing"}"#,
        ]
    );
}
