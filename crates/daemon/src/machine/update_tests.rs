use super::*;
use crate::machine::{commitment::create, vault::Memory};
use std::sync::Arc;

/// A guest's updates as pairing leaves them, in a folder of its own.
pub(super) fn paired() -> (tempfile::TempDir, Arc<Memory>, Updates<Arc<Memory>>) {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let updates = Updates::new(folder.path().join("updates.json"), vault.clone());
    (folder, vault, updates)
}

/// An update asked as `change`, as admission keeps it.
pub(super) fn asked(change: &Token) -> Update {
    Update {
        change: change.clone(),
        source: PathBuf::from("/Users/atlas/staged/Atlas.app"),
        manifest: None,
        transaction: Token::new(),
        install: Token::new(),
        release: Token::new(),
        input: Health::Permission,
        capture: Health::Ready,
        phase: Phase::Asked,
        why: None,
        published: false,
    }
}

fn add(updates: &Updates<Arc<Memory>>, update: Update) {
    updates
        .keep(|before| {
            let mut kept = before.to_vec();
            kept.push(update);
            Some(kept)
        })
        .unwrap();
}

#[test]
fn an_update_kept_stands_unresolved_across_restarts_until_it_is_settled() {
    let (folder, vault, updates) = paired();
    let change = Token::new();
    assert!(!updates.unresolved());

    add(&updates, asked(&change));
    let restarted = Updates::new(folder.path().join("updates.json"), vault);

    assert!(restarted.unresolved());
    restarted.settle(&change, Phase::Released, None).unwrap();
    assert!(!restarted.unresolved());
    assert_eq!(
        restarted.of(&change).unwrap().unwrap().phase,
        Phase::Released
    );
}

#[test]
fn an_update_settled_stays_as_it_was_settled() {
    let (_folder, _vault, updates) = paired();
    let change = Token::new();
    add(&updates, asked(&change));

    updates
        .settle(&change, Phase::RolledBack, Some("put back".into()))
        .unwrap();
    updates.settle(&change, Phase::Released, None).unwrap();

    let kept = updates.of(&change).unwrap().unwrap();
    assert_eq!(kept.phase, Phase::RolledBack);
    assert_eq!(kept.why.as_deref(), Some("put back"));
}

#[test]
fn updates_deleted_or_rolled_back_by_a_guest_command_hold_updates_up() {
    let (folder, _vault, updates) = paired();
    let path = folder.path().join("updates.json");
    add(&updates, asked(&Token::new()));
    let kept = std::fs::read(&path).unwrap();
    add(&updates, asked(&Token::new()));

    std::fs::write(&path, &kept).unwrap();
    assert!(updates.unresolved(), "an older record reads as unknown");
    std::fs::remove_file(&path).unwrap();
    assert!(updates.unresolved(), "a deleted record reads as unknown");
}

#[test]
fn a_round_that_read_before_an_admission_never_clears_it() {
    let gate = Gate::new(false);
    let read_before = gate.admitted();
    assert_eq!(gate.begin_admission(&Token::new()), Admission::Began);
    gate.end_admission(true);

    gate.resolved(read_before);

    assert!(
        gate.resume().is_none(),
        "a Resume passed an update just admitted"
    );
    gate.resolved(gate.admitted());
    assert!(gate.resume().is_some());
}

#[test]
fn an_admission_and_a_resume_never_cross() {
    let gate = Gate::new(false);
    let resuming = gate.resume().unwrap();

    let change = Token::new();
    assert_eq!(
        gate.begin_admission(&change),
        Admission::Held,
        "admitted while a Resume committed"
    );
    drop(resuming);
    assert_eq!(gate.begin_admission(&change), Admission::Began);
    assert_eq!(gate.begin_admission(&change), Admission::Already);
    assert_eq!(gate.begin_admission(&Token::new()), Admission::Held);
    assert!(
        gate.resume().is_none(),
        "resumed while an update was admitted"
    );
}

#[test]
fn the_oldest_published_updates_go_and_every_one_not_yet_told_stays() {
    let settled = |n: usize| {
        let mut update = asked(&Token::new());
        update.phase = Phase::Released;
        update.why = Some(n.to_string());
        update.published = true;
        update
    };
    let unsettled = asked(&Token::new());
    let mut untold = asked(&Token::new());
    untold.phase = Phase::RolledBack;
    let mut updates = vec![unsettled.clone(), untold.clone()];
    updates.extend((0..SETTLED + 3).map(settled));

    let kept = pruned(updates);

    assert_eq!(kept.len(), SETTLED + 2);
    assert_eq!(kept[..2], [unsettled, untold]);
    assert_eq!(kept[2].why.as_deref(), Some("3"), "the oldest went first");
}

#[test]
fn a_record_too_long_to_read_back_is_never_written() {
    let (folder, _vault, updates) = paired();
    add(&updates, asked(&Token::new()));
    let before = std::fs::read(folder.path().join("updates.json")).unwrap();
    let mut long = asked(&Token::new());
    long.why = Some("x".repeat(usize::try_from(LIMIT).unwrap()));

    let kept = updates.keep(|before| {
        let mut kept = before.to_vec();
        kept.push(long);
        Some(kept)
    });

    assert_eq!(kept.unwrap_err().code, Code::Busy);
    assert_eq!(
        std::fs::read(folder.path().join("updates.json")).unwrap(),
        before
    );
    assert_eq!(updates.all().unwrap().len(), 1);
}
