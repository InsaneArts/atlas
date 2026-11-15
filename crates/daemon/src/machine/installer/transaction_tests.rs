use super::*;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Signatures, launchd and the clock as a test needs them: a bundle is
/// Atlas's when its `atlas` does not say `unsigned`, and its code directory
/// hash is the digest of that file.
#[derive(Default)]
struct Fake {
    now: AtomicU64,
    restarts: AtomicUsize,
    refusing_restarts: std::sync::atomic::AtomicBool,
    /// What the machine job runs now, as `job_code` says it.
    job: std::sync::Mutex<Option<Vec<u8>>>,
}

impl System for Fake {
    fn verified(&self, bundle: &Path) -> Option<Vec<u8>> {
        let bytes = std::fs::read(bundle.join("Contents/MacOS/atlas")).ok()?;
        (bytes != b"unsigned").then(|| Sha256::digest(&bytes)[..20].to_vec())
    }

    fn restart(&self, _uid: u32) -> bool {
        self.restarts.fetch_add(1, Ordering::SeqCst);
        !self.refusing_restarts.load(Ordering::SeqCst)
    }

    fn job_code(&self, _uid: u32) -> Option<Vec<u8>> {
        self.job.lock().unwrap().clone()
    }

    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

/// A guest with an Atlas installed, the installer, and a new Atlas staged.
struct Guest {
    root: tempfile::TempDir,
    staged: tempfile::TempDir,
    fake: Arc<Fake>,
    installer: Installer,
}

impl Guest {
    fn new(program: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("Applications/Atlas.app/Contents/MacOS");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("atlas"), "old").unwrap();
        let staged = tempfile::tempdir().unwrap();
        let bundle = staged.path().join("Atlas.app/Contents/MacOS");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("atlas"), program).unwrap();
        let fake = Arc::new(Fake::default());
        let installer = Installer::new(Layout::under(root.path()), fake.clone()).unwrap();
        Self {
            root,
            staged,
            fake,
            installer,
        }
    }

    fn installed(&self) -> String {
        std::fs::read_to_string(
            self.root
                .path()
                .join("Applications/Atlas.app/Contents/MacOS/atlas"),
        )
        .unwrap()
    }

    fn install(&self, request: &Token, transaction: &Token) -> Value {
        let source = self.staged.path().join("Atlas.app");
        let manifest = Snapshot::of(&source).unwrap();
        self.installer
            .install(request, transaction, 501, &source, &manifest)
            .unwrap()
    }

    fn record(&self, transaction: &Token) -> Transaction {
        Transaction::read(&self.installer.record(transaction))
            .unwrap()
            .unwrap()
    }

    /// Put `transaction`'s record back at `phase`, as a crash there leaves it.
    fn crashed_at(&self, transaction: &Token, phase: Phase) {
        let mut kept = self.record(transaction);
        kept.phase = phase;
        kept.write(&self.installer.record(transaction)).unwrap();
    }

    /// Swap the installed and held bundles, as a swap the record never saw.
    fn swap(&self, transaction: &Token) {
        let applications = File::open(self.root.path().join("Applications")).unwrap();
        let into = File::open(self.installer.folder(transaction)).unwrap();
        exchange_between(&applications, &name(BUNDLE), &into, &name(HELD)).unwrap();
    }

    fn cdhash(&self, program: &str) -> Vec<u8> {
        Sha256::digest(program.as_bytes())[..20].to_vec()
    }
}

#[test]
fn an_update_is_placed_whole_with_the_previous_bundle_kept_and_asked_again_answered_from_its_record()
 {
    let guest = Guest::new("new");
    let (request, transaction) = (Token::new(), Token::new());

    let placed = guest.install(&request, &transaction);

    assert_eq!(placed["phase"], "placed", "{placed}");
    assert_eq!(guest.installed(), "new");
    let held = guest
        .installer
        .folder(&transaction)
        .join("Atlas.app/Contents/MacOS/atlas");
    assert_eq!(std::fs::read_to_string(held).unwrap(), "old");
    assert_eq!(guest.install(&request, &transaction), placed);
    assert_eq!(guest.installer.outcome(&request).unwrap(), placed);
    let other = guest.install(&Token::new(), &Token::new());
    assert_eq!(other["ok"], false, "one update open at a time: {other}");
}

