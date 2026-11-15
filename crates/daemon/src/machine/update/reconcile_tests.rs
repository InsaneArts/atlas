use super::*;
use crate::machine::{
    commitment::create,
    update::{Health, STORE, Updates},
    vault::Memory,
};
use atlas_core::files::Token;
use std::sync::Mutex;

/// The installer and the guest as a test scripts them: each op answered
/// as told, every request kept.
struct Fake {
    replies: Mutex<std::collections::HashMap<&'static str, Value>>,
    asked: Mutex<Vec<Value>>,
    own: Vec<u8>,
    input: Mutex<(bool, Health)>,
    capture: Mutex<Health>,
    serving: std::sync::atomic::AtomicBool,
}

impl Fake {
    fn new() -> Self {
        Self {
            replies: Mutex::default(),
            asked: Mutex::default(),
            own: vec![0xab; 20],
            input: Mutex::new((true, Health::Permission)),
            capture: Mutex::new(Health::Ready),
            serving: std::sync::atomic::AtomicBool::new(true),
        }
    }

    fn answer(&self, op: &'static str, reply: Value) {
        self.replies.lock().unwrap().insert(op, reply);
    }

    fn ops(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|asked| asked["op"].as_str().unwrap_or_default().to_owned())
            .collect()
    }
}

impl System for Fake {
    fn ask(&self, request: &Value) -> Result<Value> {
        self.asked.lock().unwrap().push(request.clone());
        let op = request["op"].as_str().unwrap_or_default();
        self.replies
            .lock()
            .unwrap()
            .get(op)
            .cloned()
            .ok_or_else(|| {
                atlas_core::error::Error::new(atlas_core::error::Code::Transport, "no answer")
            })
    }

    fn own_code(&self) -> Option<Vec<u8>> {
        Some(self.own.clone())
    }

    fn input(&self) -> (bool, Health) {
        *self.input.lock().unwrap()
    }

    fn capture(&self) -> Health {
        *self.capture.lock().unwrap()
    }

