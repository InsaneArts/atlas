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

    fn store(&self) -> Store {
        Store::open(&self.home, true).unwrap().unwrap()
    }

    fn record(&self, id: &str) -> Install {
        self.store().load(id).unwrap().unwrap()
    }

    /// Install `name` from outside as install `id`.
    fn install(&self, id: &str, name: &str) -> Result<Value> {
        let source = self.app(name);
        install(&self.home, id, &source, name, &|| Ok(()))
    }

    fn remove(&self, id: &str) -> Value {
        remove(&self.home, id, &|| Ok(()), &looks(&|| Ok(Vec::new()))).unwrap()
    }

    /// [`Homes::remove`], moving with `mover`.
    fn remove_with(&self, id: &str, mover: Mover) -> Value {
        remove_moving(
            &self.home,
            id,
            &|| Ok(()),
            &looks(&|| Ok(Vec::new())),
            mover,
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

    /// Where a removal moves install `id`'s bundle aside, its folder made.
    fn aside(&self, id: &str) -> PathBuf {
        let folder = self.store().folder(id).join("removing");
        fs::create_dir_all(&folder).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700)).unwrap();
        folder.join("Probe.app")
    }
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
    crate::control::token()
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
fn an_app_already_in_applications_is_never_installed_over_or_removed() {
    let homes = Homes::new();
    fs::create_dir(homes.home.join(APPLICATIONS)).unwrap();
    let file = homes.destination("File.app");
    fs::write(&file, "a file").unwrap();
    let folder = homes.destination("Folder.app");
    fs::create_dir(&folder).unwrap();
    let link = homes.destination("Link.app");
    symlink(&homes.outside, &link).unwrap();
    let other = bundle(&homes.destination("Other.app"));
    let before = contents(&homes.home.join(APPLICATIONS));

    for name in ["File.app", "Folder.app", "Link.app", "Other.app"] {
        let id = id();
        let error = homes.install(&id, name).unwrap_err();
        assert_eq!(error.code, "input", "{name}");
        assert!(error.error.contains("already has"), "{error}");
        assert!(
            homes.store().load(&id).unwrap().is_none(),
            "no record for {name}"
        );
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
fn a_replaced_bundle_is_kept() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let destination = homes.destination("Probe.app");
    fs::remove_dir_all(&destination).unwrap();
    bundle(&destination);

    let reply = homes.remove(&id);
    assert_eq!(reply["reason"], "replaced");
    assert!(destination.exists());
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
    assert_eq!(homes.remove(&id)["outcome"], "removed");
    assert!(!destination.exists());
}

#[test]
fn an_install_cut_at_each_step_resumes_or_cleans_its_own_staging() {
    let staged = |homes: &Homes, id: &str| {
        let staging = homes.store().folder(id).join("staging");
        fs::create_dir_all(&staging).unwrap();
        staging.join("Probe.app")
    };
    let removing = |homes: &Homes, id: &str| homes.store().folder(id).join("removing/Probe.app");

    // Cut after copying: only a staged copy, never printed.
    let homes = Homes::new();
    let cut = id();
    homes.install(&cut, "Probe.app").unwrap();
    fs::rename(homes.destination("Probe.app"), staged(&homes, &cut)).unwrap();
    let mut record = homes.record(&cut);
    record.phase = InstallPhase::Copying;
    record.print = None;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&cut)["outcome"], "alreadyGone");

    // Cut after printing, before the move.
    let before_move = id();
    homes.install(&before_move, "Probe.app").unwrap();
    fs::rename(homes.destination("Probe.app"), staged(&homes, &before_move)).unwrap();
    let mut record = homes.record(&before_move);
    record.phase = InstallPhase::Placing;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&before_move)["outcome"], "alreadyGone");

    // Cut after the move, before it was written installed.
    let after_move = id();
    homes.install(&after_move, "Probe.app").unwrap();
    let mut record = homes.record(&after_move);
    record.phase = InstallPhase::Placing;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&after_move)["outcome"], "removed");

    // Cut while removing: moved out of place, not yet deleted.
    let inside = id();
    homes.install(&inside, "Probe.app").unwrap();
    fs::create_dir_all(removing(&homes, &inside).parent().unwrap()).unwrap();
    fs::rename(homes.destination("Probe.app"), removing(&homes, &inside)).unwrap();
    let mut record = homes.record(&inside);
    record.phase = InstallPhase::Moving;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&inside)["outcome"], "removed");

    // Cut while removing, before it moved: every check runs again.
    let unmoved = id();
    homes.install(&unmoved, "Probe.app").unwrap();
    let mut record = homes.record(&unmoved);
    record.phase = InstallPhase::Moving;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&unmoved)["outcome"], "removed");

    for id in [&cut, &before_move, &after_move, &inside, &unmoved] {
        assert_eq!(homes.record(id).phase, InstallPhase::Removed);
        assert!(!staged(&homes, id).exists() && !removing(&homes, id).exists());
    }
    assert!(!homes.destination("Probe.app").exists());
    assert!(homes.home.join(APPLICATIONS).is_dir(), "never removed");
    assert!(homes.outside.join("Probe.app").exists(), "the source stays");
}

