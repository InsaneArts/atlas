use super::*;

fn refused(judged: Result<()>) -> bool {
    match judged {
        Ok(()) => false,
        Err(error) => {
            assert_eq!(error.code, Code::Denied);
            assert_eq!(error.error, REFUSAL);
            true
        }
    }
}

#[test]
fn only_a_program_of_the_same_atlas_counts_as_atlas_starting_a_worker() {
    let directory = tempfile::tempdir().unwrap();
    let macos = directory.path().join("Atlas.app/Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    let (atlas, app) = (macos.join("atlas"), macos.join("Atlas"));
    let shell = directory.path().join("zsh");
    let other = directory.path().join("Other.app/Contents/MacOS/atlas");
    std::fs::create_dir_all(other.parent().unwrap()).unwrap();
    for program in [&atlas, &app, &shell, &other] {
        std::fs::write(program, b"").unwrap();
    }
    let link = directory.path().join("atlas-link");
    std::os::unix::fs::symlink(&atlas, &link).unwrap();

    assert!(same_atlas(&atlas, &atlas));
    assert!(same_atlas(&link, &atlas), "the same file by a link");
    assert!(same_atlas(&app, &atlas), "the same app");
    assert!(!same_atlas(&shell, &atlas), "a shell");
    assert!(!same_atlas(&other, &atlas), "another app");
    assert!(!same_atlas(&directory.path().join("missing"), &atlas));
}

#[test]
fn a_worker_started_by_this_test_is_not_started_by_atlas() {
    assert!(refused(started_by_atlas()));
}

#[test]
fn only_the_process_launchd_runs_as_its_job_counts_as_started_by_it() {
    assert!(judge_job(80, Some(80)).is_ok());
    assert!(refused(judge_job(80, Some(81))));
    assert!(refused(judge_job(80, None)));
}

#[test]
fn a_worker_this_test_started_is_not_the_job_launchd_runs() {
    assert!(refused(launchd_job(
        "com.insanearts.atlas.tests.no-such-job"
    )));
}

/// A machine guest's environment worker as its daemon starts one: handed
/// its packet by its parent, a supervisor of this Atlas, the child of the
/// daemon 300.
fn handing() -> Handing {
    Handing {
        uid: 502,
        parent: 400,
        handed: Some(Peer { pid: 400, uid: 502 }),
        supervisor: Some((502, 300)),
        same_atlas: true,
    }
}

#[test]
fn an_environment_worker_handed_its_packet_by_its_supervisor_names_the_daemon_above_it() {
    assert_eq!(judge_environment_worker(&handing()).unwrap(), 300);
}

#[test]
fn an_environment_worker_anywhere_else_is_refused() {
    let places = [
        // Its standard input is nothing, a pipe or a terminal: started by
        // hand, or by an agent's command.
        Handing {
            handed: None,
            ..handing()
        },
        // A socket it made for itself.
        Handing {
            handed: Some(Peer { pid: 401, uid: 502 }),
            ..handing()
        },
        // A socket made in another account.
        Handing {
            handed: Some(Peer { pid: 400, uid: 503 }),
            ..handing()
        },
        // A parent that is not this Atlas: a shell, or a test.
        Handing {
            same_atlas: false,
            ..handing()
        },
        // A parent the kernel would not say anything of.
        Handing {
            supervisor: None,
            ..handing()
        },
        // A parent in another account.
        Handing {
            supervisor: Some((503, 300)),
            ..handing()
        },
        // A parent launchd took over: its daemon is gone.
        Handing {
            supervisor: Some((502, 1)),
            ..handing()
        },
    ];

    for place in places {
        assert!(
            refused(judge_environment_worker(&place).map(|_| ())),
            "{place:?}"
        );
    }
}

#[test]
fn an_environment_worker_this_test_started_is_refused() {
    assert!(refused(machine_environment_worker().map(|_| ())));
}
