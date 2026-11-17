use super::*;
use crate::machine::{
    commitment::create,
    supervise::{DONE, ENDED, proof},
    vault::Memory,
};
use std::sync::Arc;

/// A guest's supervisors as pairing leaves them, in a folder of their own.
fn paired() -> (tempfile::TempDir, Arc<Memory>, Processes<Arc<Memory>>) {
    let folder = tempfile::tempdir().unwrap();
    let vault = Arc::new(Memory::default());
    create(&vault, STORE).unwrap();
    let processes = Processes::new(folder.path().join("processes.json"), vault.clone());
    (folder, vault, processes)
}

fn supervised(pid: i32) -> Supervised {
    Supervised {
        boot: "boot".into(),
        generation: Generation(3),
        supervisor: Instance::new(pid, [1, 0]),
        change: None,
        job: None,
    }
}

#[test]
fn a_guest_that_never_supervised_anything_keeps_none() {
    let (_folder, _vault, processes) = paired();

    assert_eq!(processes.kept().unwrap(), Vec::new());
}

#[test]
fn supervisors_are_kept_across_restarts_until_each_is_retired() {
    let (folder, vault, processes) = paired();

    processes.add(supervised(10)).unwrap();
    processes.add(supervised(11)).unwrap();
    processes.add(supervised(10)).unwrap();
    let restarted = Processes::new(folder.path().join("processes.json"), vault);

    assert_eq!(restarted.kept().unwrap(), [supervised(10), supervised(11)]);
    restarted.retire(&[supervised(10)]).unwrap();
    assert_eq!(restarted.kept().unwrap(), [supervised(11)]);
}

#[test]
fn retiring_touches_only_the_very_entry_named() {
    let (_folder, _vault, processes) = paired();
    processes.add(supervised(10)).unwrap();
    let mut other = supervised(10);
    other.generation = Generation(4);

    processes.retire(&[other]).unwrap();

    assert_eq!(processes.kept().unwrap(), [supervised(10)]);
}

#[test]
fn a_record_deleted_rolled_back_or_damaged_is_unknown() {
    let (folder, _vault, processes) = paired();
    let path = folder.path().join("processes.json");
    processes.add(supervised(10)).unwrap();
    let earlier = std::fs::read(&path).unwrap();
    processes.add(supervised(11)).unwrap();

    std::fs::remove_file(&path).unwrap();
    assert_eq!(processes.kept().unwrap_err().code, Code::Stopped);
    std::fs::write(&path, &earlier).unwrap();
    assert!(processes.kept().is_err());
    std::fs::write(&path, b"{").unwrap();
    assert!(processes.kept().is_err());
    assert!(
        processes.add(supervised(12)).is_err(),
        "nothing is added to what cannot be told"
    );
}

#[test]
fn a_full_record_takes_no_more() {
    let (_folder, _vault, processes) = paired();
    for pid in 0..i32::try_from(MOST).unwrap() {
        processes.add(supervised(pid + 1)).unwrap();
    }

    let refused = processes.add(supervised(100_000)).unwrap_err();

    assert_eq!(refused.code, Code::Busy);
}

#[test]
fn a_change_committed_but_never_written_is_unknown_until_judged_otherwise() {
    let (_folder, _vault, processes) = paired();
    processes.add(supervised(10)).unwrap();
    // Committed to a next revision whose file never came.
    processes
        .commitment
        .begin(Revision {
            number: 2,
            digest: "0".repeat(64),
        })
        .unwrap();

    assert!(processes.kept().is_err());
}

#[test]
fn a_change_written_but_not_yet_made_current_is_kept_as_written() {
    let (folder, _vault, processes) = paired();
    let path = folder.path().join("processes.json");
    processes.add(supervised(10)).unwrap();
    let bytes = serde_json::to_vec(&Record {
        version: VERSION,
        number: 2,
        work: vec![supervised(10), supervised(11)],
    })
    .unwrap();
    processes
        .commitment
        .begin(Revision {
            number: 2,
            digest: bytes_sha256(&bytes),
        })
        .unwrap();
    std::fs::write(&path, &bytes).unwrap();

    assert_eq!(processes.kept().unwrap(), [supervised(10), supervised(11)]);
}