#[test]
fn a_bundle_a_process_runs_from_is_not_removed() {
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

    let listed = installs(
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
fn the_bundle_identifier_alone_never_selects_anything() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let twin = bundle(&homes.home.join("Desktop/Probe.app"));
    let other = bundle(&homes.destination("Probe Copy.app"));

    assert_eq!(homes.remove(&id)["outcome"], "removed");
    assert!(twin.join("Contents/MacOS/Probe").exists());
    assert!(other.join("Contents/MacOS/Probe").exists());
}

#[test]
fn side_effect_files_are_reported_and_never_removed() {
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
    assert_eq!(reply["outcome"], "removed");
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
    install(&homes.home, &id, &source, "Probe.app", &|| Ok(())).unwrap();

    assert_eq!(homes.remove(&id)["outcome"], "removed");
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

/// The real move.
fn moved(
    from_dir: &fs::File,
    from: &FileName,
    to_dir: &fs::File,
    to: &FileName,
) -> std::io::Result<Moved> {
    move_between(from_dir, from, to_dir, to)
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
fn a_foreign_folder_found_moved_aside_is_put_back_never_deleted() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    // The bundle Atlas placed is kept under another name, so the one made
    // in its place cannot reuse its node.
    let destination = homes.destination("Probe.app");
    let original = homes.destination("Probe.app.original");
    fs::rename(&destination, &original).unwrap();
    let foreign = bundle(&homes.aside(&id));
    fs::write(foreign.join("Contents/Resources/mine.txt"), "someone's").unwrap();
    homes.cut(&id, InstallPhase::Moving);

    let reply = homes.remove(&id);
    assert_eq!(
        (&reply["outcome"], &reply["reason"]),
        (&json!("kept"), &json!("replaced"))
    );
    assert_eq!(
        fs::read_to_string(destination.join("Contents/Resources/mine.txt")).unwrap(),
        "someone's",
        "put back where it was, whole"
    );
    assert!(!homes.aside(&id).exists());
    assert!(original.exists());
}

#[test]
fn a_removal_cut_after_the_move_checks_what_was_moved_before_deleting() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    // The move happens, but is not written through, as a crash leaves it.
    let unsynced = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        moved(from, a, to, b).map(|_| Moved::Unsynced(std::io::Error::other("no sync")))
    };

    let first = homes.remove_with(&id, &unsynced);
    assert_eq!(first["ok"], false, "{first}");
    assert_eq!(homes.record(&id).phase, InstallPhase::Moving);
    assert!(
        homes.aside(&id).join("Contents/MacOS/Probe").exists(),
        "nothing deleted yet"
    );
    assert!(!homes.destination("Probe.app").exists());

    let second = homes.remove(&id);
    assert_eq!(second["outcome"], "removed");
    assert!(!homes.aside(&id).exists());
    assert_eq!(homes.record(&id).phase, InstallPhase::Removed);
}

