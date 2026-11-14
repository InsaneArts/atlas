use super::*;
use crate::machine::{commitment::create, vault::Memory};
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
