use super::*;
use crate::machine::{
    commitment::create,
    processes::{JobRecord, Processes, STORE},
    supervise::{DONE, ENDED, proof},
    vault::Memory,
};
use atlas_core::error::{Code, Error, Result};
use atlas_session::generation::Generation;
use serde_json::Value;
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
        asked: None,
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
        released: &|_| false,
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
fn a_page_opened_for_the_person_whose_reply_was_lost_is_gone_once_its_supervisor_is_done_with() {
    let guest = guest();
    let change = Token::new();
    let kept = supervisor(11, Some(&change));
    guest.kept.add(kept.clone()).unwrap();
    let asked = || vec![intended(&change, "person-open")];

    assert!(gone(&guest, asked(), &[kept.supervisor]).is_empty());
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
    let asked = || {
        ["stage", "remove-copy", "update", "open"]
            .map(|operation| intended(&Token::new(), operation))
            .to_vec()
    };

    assert!(gone(&guest, asked(), &[]).is_empty());
    let released = Evidence {
        kept: &guest.kept,
        proofs: &guest.proofs,
        carried: &|_| false,
        owned: &|_| false,
        running: &|_| false,
        released: &|_| true,
    }
    .proven_gone(asked());
    assert!(released.is_empty(), "{released:?}");
}

#[test]
fn a_change_to_input_is_gone_only_once_the_input_service_let_go_and_none_carries_it_here() {
    let guest = guest();
    let (taken, sent, let_go, carried) = (Token::new(), Token::new(), Token::new(), Token::new());
    let owner = Token::new();
    let asked = || {
        vec![
            intended(&taken, "acquire"),
            intended(&sent, "input"),
            intended(&let_go, "release"),
            intended(&carried, "input"),
            intended(&owner, "owner"),
        ]
    };
    let judge = |released: &dyn Fn(&Token) -> bool| {
        Evidence {
            kept: &guest.kept,
            proofs: &guest.proofs,
            carried: &|change| *change == carried,
            owned: &|_| false,
            running: &|_| false,
            released,
        }
        .proven_gone(asked())
    };

    let unsaid = judge(&|_| false);
    let mut said = judge(&|_| true);

    assert!(unsaid.is_empty(), "{unsaid:?}");
    said.sort();
    let mut expected = vec![taken, sent, let_go, owner];
    expected.sort();
    assert_eq!(said, expected, "the one still carried out here is kept");
}

#[test]
fn what_the_input_service_says_never_ends_a_command() {
    let folder = tempfile::tempdir().unwrap();

    let judged = Evidence {
        kept: &Untold,
        proofs: folder.path(),
        carried: &|_| false,
        owned: &|_| false,
        running: &|_| false,
        released: &|_| true,
    }
    .proven_gone(vec![intended(&Token::new(), "run")]);

    assert!(judged.is_empty());
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
        released: &|_| false,
    }
    .proven_gone(vec![intended(&Token::new(), "run")]);

    assert!(judged.is_empty());
}

/// A job's change `change` whose request `asked` was kept beside it.
fn job_change(change: &Token, asked: Value) -> Intended {
    Intended {
        asked: Some(asked),
        ..intended(change, "job")
    }
}

/// A job's start, the change `change`.
fn start_request(change: &Token) -> Value {
    serde_json::json!({"op": "job", "action": "start", "argv": ["/bin/sleep", "30"],
        "lifetimeMs": 60_000, "uid": 501, "change": change})
}

/// A stop of job `id`, the change `change`.
fn stop_request(change: &Token, id: &JobId) -> Value {
    serde_json::json!({"op": "job", "action": "stop", "id": id,
        "expect": {"boot": this_boot(), "pid": 10, "start": [1, 0], "pgid": 10},
        "graceMs": 2000, "change": change})
}

/// A supervisor started in this boot for the job `id`, which the change
/// `started` started.
fn job_supervisor(pid: i32, started: &Token, id: &JobId) -> Supervised {
    Supervised {
        job: Some(JobRecord {
            id: id.clone(),
            name: "loop".into(),
            task: None,
            session: None,
            started_at: 0,
            ends_at: 1,
            change: started.clone(),
        }),
        ..supervisor(pid, Some(started))
    }
}

