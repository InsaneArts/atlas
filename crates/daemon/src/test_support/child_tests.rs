use super::{Child, JOBS, Processes};
use atlas_core::process;
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};

/// Run the script in `mode` as its own group's leader, owned by `fixture`.
fn spawn(fixture: &mut Child, mode: &str) -> std::process::Child {
    use std::os::unix::process::CommandExt;

    let argv = fixture.argv(mode);
    let child = atlas_core::spawn::spawn(
        std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .process_group(0),
    )
    .unwrap();
    let instance = process::info(child.id() as i32).unwrap().unwrap().instance;
    fixture.owned.0.push(instance);
    child
}

#[tokio::test]
async fn a_ready_fixture_observes_term_and_keeps_running() {
    use std::os::unix::process::ExitStatusExt;

    let mut fixture = Child::new(JOBS);
    let mut child = spawn(&mut fixture, "ignore");
    let leader = fixture.message("ready").await;
    fixture.permit("start");
    assert_eq!(process::info(leader.pid).unwrap().unwrap().instance, leader);
    kill(Pid::from_raw(leader.pid), Signal::SIGTERM).unwrap();
    assert_eq!(fixture.message("term").await, leader);
    assert!(child.try_wait().unwrap().is_none());
    kill(Pid::from_raw(leader.pid), Signal::SIGKILL).unwrap();
    assert_eq!(child.wait().unwrap().signal(), Some(Signal::SIGKILL as i32));
}

#[tokio::test]
async fn a_fixture_failure_reaps_its_group_and_escaped_descendant() {
    for stage in ["registered", "permitted", "escaped"] {
        let mut fixture = Child::new(JOBS);
        let mut child = spawn(&mut fixture, "escape");
        let leader = fixture.message("ready").await;
        fixture.permit("start");
        let member = fixture.message("member").await;
        fixture.permit("fork");
        let parent = fixture.message("parent").await;
        fixture.permit_parent("fork");
        let descendant = fixture.message("descendant").await;
        let info = process::info(descendant.pid).unwrap().unwrap();
        assert_eq!(info.ppid, parent.pid);
        assert_eq!(info.pgid, leader.pid);
        if stage != "registered" {
            fixture.permit_descendant("escape");
        }
        if stage == "escaped" {
            assert_eq!(fixture.message("escaped").await, descendant);
            assert_ne!(
                process::info(descendant.pid).unwrap().unwrap().pgid,
                leader.pid
            );
        }

        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _fixture = fixture;
            panic!("fixture assertion failed");
        }));
        assert!(failure.is_err());
        let reaped = child.try_wait();
        if matches!(reaped, Ok(None)) {
            Processes::new(leader).cleanup().unwrap();
        }
        assert_eq!(reaped.unwrap_err().raw_os_error(), Some(nix::libc::ECHILD));
        for owned in [leader, member, parent, descendant] {
            assert!(process::info(owned.pid).unwrap().is_none(), "{owned:?}");
        }
    }
}

#[tokio::test]
async fn a_fixture_failure_before_descendant_registration_reaps_its_group() {
    let mut fixture = Child::new(JOBS);
    let mut child = spawn(&mut fixture, "escape");
    let leader = fixture.message("ready").await;
    fixture.permit("start");
    let member = fixture.message("member").await;
    fixture.permit("fork");
    let parent = fixture.message("parent").await;
    fixture.permit_parent("fork");
    // Refuse the announcement before recording its identity, just as a failed
    // process observation would. The descendant must still belong to the group.
    let failure = tokio::spawn(async move { fixture.message("escaped").await })
        .await
        .unwrap_err();
    assert!(failure.is_panic());
    let reaped = child.try_wait();
    if matches!(reaped, Ok(None)) {
        Processes::new(leader).cleanup().unwrap();
    }
    assert_eq!(reaped.unwrap_err().raw_os_error(), Some(nix::libc::ECHILD));
    assert!(process::group(leader.pid).unwrap().is_empty());
    for owned in [leader, member, parent] {
        assert!(process::info(owned.pid).unwrap().is_none(), "{owned:?}");
    }
}