#[test]
fn only_a_daemon_of_the_new_build_releases_its_update_and_then_the_previous_bundle_goes() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);

    let old = guest
        .installer
        .release(&Token::new(), &transaction, &guest.cdhash("old"))
        .unwrap();
    assert_eq!(old["ok"], false, "{old}");

    let released = guest
        .installer
        .release(&Token::new(), &transaction, &guest.cdhash("new"))
        .unwrap();
    assert_eq!(released["phase"], "released", "{released}");
    assert_eq!(guest.installed(), "new");
    assert!(
        !guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists()
    );
    let program = guest
        .root
        .path()
        .join("Library/Application Support/Atlas/Installer/Atlas.app/Contents/MacOS/atlas");
    assert_eq!(
        std::fs::read_to_string(program).unwrap(),
        "new",
        "the installer's own copy follows"
    );
    assert!(
        !guest
            .root
            .path()
            .join("Library/Application Support/Atlas/Installer/Atlas.app.incoming")
            .exists(),
        "the copy it replaced is gone"
    );
    let late = guest
        .installer
        .rollback(&Token::new(), &transaction)
        .unwrap();
    assert_eq!(
        late["ok"], false,
        "a released update is never rolled back: {late}"
    );
}

#[test]
fn a_rollback_puts_the_previous_bundle_back_and_starts_its_daemon_again() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);

    let rolled = guest
        .installer
        .rollback(&Token::new(), &transaction)
        .unwrap();

    assert_eq!(rolled["phase"], "rolledBack", "{rolled}");
    assert_eq!(guest.installed(), "old");
    assert!(
        !guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists()
    );
    // Started once on the new build as it was placed, and once on the
    // previous one as it was put back.
    assert_eq!(guest.fake.restarts.load(Ordering::SeqCst), 2);
}

#[test]
fn a_placed_update_no_daemon_releases_in_time_is_rolled_back_without_being_asked() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);

    guest.installer.deadline().unwrap();
    assert_eq!(
        guest.record(&transaction).phase,
        Phase::Placed,
        "not yet due"
    );
    guest.fake.now.store(DEADLINE_MS, Ordering::SeqCst);
    guest.installer.deadline().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::RolledBack);
    assert_eq!(guest.installed(), "old");
    // Started once on the new build as it was placed, and once on the
    // previous one as it was put back.
    assert_eq!(guest.fake.restarts.load(Ordering::SeqCst), 2);
}

#[test]
fn a_bundle_that_is_not_atlas_signed_is_never_placed() {
    let guest = Guest::new("unsigned");
    let transaction = Token::new();

    let refused = guest.install(&Token::new(), &transaction);

    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(guest.record(&transaction).phase, Phase::Aborted);
    assert_eq!(guest.installed(), "old");
}

#[test]
fn a_request_id_used_for_another_request_is_refused() {
    let guest = Guest::new("new");
    let (request, transaction) = (Token::new(), Token::new());
    guest.install(&request, &transaction);

    let reused = guest.installer.rollback(&request, &transaction).unwrap();

    assert_eq!(reused["ok"], false, "{reused}");
    assert_eq!(guest.installed(), "new");
}

#[test]
fn a_crash_before_the_swap_leaves_the_installed_atlas_as_it_was() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    // As if the swap never happened.
    guest.swap(&transaction);
    guest.crashed_at(&transaction, Phase::Swapping);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::Aborted);
    assert_eq!(guest.installed(), "old");
}

#[test]
fn a_crash_after_the_swap_before_its_check_is_rolled_back_never_finished_blind() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.crashed_at(&transaction, Phase::Swapping);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::RolledBack);
    assert_eq!(guest.installed(), "old");
}

#[test]
fn a_crash_after_a_failed_check_swapped_back_finishes_the_rollback() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.swap(&transaction);
    guest.crashed_at(&transaction, Phase::PlacedUnverified);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::RolledBack);
    assert_eq!(guest.installed(), "old");
    assert!(
        !guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists()
    );
}

