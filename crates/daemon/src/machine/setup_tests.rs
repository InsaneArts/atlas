use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::PermissionsExt,
    sync::Mutex,
};

/// The guest's system as a test needs it: a bundle is Atlas's unless its
/// `atlas` says `unsigned`, and accounts live in memory.
#[derive(Default)]
struct Fake {
    accounts: Mutex<HashMap<String, Account>>,
    admins: Mutex<HashSet<String>>,
    launched: Mutex<Vec<String>>,
    passwords: Mutex<Vec<(String, String)>>,
    hidden: Mutex<HashSet<String>>,
}

impl Fake {
    fn with_account(name: &str, home: &Path) -> Self {
        let fake = Self::default();
        fake.accounts.lock().unwrap().insert(
            name.into(),
            Account {
                uid: 501,
                gid: 20,
                home: home.to_owned(),
                full_name: Some("Atlas".into()),
            },
        );
        fake.admins.lock().unwrap().insert(name.into());
        fake
    }
}

impl System for Fake {
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>> {
        let bytes = std::fs::read(bundle.join("Contents/MacOS/atlas")).ok()?;
        (bytes != b"unsigned").then(|| Sha256::digest(&bytes)[..20].to_vec())
    }

    fn launchctl(&self, arguments: &[&str]) -> bool {
        self.launched.lock().unwrap().push(arguments.join(" "));
        true
    }

    fn account(&self, name: &str) -> Option<Account> {
        self.accounts.lock().unwrap().get(name).cloned()
    }

    fn create_admin(&self, name: &str, full_name: &str, password: &str) -> bool {
        let uid = 502 + u32::try_from(self.accounts.lock().unwrap().len()).unwrap();
        self.accounts.lock().unwrap().insert(
            name.into(),
            Account {
                uid,
                gid: 20,
                home: PathBuf::from(format!("/Users/{name}")),
                full_name: Some(full_name.into()),
            },
        );
        self.admins.lock().unwrap().insert(name.into());
        self.passwords
            .lock()
            .unwrap()
            .push((name.into(), password.into()));
        true
    }

    fn set_password(&self, name: &str, password: &str) -> bool {
        self.passwords
            .lock()
            .unwrap()
            .push((name.into(), password.into()));
        true
    }

    fn is_admin(&self, name: &str) -> Option<bool> {
        Some(self.admins.lock().unwrap().contains(name))
    }

    fn set_admin(&self, name: &str, admin: bool) -> bool {
        let mut admins = self.admins.lock().unwrap();
        if admin {
            admins.insert(name.into());
        } else {
            admins.remove(name);
        }
        true
    }

    fn hide(&self, name: &str) -> bool {
        self.hidden.lock().unwrap().insert(name.into());
        true
    }

    fn close_remote_login(&self) -> bool {
        true
    }

    fn root(&self) -> bool {
        false
    }
}

/// A guest folder tree, and an Atlas staged by its account.
struct Guest {
    root: tempfile::TempDir,
    staged: tempfile::TempDir,
    places: Places,
}

