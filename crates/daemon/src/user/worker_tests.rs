use super::*;
use crate::worker::{Peer, REFUSAL};
use atlas_core::error::Code;

const UID: u32 = 502;
const DAEMON: u32 = 400;

/// A worker the helper daemon started: its child, handed a socket it made.
fn in_the_helper() -> Place {
    Place {
        uid: UID,
        parent: DAEMON,
        daemon: Some(DAEMON),
        handed: Some(Peer {
            pid: DAEMON,
            uid: UID,
        }),
    }
}

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
fn a_job_launcher_the_helper_daemon_started_and_handed_its_socket_goes_on() {
    assert!(!refused(judge_job_launcher(&in_the_helper())));
    assert!(!refused(judge_worker(&in_the_helper())));
}

#[test]
fn a_job_launcher_in_an_account_with_no_helper_daemon_is_refused() {
    let station = Place {
        daemon: None,
        ..in_the_helper()
    };
    assert!(refused(judge_job_launcher(&station)));
    assert!(refused(judge_worker(&station)));
}

#[test]
fn a_job_launcher_another_process_started_is_refused() {
    let agent = Place {
        parent: 777,
        handed: Some(Peer { pid: 777, uid: UID }),
        ..in_the_helper()
    };
    assert!(refused(judge_job_launcher(&agent)));
    assert!(refused(judge_worker(&agent)));
}

#[test]
fn a_job_launcher_told_to_go_on_a_pipe_or_another_socket_is_refused() {
    for handed in [
        None,
        Some(Peer { pid: 777, uid: UID }),
        Some(Peer {
            pid: DAEMON,
            uid: 501,
        }),
    ] {
        let place = Place {
            handed,
            ..in_the_helper()
        };
        assert!(refused(judge_job_launcher(&place)), "{handed:?}");
    }
}

/// This very process, standing in for the helper job launchd runs, serving
/// as its own account for a controller account.
fn this_helper_daemon() -> Place {
    Place {
        uid: UID,
        parent: 1,
        daemon: Some(OWN),
        handed: None,
    }
}

const OWN: u32 = 1234;

#[test]
fn the_helper_job_launchd_runs_serves() {
    assert!(!refused(judge_serving(
        &this_helper_daemon(),
        OWN,
        UID,
        UID + 1
    )));
}

#[test]
fn a_hand_started_helper_serve_is_refused() {
    // Not the process launchd names as the helper job.
    assert!(refused(judge_serving(
        &Place {
            daemon: Some(OWN + 1),
            ..this_helper_daemon()
        },
        OWN,
        UID,
        UID + 1,
    )));
    // No helper job in this account at all.
    assert!(refused(judge_serving(
        &Place {
            daemon: None,
            ..this_helper_daemon()
        },
        OWN,
        UID,
        UID + 1,
    )));
    // The --uid it was given is not its own account.
    assert!(refused(judge_serving(
        &this_helper_daemon(),
        OWN,
        UID + 9,
        UID + 1
    )));
    // Its controller is its own account.
    assert!(refused(judge_serving(&this_helper_daemon(), OWN, UID, UID)));
}

#[test]
fn an_input_service_in_a_helpers_own_account_serves_for_another() {
    assert!(!refused(judge_input_service(UID, true, UID, UID + 1)));
}

#[test]
fn an_input_service_outside_a_helpers_account_or_naming_another_is_refused() {
    // The person's account: no helper job loaded there.
    assert!(refused(judge_input_service(UID, false, UID, UID + 1)));
    // Serving as another account than its own.
    assert!(refused(judge_input_service(UID, true, UID + 9, UID + 1)));
    // Its controller is its own account.
    assert!(refused(judge_input_service(UID, true, UID, UID)));
}