#[test]
fn a_release_cut_short_is_finished_by_recovery() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.crashed_at(&transaction, Phase::Releasing);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::Released);
    assert_eq!(guest.installed(), "new");
    assert!(
        !guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists()
    );
}

#[test]
fn an_unexpected_atlas_in_place_leaves_a_rollback_pending_and_removes_nothing() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    let installed = guest.root.path().join("Applications/Atlas.app");
    std::fs::rename(&installed, guest.root.path().join("elsewhere")).unwrap();
    std::fs::create_dir_all(installed.join("Contents/MacOS")).unwrap();
    std::fs::write(installed.join("Contents/MacOS/atlas"), "another").unwrap();

    let rolled = guest
        .installer
        .rollback(&Token::new(), &transaction)
        .unwrap();

    assert_eq!(rolled["phase"], "pending", "{rolled}");
    assert_eq!(guest.installed(), "another");
    assert!(
        guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists(),
        "the previous bundle kept"
    );
}

#[test]
fn a_crash_after_the_swap_was_kept_is_checked_again_and_stays_placed_when_it_holds() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.crashed_at(&transaction, Phase::PlacedUnverified);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::Placed);
    assert_eq!(guest.installed(), "new");
}

#[test]
fn a_rollback_whose_daemon_cannot_be_started_again_owes_it_and_tries_again() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.fake.refusing_restarts.store(true, Ordering::SeqCst);

    guest
        .installer
        .rollback(&Token::new(), &transaction)
        .unwrap();
    assert_eq!(guest.record(&transaction).phase, Phase::Restarting);
    assert_eq!(guest.installed(), "old");

    guest.fake.refusing_restarts.store(false, Ordering::SeqCst);
    guest.installer.deadline().unwrap();
    assert_eq!(guest.record(&transaction).phase, Phase::RolledBack);
    // Once as it was placed, then refused, then started.
    assert_eq!(guest.fake.restarts.load(Ordering::SeqCst), 3);
}

#[test]
fn a_request_id_is_bound_to_its_arguments_and_its_refusals_are_kept() {
    let guest = Guest::new("new");
    let (request, transaction) = (Token::new(), Token::new());
    guest.install(&request, &transaction);

    // Asked again with another transaction: refused, not taken for the first.
    let source = guest.staged.path().join("Atlas.app");
    let manifest = Snapshot::of(&source).unwrap();
    let reused = guest
        .installer
        .install(&request, &Token::new(), 501, &source, &manifest)
        .unwrap();
    assert_eq!(reused["ok"], false, "{reused}");

    // A refusal asked again is the same refusal.
    let second = Token::new();
    let refused = guest
        .installer
        .install(&second, &Token::new(), 501, &source, &manifest)
        .unwrap();
    assert_eq!(refused["ok"], false, "one update open at a time: {refused}");
    assert_eq!(guest.installer.outcome(&second).unwrap(), refused);
}

#[test]
fn a_release_kept_but_cut_short_before_its_answer_goes_on_when_asked_again() {
    let guest = Guest::new("new");
    let (request, transaction) = (Token::new(), Token::new());
    guest.install(&Token::new(), &transaction);
    // As if the installer went down after keeping the request.
    Asked {
        request: request.clone(),
        op: "release".into(),
        args: transaction.as_str().into(),
        transaction: Some(transaction.clone()),
        reply: None,
    }
    .write(&guest.installer.request_record(&request))
    .unwrap();

    let released = guest
        .installer
        .release(&request, &transaction, &guest.cdhash("new"))
        .unwrap();

    assert_eq!(released["phase"], "released", "{released}");
}

#[test]
fn a_release_never_drops_the_fallback_while_another_atlas_is_installed() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    let installed = guest.root.path().join("Applications/Atlas.app");
    std::fs::rename(&installed, guest.root.path().join("aside")).unwrap();
    std::fs::create_dir_all(installed.join("Contents/MacOS")).unwrap();
    std::fs::write(installed.join("Contents/MacOS/atlas"), "another").unwrap();

    let released = guest
        .installer
        .release(&Token::new(), &transaction, &guest.cdhash("new"))
        .unwrap();

    assert_eq!(released["phase"], "pending", "{released}");
    assert!(
        guest
            .installer
            .folder(&transaction)
            .join("Atlas.app")
            .exists(),
        "the fallback kept"
    );
}

