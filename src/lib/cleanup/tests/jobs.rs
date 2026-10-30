//! Stopping background jobs: when in a cleanup they stop, and what a step
//! says when one cannot be stopped or its helper cannot be reached.
use super::*;

#[tokio::test]
async fn outputs_are_checked_first_and_jobs_stop_after_quits_before_plans() {
    let place = Place::new();
    let key = session("jobs-order");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        kinds(&reply),
        [
            "check-outputs",
            "quit",
            "stop-job",
            "undo-plan",
            "remove-copy"
        ]
    );
    assert_eq!(reply["steps"][2]["name"], "web");
    let log = fake.logged();
    let at = |what: &str| log.iter().position(|line| line == what).unwrap();
    assert!(at(&format!("check {receipt}")) < at(&format!("stop job {job}")));
    assert!(at(&format!("stop job {job}")) < at(&format!("undo {plan}")));

    let receipt = place.receipt(&reply);
    let stop = receipt
        .steps
        .iter()
        .find(|entry| matches!(entry.step, Step::StopJob { .. }))
        .unwrap();
    let undo = receipt
        .steps
        .iter()
        .find(|entry| matches!(entry.step, Step::UndoPlan { .. }))
        .unwrap();
    assert!(
        undo.after.contains(&stop.n),
        "a plan waits for the job's stop"
    );
}

#[tokio::test]
async fn a_job_that_already_ended_is_done_without_a_second_signal() {
    let place = Place::new();
    let key = session("ended-job");
    let task = default_task(&key);
    let job = job_started(&place, &task, "worker");
    let fake = Fake::at(&place.root);
    fake.jobs.borrow_mut().insert(
        job.to_string(),
        json!({"id": job, "state": "ended", "how": "stopped", "left": []}),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(states(&reply), ["done"]);
    assert!(
        !fake
            .logged()
            .iter()
            .any(|line| line.starts_with("stop job")),
        "nothing was sent again"
    );
}

#[tokio::test]
async fn a_job_step_says_what_left_and_why_it_could_not_stop() {
    let place = Place::new();
    let key = session("job-left");
    let task = default_task(&key);
    let left = job_started(&place, &task, "server");
    let stubborn = job_started(&place, &task, "stubborn");
    let foreign = job_started(&place, &task, "setuid");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        left.to_string(),
        Ok(json!({"ok": true, "state": "ended", "left": [{"pid": 77, "name": "sleep"}]})),
    );
    fake.job_stops.borrow_mut().insert(
        stubborn.to_string(),
        Ok(json!({"ok": false, "state": "stopping", "reason": "still-stopping"})),
    );
    fake.job_stops.borrow_mut().insert(
        foreign.to_string(),
        Err(
            Error::new(Code::Job, "Part of the job runs as another account")
                .with("reason", "other-account"),
        ),
    );

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["code"], "cleanup");
    let step = |n: usize| &reply["steps"][n];
    assert_eq!(step(0)["state"], "done");
    assert_eq!(step(0)["left"], json!([{"pid": 77, "name": "sleep"}]));
    assert_eq!(
        (&step(1)["state"], &step(1)["reason"], &step(1)["retry"]),
        (&json!("failed"), &json!("still-stopping"), &json!(true))
    );
    assert_eq!(
        (&step(2)["state"], &step(2)["reason"]),
        (&json!("failed"), &json!("other-account"))
    );
    assert!(step(2).get("retry").is_none(), "nothing more is tried");
}

#[tokio::test]
async fn a_job_whose_helper_cannot_be_reached_is_blocked_and_a_lost_stop_is_uncertain() {
    let place = Place::new();
    let key = session("job-blocked");
    let task = default_task(&key);
    let unreachable = job_started(&place, &task, "a");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        unreachable.to_string(),
        Err(Error::new(
            Code::Daemon,
            "The helper is running an older Atlas",
        )),
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["state"], "blocked", "{reply}");
    assert_eq!(reply["steps"][0]["reason"], "daemon");

    let place = Place::new();
    let task = default_task(&key);
    let lost = job_started(&place, &task, "b");
    let fake = Fake::at(&place.root);
    fake.job_stops.borrow_mut().insert(
        lost.to_string(),
        Err(Error::new(Code::Transport, "Helper exceeded the request deadline").uncertain()),
    );
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["state"], "uncertain", "{reply}");
    assert_eq!(reply["uncertain"], true);
}

