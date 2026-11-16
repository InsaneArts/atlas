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