#[test]
fn a_file_in_a_bundles_place_leaves_recovery_pending_and_serving() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    let installed = guest.root.path().join("Applications/Atlas.app");
    std::fs::rename(&installed, guest.root.path().join("aside")).unwrap();
    std::fs::write(&installed, "not a bundle").unwrap();
    guest.crashed_at(&transaction, Phase::RollingBack);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::Pending);
    assert_eq!(
        guest.installer.transaction(&transaction).unwrap()["phase"],
        "pending"
    );
}

#[test]
fn a_release_finished_by_recovery_is_answered_as_released_when_asked_again() {
    let guest = Guest::new("new");
    let (request, transaction) = (Token::new(), Token::new());
    guest.install(&Token::new(), &transaction);
    Asked {
        request: request.clone(),
        op: "release".into(),
        args: transaction.as_str().into(),
        transaction: Some(transaction.clone()),
        reply: None,
    }
    .write(&guest.installer.request_record(&request))
    .unwrap();
    guest.crashed_at(&transaction, Phase::Releasing);
    guest.installer.recover().unwrap();

    let again = guest
        .installer
        .release(&request, &transaction, &guest.cdhash("new"))
        .unwrap();

    assert_eq!(again["phase"], "released", "{again}");
}

#[test]
fn recovery_never_removes_what_it_does_not_hold() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.swap(&transaction);
    // Something else in its copy's place.
    let held = guest.installer.folder(&transaction).join("Atlas.app");
    std::fs::rename(&held, guest.root.path().join("away")).unwrap();
    std::fs::create_dir(&held).unwrap();
    guest.crashed_at(&transaction, Phase::Swapping);

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::Pending);
    assert!(held.exists());
}

#[test]
fn placing_owes_the_daemon_its_start_in_the_same_write_and_says_the_placed_code() {
    let guest = Guest::new("new");
    let transaction = Token::new();

    let placed = guest.install(&Token::new(), &transaction);

    let kept = guest.record(&transaction);
    assert_eq!(kept.phase, Phase::Placed);
    assert_eq!(kept.restart, Restart::Done);
    assert_eq!(guest.fake.restarts.load(Ordering::SeqCst), 1);
    assert_eq!(placed["cdhash"], hex(&guest.cdhash("new")), "{placed}");
    assert_eq!(placed["restart"], "done", "{placed}");
}

#[test]
fn a_start_owed_whose_daemon_already_runs_the_placed_code_is_never_made_again() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    // Placed, its start owed, as a start whose record was cut short leaves it.
    guest.fake.refusing_restarts.store(true, Ordering::SeqCst);
    guest.install(&Token::new(), &transaction);
    assert_eq!(guest.record(&transaction).restart, Restart::Owed);
    guest.fake.refusing_restarts.store(false, Ordering::SeqCst);
    *guest.fake.job.lock().unwrap() = Some(guest.cdhash("new"));

    guest.installer.recover().unwrap();

    assert_eq!(guest.record(&transaction).restart, Restart::Done);
    assert_eq!(
        guest.fake.restarts.load(Ordering::SeqCst),
        1,
        "only the refused start at placing"
    );
}

#[test]
fn a_rollback_whose_daemon_already_runs_the_previous_code_is_not_started_again() {
    let guest = Guest::new("new");
    let transaction = Token::new();
    guest.install(&Token::new(), &transaction);
    guest.fake.refusing_restarts.store(true, Ordering::SeqCst);
    guest
        .installer
        .rollback(&Token::new(), &transaction)
        .unwrap();
    assert_eq!(guest.record(&transaction).phase, Phase::Restarting);
    *guest.fake.job.lock().unwrap() = Some(guest.cdhash("old"));

    guest.installer.deadline().unwrap();

    assert_eq!(guest.record(&transaction).phase, Phase::RolledBack);
    assert_eq!(
        guest.fake.restarts.load(Ordering::SeqCst),
        2,
        "the start at placing and the refused one, no third"
    );
}