#[tokio::test]
async fn a_job_only_the_helpers_record_knows_is_stopped_too() {
    let place = Place::new();
    let key = session("unnoted");
    let task = default_task(&key);
    place.claim(&task, Owned::Plan { id: Token::new() });
    let fake = Fake::at(&place.root);
    let id = crate::daemon::protocol::JobId::new();
    fake.daemon_jobs.borrow_mut().push(plan::Job {
        id: id.clone(),
        task: task.key().clone(),
        name: "worker".into(),
    });
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(kinds(&reply), ["stop-job", "undo-plan"], "{reply}");
    assert_eq!(reply["steps"][0]["name"], "worker");
    assert!(fake.logged().contains(&format!("stop job {id}")));
}

#[tokio::test]
async fn a_job_list_that_cannot_be_read_holds_every_removal_back_until_it_can() {
    let place = Place::new();
    let key = session("unlisted");
    let task = default_task(&key);
    let plan = plan_claimed(&place, &task);
    let known = job_started(&place, &task, "web");
    let fake = Fake::at(&place.root);
    fake.unlisted.set(true);
    let unknown = crate::daemon::protocol::JobId::new();
    fake.daemon_jobs.borrow_mut().push(plan::Job {
        id: unknown.clone(),
        task: task.key().clone(),
        name: "worker".into(),
    });

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(kinds(&reply), ["find-jobs", "stop-job", "undo-plan"]);
    assert_eq!(states(&reply), ["failed", "done", "failed"]);
    assert_eq!(reply["steps"][0]["reason"], "daemon");
    assert!(
        reply["steps"][0]["error"]
            .as_str()
            .unwrap()
            .contains("could not be read"),
        "{reply}"
    );
    assert_eq!(reply["steps"][2]["reason"], "needs-step");
    assert!(
        fake.logged().contains(&format!("stop job {known}")),
        "a known job still stops"
    );
    assert!(
        !fake.logged().contains(&format!("undo {plan}")),
        "nothing was removed"
    );

    // Readable again: the job it alone knows is stopped, then the rest runs.
    fake.unlisted.set(false);
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("stop job {unknown}")));
    assert!(fake.logged().contains(&format!("undo {plan}")));
}

#[tokio::test]
async fn while_an_outcome_is_unknown_only_jobs_stop_and_copies_are_checked() {
    let place = Place::new();
    let key = session("unknown-outcome");
    let task = default_task(&key);
    let copy = place.copy(Some(&key));
    place.claim(&task, Owned::Copy { id: copy.clone() });
    let plan = plan_claimed(&place, &task);
    let job = job_started(&place, &task, "web");
    let receipt = collected(&place, &task);
    let fake = Fake::at(&place.root);
    fake.unknown.set(true);

    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["state"], "incomplete");
    assert_eq!(reply["uncertain"], false);
    assert_eq!(
        kinds(&reply),
        [
            "check-outputs",
            "quit",
            "stop-job",
            "undo-plan",
            "remove-copy"
        ]
    );
    assert_eq!(
        states(&reply),
        ["done", "blocked", "done", "blocked", "blocked"]
    );
    for n in [1, 3, 4] {
        assert_eq!(reply["steps"][n]["reason"], "uncertain", "{reply}");
    }
    assert!(
        reply["error"]
            .as_str()
            .unwrap()
            .contains("outcome is unknown")
    );
    let log = fake.logged();
    assert!(log.contains(&format!("check {receipt}")));
    assert!(log.contains(&format!("stop job {job}")));
    assert!(!log.contains(&format!("undo {plan}")), "nothing is undone");
    assert!(place.folder(&copy).exists(), "nothing is removed");

    // Recovered, the rest runs.
    fake.unknown.set(false);
    let reply = place.apply(&fake, &own(&key)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(fake.logged().contains(&format!("undo {plan}")));
}