#[test]
fn a_bundle_that_changed_as_it_was_moved_is_put_back_and_kept() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let aside = homes.aside(&id);
    // An updater rewrites a resource just as the bundle is moved.
    let rewriting = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        let result = moved(from, a, to, b);
        if result.is_ok() && aside.exists() {
            fs::write(aside.join("Contents/Resources/strings.txt"), "updated").unwrap();
        }
        result
    };

    let reply = homes.remove_with(&id, &rewriting);
    assert_eq!(reply["reason"], "changed", "{reply}");
    let destination = homes.destination("Probe.app");
    assert_eq!(
        fs::read_to_string(destination.join("Contents/Resources/strings.txt")).unwrap(),
        "updated"
    );
    assert!(reply.get("heldAside").is_none());
}

#[test]
fn a_changed_bundle_that_cannot_be_put_back_is_held_aside_not_deleted() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let aside = homes.aside(&id);
    let calls = std::cell::Cell::new(0);
    let failing_back = |from: &fs::File, a: &FileName, to: &fs::File, b: &FileName| {
        calls.set(calls.get() + 1);
        if calls.get() > 1 {
            return Err(std::io::Error::other("cannot move back"));
        }
        let result = moved(from, a, to, b);
        fs::write(aside.join("Contents/Resources/strings.txt"), "updated").unwrap();
        result
    };

    let reply = homes.remove_with(&id, &failing_back);
    assert_eq!(
        (&reply["reason"], &reply["heldAside"]),
        (&json!("changed"), &json!(true))
    );
    assert!(aside.join("Contents/MacOS/Probe").exists(), "kept, aside");
    let record = homes.record(&id);
    assert!(record.held_aside && record.phase == InstallPhase::Kept);
    assert_eq!(homes.remove(&id)["outcome"], "kept", "never deleted later");
    assert!(aside.exists());
}

#[test]
fn a_deletion_cut_short_goes_on_only_for_the_folder_it_checked() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let original = homes.destination("Probe.app.original");
    fs::rename(homes.destination("Probe.app"), &original).unwrap();
    let foreign = bundle(&homes.aside(&id));
    homes.cut(&id, InstallPhase::Deleting);

    let reply = homes.remove(&id);
    assert_eq!(
        (&reply["reason"], &reply["heldAside"]),
        (&json!("replaced"), &json!(true))
    );
    assert!(foreign.join("Contents/MacOS/Probe").exists());
}

#[test]
fn a_deletion_that_fails_stays_retryable_and_is_never_taken_for_removed() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let aside = homes.aside(&id);
    fs::rename(homes.destination("Probe.app"), &aside).unwrap();
    homes.cut(&id, InstallPhase::Deleting);
    let locked = aside.join("Contents/Resources");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();

    let failed = remove(&homes.home, &id, &|| Ok(()), &looks(&|| Ok(Vec::new())));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        failed.is_err() || failed.as_ref().unwrap()["ok"] == false,
        "{failed:?}"
    );
    assert_eq!(homes.record(&id).phase, InstallPhase::Deleting);

    assert_eq!(homes.remove(&id)["outcome"], "removed");
    assert!(!aside.exists());
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
        installs(
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

#[test]
fn a_translocated_process_macos_cannot_trace_keeps_the_install() {
    let homes = Homes::new();
    let id = id();
    homes.install(&id, "Probe.app").unwrap();
    let translocated =
        "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe"
            .to_owned();
    let processes = move || Ok(vec![(900, translocated.clone())]);
    let ours = |_: &Path| Some(BUNDLE_ID.to_owned());
    let looks = Looks {
        processes: &processes,
        original: &|_| None,
        bundle_id: &ours,
    };

    let reply = remove(&homes.home, &id, &|| Ok(()), &looks).unwrap();
    assert_eq!(reply["outcome"], "inUse");
    let listed = installs(&homes.home, std::slice::from_ref(&id), &|| Ok(()), &looks).unwrap();
    assert_eq!(
        listed["installs"][0]["processes"],
        json!([]),
        "never signalled"
    );
    assert_eq!(listed["installs"][0]["unproven"][0]["pid"], 900);

    let destination = homes.destination("Probe.app");
    let traced = move |_: &Path| Some(destination.clone());
    let looks = Looks {
        original: &traced,
        ..looks
    };
    let listed = installs(&homes.home, std::slice::from_ref(&id), &|| Ok(()), &looks).unwrap();
    assert_eq!(
        listed["installs"][0]["processes"][0]["pid"], 900,
        "traced to it"
    );
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
    assert!(homes.store().load(&missing).unwrap().is_none());

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