    fn serving(&self) -> bool {
        self.serving.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// A guest with one update asked, its reconciler and its records.
struct Guest {
    folder: tempfile::TempDir,
    updates: Arc<Updates<Arc<Memory>>>,
    changes: Arc<Changes<Arc<Memory>>>,
    fake: Arc<Fake>,
    reconciler: Arc<Reconciler<Arc<Memory>>>,
    update: Update,
}

impl Guest {
    fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let vault = Arc::new(Memory::default());
        create(&vault, STORE).unwrap();
        create(&vault, crate::machine::changes::STORE).unwrap();
        let updates = Arc::new(Updates::new(
            folder.path().join("updates.json"),
            vault.clone(),
        ));
        let changes = Arc::new(Changes::new(folder.path().join("changes"), vault));
        std::fs::create_dir_all(folder.path().join("changes")).unwrap();
        let update = Update {
            change: Token::new(),
            source: "/Users/atlas/staged/Atlas.app".into(),
            manifest: Some(atlas_core::stage::Snapshot::of(folder.path()).unwrap()),
            transaction: Token::new(),
            install: Token::new(),
            release: Token::new(),
            input: Health::Permission,
            capture: Health::Ready,
            phase: Phase::Asked,
            why: None,
            published: false,
        };
        changes
            .ask(&update.change, "update", &json!({"op": "update"}))
            .unwrap();
        let kept = update.clone();
        updates.keep(|_| Some(vec![kept])).unwrap();
        let fake = Arc::new(Fake::new());
        let reconciler = Reconciler::new(
            updates.clone(),
            changes.clone(),
            Arc::new(Gate::new(true)),
            fake.clone(),
        );
        Self {
            folder,
            updates,
            changes,
            fake,
            reconciler,
            update,
        }
    }

    fn phase(&self) -> Phase {
        self.updates.of(&self.update.change).unwrap().unwrap().phase
    }

    fn placed(&self, cdhash: &[u8]) -> Value {
        let cdhash: String = cdhash.iter().map(|byte| format!("{byte:02x}")).collect();
        json!({"ok": true, "phase": "placed", "restart": "done", "cdhash": cdhash})
    }
}

#[test]
fn a_placed_update_this_build_runs_is_released_once_verified_and_its_change_settled() {
    let guest = Guest::new();
    let own = guest.fake.own.clone();
    guest.fake.answer("transaction", guest.placed(&own));
    guest
        .fake
        .answer("release", json!({"ok": true, "phase": "released"}));

    let open = guest.reconciler.round();

    assert!(!open);
    assert_eq!(guest.phase(), Phase::Released);
    assert_eq!(guest.fake.ops(), ["transaction", "release"]);
    let settled = guest.changes.of(&guest.update.change).unwrap();
    assert!(
        matches!(settled, Some(crate::machine::changes::State::Done { reply }) if reply["ok"] == true)
    );
}

#[test]
fn a_placed_update_of_another_build_waits_and_spends_no_release() {
    let guest = Guest::new();
    guest.fake.answer("transaction", guest.placed(&[0xcd; 20]));

    assert!(guest.reconciler.round());

    assert_eq!(guest.phase(), Phase::Asked);
    assert_eq!(guest.fake.ops(), ["transaction"]);
}

#[test]
fn a_new_build_worse_than_the_one_it_replaces_is_never_released() {
    for worse in ["input", "capture", "unsettled"] {
        let guest = Guest::new();
        let own = guest.fake.own.clone();
        guest.fake.answer("transaction", guest.placed(&own));
        match worse {
            "input" => *guest.fake.input.lock().unwrap() = (true, Health::Other),
            "capture" => *guest.fake.capture.lock().unwrap() = Health::Permission,
            _ => *guest.fake.input.lock().unwrap() = (false, Health::Permission),
        }

        assert!(guest.reconciler.round(), "{worse}");

        assert_eq!(guest.phase(), Phase::Asked, "{worse}");
        assert!(!guest.fake.ops().contains(&"release".to_owned()), "{worse}");
    }
}

#[test]
fn a_rollback_is_settled_from_the_installers_state_with_nothing_asked_again() {
    let guest = Guest::new();
    guest.fake.answer(
        "transaction",
        json!({"ok": true, "phase": "rolledBack", "why": "not released in time"}),
    );

    assert!(!guest.reconciler.round());

    assert_eq!(guest.phase(), Phase::RolledBack);
    let kept = guest.updates.of(&guest.update.change).unwrap().unwrap();
    assert_eq!(kept.why.as_deref(), Some("not released in time"));
}

#[test]
fn an_install_refused_before_its_transaction_began_is_settled_refused() {
    let guest = Guest::new();
    guest.fake.answer(
        "transaction",
        json!({"ok": false, "error": "No such update."}),
    );
    guest.fake.answer(
        "outcome",
        json!({"ok": false, "error": "Another update is open."}),
    );

    assert!(!guest.reconciler.round());

    assert_eq!(guest.phase(), Phase::Refused);
    assert!(!guest.fake.ops().contains(&"install".to_owned()));
}

#[test]
fn an_install_never_asked_or_cut_short_is_asked_again_as_the_same_request() {
    for outcome in [
        json!({"ok": true, "asked": false}),
        json!({"ok": true, "pending": true}),
    ] {
        let guest = Guest::new();
        guest.fake.answer(
            "transaction",
            json!({"ok": false, "error": "No such update."}),
        );
        guest.fake.answer("outcome", outcome.clone());
        guest
            .fake
            .answer("install", json!({"ok": true, "phase": "copying"}));

        assert!(guest.reconciler.round());

        let asked = guest.fake.asked.lock().unwrap().clone();
        let install = asked.iter().find(|asked| asked["op"] == "install").unwrap();
        assert_eq!(install["request"], json!(guest.update.install), "{outcome}");
        assert_eq!(
            install["transaction"],
            json!(guest.update.transaction),
            "{outcome}"
        );
        assert_eq!(guest.phase(), Phase::Asked, "{outcome}");
    }
}

#[test]
fn an_installer_that_does_not_answer_leaves_the_update_pending() {
    let guest = Guest::new();

    assert!(guest.reconciler.round());

    assert_eq!(guest.phase(), Phase::Asked);
}

#[test]
fn consent_wanted_before_and_wanted_still_is_no_worse_and_any_other_failure_is() {
    assert!(Health::Permission.no_worse_than(Health::Permission));
    assert!(Health::Ready.no_worse_than(Health::Other));
    assert!(!Health::Permission.no_worse_than(Health::Ready));
    assert!(!Health::Other.no_worse_than(Health::Other));
}

#[test]
fn an_installer_failing_to_do_or_read_is_never_taken_for_its_refusal() {
    for failed in ["transaction", "install"] {
        let guest = Guest::new();
        let failure = json!({"ok": false, "code": "io", "error": "A file could not be written."});
        if failed == "transaction" {
            guest.fake.answer("transaction", failure);
        } else {
            guest.fake.answer(
                "transaction",
                json!({"ok": false, "error": "No such update."}),
            );
            guest
                .fake
                .answer("outcome", json!({"ok": true, "asked": false}));
            guest.fake.answer("install", failure);
        }

        assert!(guest.reconciler.round(), "{failed}");

        assert_eq!(guest.phase(), Phase::Asked, "{failed}");
    }
}

#[test]
fn an_update_settled_whose_change_was_not_yet_told_is_told_by_the_next_round() {
    let guest = Guest::new();
    guest
        .updates
        .settle(
            &guest.update.change,
            Phase::RolledBack,
            Some("put back".into()),
        )
        .unwrap();
    assert!(matches!(
        guest.changes.of(&guest.update.change).unwrap(),
        Some(crate::machine::changes::State::Intended)
    ));

    guest.reconciler.round();

    let told = guest.changes.of(&guest.update.change).unwrap();
    assert!(
        matches!(&told, Some(crate::machine::changes::State::Done { reply }) if reply["update"] == "rolledBack"),
        "{told:?}"
    );
}

#[test]
fn a_new_build_not_yet_serving_its_station_is_never_released() {
    let guest = Guest::new();
    let own = guest.fake.own.clone();
    guest.fake.answer("transaction", guest.placed(&own));
    guest
        .fake
        .serving
        .store(false, std::sync::atomic::Ordering::SeqCst);

    assert!(guest.reconciler.round());

    assert!(!guest.fake.ops().contains(&"release".to_owned()));
}

#[test]
fn a_settled_update_whose_change_cannot_be_told_yet_keeps_its_round_open() {
    let guest = Guest::new();
    guest
        .updates
        .settle(&guest.update.change, Phase::Released, None)
        .unwrap();
    let changes = guest.folder.path().join("changes");
    std::fs::rename(&changes, guest.folder.path().join("away")).unwrap();

    assert!(guest.reconciler.round(), "closed with a change untold");

    std::fs::rename(guest.folder.path().join("away"), &changes).unwrap();
    assert!(!guest.reconciler.round());
    assert!(matches!(
        guest.changes.of(&guest.update.change).unwrap(),
        Some(crate::machine::changes::State::Done { .. })
    ));
}

#[test]
fn an_update_settled_in_a_round_whose_change_cannot_be_told_keeps_it_open() {
    let guest = Guest::new();
    guest
        .fake
        .answer("transaction", json!({"ok": true, "phase": "rolledBack"}));
    let changes = guest.folder.path().join("changes");
    std::fs::rename(&changes, guest.folder.path().join("away")).unwrap();

    assert!(guest.reconciler.round(), "closed with a change untold");
    assert_eq!(guest.phase(), Phase::RolledBack);

    std::fs::rename(guest.folder.path().join("away"), &changes).unwrap();
    assert!(!guest.reconciler.round());
    assert!(
        guest
            .updates
            .of(&guest.update.change)
            .unwrap()
            .unwrap()
            .published
    );
}

#[test]
fn an_update_whose_change_was_told_and_acknowledged_is_marked_told_and_rests() {
    let guest = Guest::new();
    let change = &guest.update.change;
    guest.updates.settle(change, Phase::Released, None).unwrap();
    guest.changes.done(change, &json!({"ok": true})).unwrap();
    guest.changes.settled(std::slice::from_ref(change)).unwrap();

    assert!(!guest.reconciler.round());

    assert!(guest.updates.of(change).unwrap().unwrap().published);
    assert!(guest.changes.of(change).unwrap().is_none());
    assert!(!guest.reconciler.round());
}
