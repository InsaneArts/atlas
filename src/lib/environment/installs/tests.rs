use super::*;
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
        remove(&self.home, id, &|| Ok(()), &|| Ok(Vec::new())).unwrap()
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

    let print = print(&destination, Some("Probe")).unwrap();
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
    record.phase = InstallPhase::Removing;
    homes.store().save(&record).unwrap();
    assert_eq!(homes.remove(&inside)["outcome"], "removed");

    // Cut while removing, before it moved: every check runs again.
    let unmoved = id();
    homes.install(&unmoved, "Probe.app").unwrap();
    let mut record = homes.record(&unmoved);
    record.phase = InstallPhase::Removing;
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

    let reply = remove(&homes.home, &id, &|| Ok(()), &processes).unwrap();
    assert_eq!(reply["outcome"], "inUse");
    assert!(executable.exists());
    assert_eq!(homes.record(&id).phase, InstallPhase::Installed);

    let listed = installs(
        &homes.home,
        std::slice::from_ref(&id),
        &|| Ok(()),
        &processes,
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
    let small = Limits {
        entries: 3,
        bytes: MAX_BYTES,
    };
    let error = print_within(&app, Some("Probe"), small).unwrap_err();
    assert_eq!(error.code, "input");
    let few = Limits {
        entries: MAX_ENTRIES,
        bytes: 4,
    };
    assert_eq!(
        print_within(&app, Some("Probe"), few).unwrap_err().code,
        "input"
    );
    assert!(print(&app, Some("Probe")).is_ok());
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