/// A supervisor of this boot, as an earlier daemon kept it.
fn left(pid: i32) -> Supervised {
    Supervised {
        boot: atlas_core::process::boot().unwrap(),
        ..supervised(pid)
    }
}

/// Recover with nothing owned and nothing waited for, under a saved Stop.
fn recovered(
    processes: &Processes<Arc<Memory>>,
    proofs: &Path,
    running: impl Fn(&Instance) -> bool,
) -> bool {
    recover(processes, proofs, |_| false, running, Duration::ZERO, true)
}

#[test]
fn recovery_with_nothing_left_is_done() {
    let (folder, _vault, processes) = paired();

    assert!(recovered(&processes, folder.path(), |_| false));
}

#[test]
fn recovery_of_a_record_that_cannot_be_trusted_is_never_done() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();
    std::fs::write(folder.path().join("processes.json"), b"{").unwrap();

    assert!(!recovered(&processes, folder.path(), |_| false));
}

#[test]
fn a_supervisor_of_another_boot_is_forgotten() {
    let (folder, _vault, processes) = paired();
    processes.add(supervised(10)).unwrap();

    assert!(recovered(&processes, folder.path(), |_| true));
    assert_eq!(processes.kept().unwrap(), []);
}

#[test]
fn a_supervisor_gone_with_its_proof_is_forgotten_with_its_proof() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();
    proof::leave(folder.path(), &left(10).supervisor, ENDED).unwrap();

    assert!(recovered(&processes, folder.path(), |_| false));
    assert_eq!(processes.kept().unwrap(), []);
    assert_eq!(proof::left(folder.path(), &left(10).supervisor), None);
}

#[test]
fn a_supervisor_gone_without_proof_or_unsure_of_its_work_stays() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();
    processes.add(left(11)).unwrap();
    proof::leave(
        folder.path(),
        &left(11).supervisor,
        crate::machine::supervise::UNSURE,
    )
    .unwrap();

    assert!(!recovered(&processes, folder.path(), |_| false));
    assert_eq!(processes.kept().unwrap(), [left(10), left(11)]);
}

#[test]
fn a_supervisor_still_running_stays_whatever_proof_it_has() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();
    proof::leave(folder.path(), &left(10).supervisor, DONE).unwrap();

    assert!(!recovered(&processes, folder.path(), |_| true));
    assert_eq!(processes.kept().unwrap(), [left(10)]);
}

#[test]
fn a_supervisor_a_keeper_owns_is_left_to_its_keeper() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();

    let done = recover(
        &processes,
        folder.path(),
        |_| true,
        |_| true,
        Duration::ZERO,
        true,
    );

    assert!(done);
    assert_eq!(processes.kept().unwrap(), [left(10)]);
}

#[test]
fn recovery_without_a_saved_stop_forgets_nothing() {
    let (folder, _vault, processes) = paired();
    processes.add(supervised(10)).unwrap();

    let done = recover(
        &processes,
        folder.path(),
        |_| false,
        |_| false,
        Duration::ZERO,
        false,
    );

    assert!(!done);
    assert_eq!(processes.kept().unwrap(), [supervised(10)]);
}

#[test]
fn recovery_waits_for_a_supervisor_still_ending_its_work() {
    let (folder, _vault, processes) = paired();
    processes.add(left(10)).unwrap();
    proof::leave(folder.path(), &left(10).supervisor, ENDED).unwrap();
    let started = std::time::Instant::now();

    let done = recover(
        &processes,
        folder.path(),
        |_| false,
        |_| started.elapsed() < Duration::from_millis(300),
        Duration::from_secs(5),
        true,
    );

    assert!(done);
    assert!(started.elapsed() < Duration::from_secs(2));
}
