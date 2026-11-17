use super::*;
use crate::machine::{
    commitment::create,
    processes::{JobRecord, Processes, STORE},
    supervise::{DONE, ENDED, proof},
    vault::Memory,
};
use atlas_core::error::{Code, Error, Result};
use atlas_session::generation::Generation;
use std::{cell::Cell, sync::Arc};

/// The supervisors kept, and where they leave their proofs.
struct Guest {
    _folder: tempfile::TempDir,
    proofs: std::path::PathBuf,
    kept: Processes<Arc<Memory>>,
}

fn guest() -> Guest {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let kept = Processes::new(folder.path().join("processes.json"), vault);
    let proofs = folder.path().join("proofs");
    std::fs::create_dir(&proofs).unwrap();
    Guest {
        _folder: folder,
        proofs,
        kept,
    }
}

fn this_boot() -> String {
    atlas_core::process::boot().unwrap()
}

/// A supervisor started in this boot for `change`.
fn supervisor(pid: i32, change: Option<&Token>) -> Supervised {
    Supervised {
        boot: this_boot(),
        generation: Generation(3),
        supervisor: Instance::new(pid, [1, 0]),
        change: change.cloned(),
        job: None,
    }
}

fn intended(change: &Token, operation: &str) -> Intended {
    Intended {
        change: change.clone(),
        operation: operation.to_owned(),
        request: String::new(),
    }
}

/// The changes of `unresolved` proven gone, nothing carried out here or
/// owned, and only the supervisors `running` names still running.
fn gone(guest: &Guest, unresolved: Vec<Intended>, running: &[Instance]) -> Vec<Token> {
    judged(guest, unresolved, running, &|_| false, &|_| false)
}

fn judged(
    guest: &Guest,
    unresolved: Vec<Intended>,
    running: &[Instance],
    carried: &dyn Fn(&Token) -> bool,
    owned: &dyn Fn(&Instance) -> bool,
) -> Vec<Token> {
    Evidence {
        kept: &guest.kept,
        proofs: &guest.proofs,
        carried,
        owned,
        running: &|supervisor| running.contains(supervisor),
    }
    .proven_gone(unresolved)
}

#[test]
fn a_command_cut_after_its_intent_and_before_any_supervisor_is_proven_gone() {
    let guest = guest();
    let change = Token::new();

    assert_eq!(gone(&guest, vec![intended(&change, "run")], &[]), [change]);
}

#[test]
fn a_command_cut_after_its_supervisor_or_its_spawn_is_gone_only_once_its_supervisor_is_done_with() {
    let guest = guest();
    let change = Token::new();
    let kept = supervisor(10, Some(&change));
    guest.kept.add(kept.clone()).unwrap();
    let asked = || vec![intended(&change, "run")];

    // Its supervisor still runs, or went leaving no proof that its work ended.
    assert!(gone(&guest, asked(), &[kept.supervisor]).is_empty());
    assert!(gone(&guest, asked(), &[]).is_empty());
    proof::leave(&guest.proofs, &kept.supervisor, 3).unwrap();
    assert!(gone(&guest, asked(), &[]).is_empty(), "an unsure end");

    proof::leave(&guest.proofs, &kept.supervisor, ENDED).unwrap();
    assert!(
        gone(&guest, asked(), &[kept.supervisor]).is_empty(),
        "a proof beside a supervisor still running"
    );
    assert_eq!(gone(&guest, asked(), &[]), std::slice::from_ref(&change));
    proof::leave(&guest.proofs, &kept.supervisor, DONE).unwrap();
    assert_eq!(gone(&guest, asked(), &[]), [change]);
}

#[test]
fn a_command_whose_supervisor_is_from_another_boot_is_proven_gone() {
    let guest = guest();
    let change = Token::new();
    let mut kept = supervisor(10, Some(&change));
    kept.boot = "another boot".into();
    guest.kept.add(kept.clone()).unwrap();

    assert_eq!(
        gone(&guest, vec![intended(&change, "run")], &[kept.supervisor]),
        [change]
    );
}

#[test]
fn a_change_this_daemon_still_carries_out_or_whose_supervisor_a_keeper_owns_is_never_judged() {
    let guest = guest();
    let (carried, owned) = (Token::new(), Token::new());
    let kept = supervisor(10, Some(&owned));
    guest.kept.add(kept.clone()).unwrap();
    proof::leave(&guest.proofs, &kept.supervisor, DONE).unwrap();
    let asked_whether_carried = Cell::new(false);

    let judged = judged(
        &guest,
        vec![intended(&carried, "run"), intended(&owned, "run")],
        &[],
        &|change| {
            asked_whether_carried.set(true);
            *change == carried
        },
        &|supervisor| *supervisor == kept.supervisor,
    );

    assert!(judged.is_empty(), "{judged:?}");
    assert!(asked_whether_carried.get());
}

#[test]
fn a_supervisor_an_earlier_build_kept_naming_no_change_holds_every_command_until_done_with() {
    let guest = guest();
    let change = Token::new();
    let earlier = supervisor(10, None);
    guest.kept.add(earlier.clone()).unwrap();

    assert!(gone(&guest, vec![intended(&change, "run")], &[]).is_empty());
    proof::leave(&guest.proofs, &earlier.supervisor, DONE).unwrap();
    assert_eq!(gone(&guest, vec![intended(&change, "run")], &[]), [change]);
}

#[test]
fn a_job_start_is_judged_by_its_supervisor_and_a_job_change_naming_none_is_never_judged() {
    let guest = guest();
    let (started, unnamed) = (Token::new(), Token::new());
    let mut kept = supervisor(10, Some(&started));
    kept.job = Some(JobRecord {
        id: atlas_core::protocol::JobId::new(),
        name: "loop".into(),
        task: None,
        session: None,
        started_at: 0,
        ends_at: 1,
        change: started.clone(),
    });
    guest.kept.add(kept.clone()).unwrap();
    let asked = || vec![intended(&started, "job"), intended(&unnamed, "job")];

    assert!(gone(&guest, asked(), &[kept.supervisor]).is_empty());
    proof::leave(&guest.proofs, &kept.supervisor, ENDED).unwrap();

    // A stop, or a start whose supervisor was retired: which, nothing tells.
    assert_eq!(gone(&guest, asked(), &[]), [started]);
}

#[test]
fn changes_of_a_kind_with_no_evidence_here_are_never_judged() {
    let guest = guest();
    let asked = [
        "acquire",
        "input",
        "release",
        "stage",
        "remove-copy",
        "update",
        "open",
    ]
    .map(|operation| intended(&Token::new(), operation))
    .to_vec();

    assert!(gone(&guest, asked, &[]).is_empty());
}

/// Supervisors whose record cannot be trusted.
struct Untold;

impl Remembered for Untold {
    fn kept(&self) -> Result<Vec<Supervised>> {
        Err(Error::new(Code::Stopped, "unknown"))
    }

    fn add(&self, _: Supervised) -> Result<()> {
        Err(Error::new(Code::Stopped, "unknown"))
    }

    fn retire(&self, _: &[Supervised]) -> Result<()> {
        Err(Error::new(Code::Stopped, "unknown"))
    }
}

#[test]
fn supervisors_whose_record_cannot_be_trusted_prove_nothing_gone() {
    let folder = tempfile::tempdir().unwrap();

    let judged = Evidence {
        kept: &Untold,
        proofs: folder.path(),
        carried: &|_| false,
        owned: &|_| false,
        running: &|_| false,
    }
    .proven_gone(vec![intended(&Token::new(), "run")]);

    assert!(judged.is_empty());
}