#[test]
fn a_job_start_whose_request_was_kept_and_that_no_supervisor_names_is_judged_as_a_command_is() {
    let guest = guest();
    let (never, started) = (Token::new(), Token::new());
    let kept = job_supervisor(10, &started, &JobId::new());
    guest.kept.add(kept.clone()).unwrap();
    let asked = || {
        vec![
            job_change(&never, start_request(&never)),
            job_change(&started, start_request(&started)),
        ]
    };

    // Never spawned, or its supervisor retired: gone, as a command's is.
    assert_eq!(
        gone(&guest, asked(), &[kept.supervisor]),
        std::slice::from_ref(&never)
    );
    proof::leave(&guest.proofs, &kept.supervisor, ENDED).unwrap();
    let mut judged = gone(&guest, asked(), &[]);
    judged.sort();
    let mut both = vec![never, started];
    both.sort();
    assert_eq!(judged, both);
}

#[test]
fn a_job_stop_is_ended_only_once_its_target_is_proven_done_with() {
    let guest = guest();
    let (started, stop) = (Token::new(), Token::new());
    let id = JobId::new();
    let target = job_supervisor(10, &started, &id);
    guest.kept.add(target.clone()).unwrap();
    let asked = || vec![job_change(&stop, stop_request(&stop, &id))];

    // No supervisor names the stop: that proves nothing while its target
    // runs, or went with no proof its work ended.
    assert!(gone(&guest, asked(), &[target.supervisor]).is_empty());
    assert!(gone(&guest, asked(), &[]).is_empty());
    proof::leave(&guest.proofs, &target.supervisor, 3).unwrap();
    assert!(gone(&guest, asked(), &[]).is_empty(), "an unsure end");

    proof::leave(&guest.proofs, &target.supervisor, ENDED).unwrap();
    assert!(
        gone(&guest, asked(), &[target.supervisor]).is_empty(),
        "a proof beside a supervisor still running"
    );
    assert_eq!(gone(&guest, asked(), &[]), [stop]);
}

#[test]
fn a_job_stop_whose_target_is_from_another_boot_or_no_longer_kept_is_ended() {
    let guest = guest();
    let (started, stop, unknown) = (Token::new(), Token::new(), Token::new());
    let id = JobId::new();
    let mut target = job_supervisor(10, &started, &id);
    target.boot = "another boot".into();
    guest.kept.add(target.clone()).unwrap();

    let mut judged = gone(
        &guest,
        vec![
            job_change(&stop, stop_request(&stop, &id)),
            job_change(&unknown, stop_request(&unknown, &JobId::new())),
        ],
        &[target.supervisor],
    );
    judged.sort();
    let mut both = vec![stop, unknown];
    both.sort();

    assert_eq!(judged, both);
}

#[test]
fn a_job_change_an_earlier_guest_kept_without_its_request_is_judged_as_before() {
    let guest = guest();
    let (started, unnamed) = (Token::new(), Token::new());
    let kept = job_supervisor(10, &started, &JobId::new());
    guest.kept.add(kept.clone()).unwrap();
    proof::leave(&guest.proofs, &kept.supervisor, DONE).unwrap();
    // A request kept that is no job's tells nothing either.
    let odd = Intended {
        asked: Some(serde_json::json!({"op": "hello"})),
        ..intended(&unnamed, "job")
    };

    assert_eq!(
        gone(&guest, vec![intended(&started, "job"), odd], &[]),
        [started]
    );
}

#[test]
fn an_environment_change_is_judged_at_each_crash_boundary_as_a_command_is() {
    let guest = guest();
    let (unstarted, started, other_boot, carried) =
        (Token::new(), Token::new(), Token::new(), Token::new());
    let kept = supervisor(10, Some(&started));
    guest.kept.add(kept.clone()).unwrap();
    let mut earlier = supervisor(11, Some(&other_boot));
    earlier.boot = "another boot".into();
    guest.kept.add(earlier.clone()).unwrap();
    let asked = |change: &Token| vec![intended(change, "environment")];

    // Cut after its intent, before its supervisor was kept.
    assert_eq!(gone(&guest, asked(&unstarted), &[]), [unstarted]);
    // Its supervisor kept: gone only once that supervisor is done with.
    assert!(gone(&guest, asked(&started), &[kept.supervisor]).is_empty());
    assert!(gone(&guest, asked(&started), &[]).is_empty(), "no proof");
    proof::leave(&guest.proofs, &kept.supervisor, ENDED).unwrap();
    assert!(gone(&guest, asked(&started), &[kept.supervisor]).is_empty());
    assert_eq!(
        gone(&guest, asked(&started), &[]),
        std::slice::from_ref(&started)
    );
    // Its supervisor from another boot.
    assert_eq!(
        gone(&guest, asked(&other_boot), &[earlier.supervisor]),
        [other_boot]
    );
    // Still carried out here: never judged.
    assert!(judged(&guest, asked(&carried), &[], &|_| true, &|_| false).is_empty());
}