impl Guest {
    fn new(program: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("Applications")).unwrap();
        let staged = tempfile::tempdir().unwrap();
        let macos = staged.path().join("Atlas.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("atlas"), program).unwrap();
        std::fs::set_permissions(macos.join("atlas"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let places = Places::under(root.path());
        Self {
            root,
            staged,
            places,
        }
    }

    fn source(&self) -> PathBuf {
        self.staged.path().join("Atlas.app")
    }

    fn installed(&self) -> String {
        std::fs::read_to_string(
            self.root
                .path()
                .join("Applications/Atlas.app/Contents/MacOS/atlas"),
        )
        .unwrap()
    }

    fn install_old(&self, program: &str) {
        let macos = self
            .root
            .path()
            .join("Applications/Atlas.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("atlas"), program).unwrap();
    }
}

#[test]
fn an_atlas_staged_by_the_guest_account_is_published_whole_and_asked_again_found_placed() {
    let guest = Guest::new("new");
    let change = Token::new();

    let published =
        publish::publish(&guest.places, &Fake::default(), &change, &guest.source()).unwrap();

    assert_eq!(published["published"], "placed", "{published}");
    assert_eq!(guest.installed(), "new");
    let again =
        publish::publish(&guest.places, &Fake::default(), &change, &guest.source()).unwrap();
    assert_eq!(again["published"], "placed");
}

#[test]
fn an_atlas_of_other_code_is_swapped_out_and_kept_and_one_of_the_same_code_is_kept() {
    let guest = Guest::new("new");
    guest.install_old("old");
    let change = Token::new();

    publish::publish(&guest.places, &Fake::default(), &change, &guest.source()).unwrap();

    assert_eq!(guest.installed(), "new");
    let kept = guest
        .places
        .bootstrap()
        .join(change.as_str())
        .join("Atlas.app/Contents/MacOS/atlas");
    assert_eq!(std::fs::read_to_string(kept).unwrap(), "old");

    let same = publish::publish(
        &guest.places,
        &Fake::default(),
        &Token::new(),
        &guest.source(),
    )
    .unwrap();
    assert_eq!(same["published"], "kept", "{same}");
    assert_eq!(guest.installed(), "new");
}

#[test]
fn an_atlas_that_is_not_atlas_signed_is_never_published() {
    let guest = Guest::new("unsigned");

    let refused = publish::publish(
        &guest.places,
        &Fake::default(),
        &Token::new(),
        &guest.source(),
    );

    assert!(refused.is_err());
    assert!(!guest.root.path().join("Applications/Atlas.app").exists());
}

#[test]
fn a_publish_cut_short_before_its_placing_is_placed_when_asked_again() {
    let guest = Guest::new("new");
    let change = Token::new();
    publish::publish(&guest.places, &Fake::default(), &change, &guest.source()).unwrap();
    // As if it had stopped after keeping "placing", before the rename.
    let folder = guest.places.bootstrap().join(change.as_str());
    std::fs::rename(
        guest.root.path().join("Applications/Atlas.app"),
        folder.join("Atlas.app"),
    )
    .unwrap();
    let mut kept = record::Bootstrap::read(&guest.places).unwrap();
    kept.publish.as_mut().unwrap().phase = record::Publishing::Placing;
    kept.write(&guest.places).unwrap();

    let placed =
        publish::publish(&guest.places, &Fake::default(), &change, &guest.source()).unwrap();

    assert_eq!(placed["published"], "placed", "{placed}");
    assert_eq!(guest.installed(), "new");
}

#[test]
fn the_jobs_are_written_as_the_machine_launch_check_reads_them_and_started_in_their_domains() {
    let guest = Guest::new("new");
    guest.install_old("new");
    let home = guest.root.path().join("Users/atlas");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(guest.places.share()).unwrap();
    std::fs::write(guest.places.share().join("pairing-secret"), "secret").unwrap();
    let fake = Fake::with_account("atlas", &home);
    let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();
    let enrolment = Token::new();

    let found = jobs::jobs(&guest.places, &fake, &desktop, &enrolment, "atlas", false).unwrap();

    assert_eq!(found["secret"], "copied", "{found}");
    let agent = guest
        .places
        .agents()
        .join("com.insanearts.atlas.machine.plist");
    let json = atlas_core::spawn::output(
        std::process::Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&agent),
    )
    .unwrap();
    let plist: crate::machine::launch::Plist = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(plist.label, crate::machine::launch::LABEL);
    assert_eq!(
        plist.program_arguments,
        [
            "/Applications/Atlas.app/Contents/MacOS/atlas",
            "daemon",
            "--machine",
            "--desktop-id",
            desktop.as_str(),
            "--enrolment",
            enrolment.as_str(),
        ]
    );
    let secret = home.join("Library/Application Support/Atlas/Machine/pairing-secret");
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret");
    assert_eq!(
        std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::read_to_string(
            guest
                .places
                .installer()
                .join("Atlas.app/Contents/MacOS/atlas")
        )
        .unwrap(),
        "new",
        "the installer runs its own whole copy of the published Atlas"
    );
    assert!(!guest.places.installer().join("Making").exists());
    let launched = fake.launched.lock().unwrap().clone();
    assert!(
        launched
            .iter()
            .any(|call| call.starts_with("bootstrap gui/501 ")),
        "{launched:?}"
    );
    assert!(
        launched
            .iter()
            .any(|call| call.starts_with("bootstrap system ")),
        "{launched:?}"
    );
}

#[test]
fn the_jobs_need_a_pairing_secret_unless_the_pairing_is_complete() {
    let guest = Guest::new("new");
    guest.install_old("new");
    let home = guest.root.path().join("Users/atlas");
    let fake = Fake::with_account("atlas", &home);
    let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();

    assert!(
        jobs::jobs(
            &guest.places,
            &fake,
            &desktop,
            &Token::new(),
            "atlas",
            false
        )
        .is_err()
    );
    let paired = jobs::jobs(&guest.places, &fake, &desktop, &Token::new(), "atlas", true).unwrap();
    assert_eq!(paired["secret"], "paired");
}

#[test]
fn a_link_in_the_guest_accounts_folders_is_never_followed_with_its_secret() {
    let guest = Guest::new("new");
    guest.install_old("new");
    let home = guest.root.path().join("Users/atlas");
    let support = home.join("Library/Application Support");
    std::fs::create_dir_all(&support).unwrap();
    let elsewhere = guest.root.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, support.join("Atlas")).unwrap();
    std::fs::create_dir_all(guest.places.share()).unwrap();
    std::fs::write(guest.places.share().join("pairing-secret"), "secret").unwrap();
    let fake = Fake::with_account("atlas", &home);
    let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();

    let handed = jobs::jobs(
        &guest.places,
        &fake,
        &desktop,
        &Token::new(),
        "atlas",
        false,
    );

    assert!(handed.is_err(), "{handed:?}");
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn a_pairing_secret_that_is_a_link_or_a_pipe_is_refused_never_waited_on() {
    for make in ["link", "pipe"] {
        let guest = Guest::new("new");
        guest.install_old("new");
        let home = guest.root.path().join("Users/atlas");
        std::fs::create_dir_all(guest.places.share()).unwrap();
        let secret = guest.places.share().join("pairing-secret");
        match make {
            "link" => std::os::unix::fs::symlink("/etc/hosts", &secret).unwrap(),
            _ => nix::unistd::mkfifo(&secret, nix::sys::stat::Mode::from_bits_truncate(0o600))
                .unwrap(),
        }
        let fake = Fake::with_account("atlas", &home);
        let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();

        assert!(
            jobs::jobs(
                &guest.places,
                &fake,
                &desktop,
                &Token::new(),
                "atlas",
                false
            )
            .is_err(),
            "{make}"
        );
    }
}

#[test]
fn the_second_administrator_is_made_with_its_password_given_and_asked_again_is_finished() {
    let guest = Guest::new("new");
    let fake = Fake::default();

    account::admin(&guest.places, &fake, "atlas-admin", "passw0rd").unwrap();

    assert_eq!(fake.is_admin("atlas-admin"), Some(true));
    assert!(fake.hidden.lock().unwrap().contains("atlas-admin"));
    // Its membership lost, as a creation cut short might leave it: finished.
    fake.set_admin("atlas-admin", false);
    account::admin(&guest.places, &fake, "atlas-admin", "passw0rd").unwrap();
    assert_eq!(fake.is_admin("atlas-admin"), Some(true));
}

#[test]
fn an_account_this_setup_did_not_make_is_never_taken_over() {
    let guest = Guest::new("new");
    let fake = Fake::with_account("atlas-admin", Path::new("/Users/atlas-admin"));

    let refused = account::admin(&guest.places, &fake, "atlas-admin", "passw0rd");

    assert!(refused.is_err());
    assert!(
        fake.passwords.lock().unwrap().is_empty(),
        "its password was never set"
    );
}

#[test]
fn an_account_made_before_its_record_said_so_is_known_by_its_full_name() {
    let guest = Guest::new("new");
    let fake = Fake::default();
    // Kept as being made, then made, then the setup went before keeping its UID.
    record::Bootstrap {
        publish: None,
        admin: Some(record::Admin {
            name: "atlas-admin".into(),
            uid: None,
        }),
    }
    .write(&guest.places)
    .unwrap();
    fake.create_admin("atlas-admin", account::FULL_NAME, "passw0rd");

    account::admin(&guest.places, &fake, "atlas-admin", "passw0rd").unwrap();

    let kept = record::Bootstrap::read(&guest.places).unwrap();
    assert!(kept.admin.unwrap().uid.is_some());
}

#[test]
fn the_guest_account_is_demoted_and_checked_out_of_the_administrators() {
    let fake = Fake::with_account("atlas", Path::new("/Users/atlas"));

    account::demote(&fake, "atlas").unwrap();

    assert_eq!(fake.is_admin("atlas"), Some(false));
}

#[test]
fn a_password_comes_from_the_line_after_sudos() {
    assert_eq!(
        account::password_line(&mut "s3cret\n".as_bytes()).unwrap(),
        "s3cret"
    );
    assert!(account::password_line(&mut "\n".as_bytes()).is_err());
}
