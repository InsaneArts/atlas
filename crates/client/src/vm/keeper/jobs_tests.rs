//! A VM's background jobs end to end, through a real keeper and a real
//! client, its guest a stand-in that keeps jobs as a machine guest does:
//! each runs past the command that started it and ends with the generation
//! it started in, or with a Stop the guest took after it started.
use super::tests::{admitted, asked_for, live, proven, restarted, settled};
use crate::{
    client::{Leftovers, Mutation},
    doing::Doing,
    vm::keeper::{
        clients::tests::{Serving, asked, client_of},
        tests::{Saying, until},
    },
};
use atlas_core::{
    error::Code,
    files::Token,
    protocol::{
        EndedView, How, JobId, JobReply, JobState, JobView, Logged, Progress, SessionPin, Showing,
        Shown, StopCameTo, Stopped,
    },
};
use atlas_session::{lane::LaneKind, standin::StandIn};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
    time::Duration,
};

/// One job the guest started.
struct Kept {
    view: JobView,
    /// The generation it started in, which it ends with.
    generation: u64,
    /// How many Stops the guest had taken when it started.
    stops: usize,
    /// Whether an agent stopped it.
    stopped: bool,
    /// How many times its log was read: it says one line more each time.
    logged: usize,
}

/// A machine guest's background jobs, as a test keeps them.
#[derive(Default)]
struct Jobs {
    kept: Mutex<Vec<Kept>>,
    /// Every job change it was asked for, in order.
    asked: Mutex<Vec<Value>>,
    /// What it says of its leftover work, in place of what its jobs say.
    leftovers: Mutex<Option<&'static str>>,
    /// Whether a start is carried out and never answered, as one whose
    /// program has not said it started yet.
    unanswered: AtomicBool,
}

impl Jobs {
    /// Answer `request`, on a lane of `kind` in `generation`, as a guest
    /// that keeps jobs does.
    fn answer(
        &self,
        standin: &StandIn,
        kind: &LaneKind,
        generation: u64,
        request: &Value,
    ) -> Value {
        self.end(standin);
        match (kind, request["op"].as_str(), request["action"].as_str()) {
            (LaneKind::Change { .. }, Some("job"), Some("start")) => {
                self.asked.lock().unwrap().push(request.clone());
                let view = view(request["uid"].as_u64().unwrap() as u32);
                self.kept.lock().unwrap().push(Kept {
                    view: view.clone(),
                    generation,
                    stops: standin.stops.load(SeqCst),
                    stopped: false,
                    logged: 0,
                });
                if self.unanswered.load(SeqCst) {
                    return Value::Null;
                }
                JobReply::Start { job: view }.into_value()
            }
            (LaneKind::Change { .. }, Some("job"), Some("stop")) => {
                self.asked.lock().unwrap().push(request.clone());
                let mut kept = self.kept.lock().unwrap();
                let Some(job) = kept
                    .iter_mut()
                    .find(|job| request["id"] == json!(job.view.id))
                else {
                    return json!({"ok": false, "code": "job", "reason": "unknown",
                        "error": "No such job runs here"});
                };
                job.stopped = true;
                let mut stopping = job.view.clone();
                stopping.state = JobState::Stopping;
                JobReply::Stop(Stopped {
                    job: stopping,
                    came_to: StopCameTo::Signalled("SIGTERM".into()),
                })
                .into_value()
            }
            (LaneKind::Read, Some("job"), Some("show")) => {
                let kept = self.kept.lock().unwrap();
                let shown = kept
                    .iter()
                    .map(|job| Shown::Job(Box::new(job.view.clone())));
                let showing = match request["id"].as_str() {
                    Some(id) => Showing::One {
                        job: shown
                            .into_iter()
                            .find(|shown| shown.job().is_some_and(|job| job.id.as_str() == id)),
                    },
                    None => Showing::Every {
                        jobs: shown.collect(),
                    },
                };
                JobReply::Show(showing).into_value()
            }
            (LaneKind::Read, Some("job"), Some("log")) => {
                use base64::Engine;
                let mut kept = self.kept.lock().unwrap();
                let Some(job) = kept
                    .iter_mut()
                    .find(|job| request["id"] == json!(job.view.id))
                else {
                    return json!({"ok": false, "code": "job", "reason": "unknown"});
                };
                job.logged += 1;
                let said = "tick\n".repeat(job.logged);
                JobReply::Log(Logged {
                    id: job.view.id.clone(),
                    task: None,
                    data: base64::engine::general_purpose::STANDARD.encode(&said),
                    size: said.len() as u64,
                    output_truncated: false,
                    log_resets: 0,
                })
                .into_value()
            }
            (LaneKind::Read, Some("leftovers"), _) => {
                let running = self
                    .kept
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|job| job.view.state != JobState::Ended);
                let said = self.leftovers.lock().unwrap().unwrap_or(if running {
                    "running"
                } else {
                    "none"
                });
                json!({"ok": true, "op": "leftovers", "leftovers": said})
            }
            _ => json!({"ok": true, "echo": request}),
        }
    }

    /// End each job as the guest would by now: with a Stop it took after
    /// the job started, with its generation gone, or with its own stop.
    fn end(&self, standin: &StandIn) {
        let (stops, retired) = (
            standin.stops.load(SeqCst),
            standin.retired.lock().unwrap().clone(),
        );
        for job in self.kept.lock().unwrap().iter_mut() {
            if job.view.state == JobState::Ended {
                continue;
            }
            let how = if stops > job.stops {
                How::Person
            } else if retired
                .iter()
                .any(|(generation, _)| generation.0 == job.generation)
            {
                How::Gone
            } else if job.stopped {
                How::Stopped
            } else {
                continue;
            };
            job.view.state = JobState::Ended;
            job.view.progress = Progress::Ended(EndedView {
                how,
                ended_at: 2,
                signal: job.stopped.then(|| "SIGTERM".into()),
                exit_code: None,
                exit_signal: None,
                left: Vec::new(),
                remaining: None,
            });
        }
    }

    fn asked(&self) -> Vec<Value> {
        self.asked.lock().unwrap().clone()
    }
}

/// A job running as account `uid`, just started.
fn view(uid: u32) -> JobView {
    JobView {
        id: JobId::new(),
        name: "sh".into(),
        task: None,
        session: None,
        uid,
        boot: "boot-1".into(),
        pid: Some(4321),
        start: Some([1_700_000_000, 5]),
        pgid: Some(4321),
        state: JobState::Running,
        started_at: 1,
        ends_at: u64::MAX,
        log_resets: 0,
        adopted: false,
        progress: Progress::Neither {},
    }
}

/// The guest of `serving`, saying what `saying` says, keeping jobs.
fn keeping(serving: &Serving, saying: &Saying) -> Arc<Jobs> {
    let jobs = Arc::new(Jobs::default());
    let (kept, standin) = (jobs.clone(), serving.fixture.guest.standin.clone());
    *saying.otherwise.lock().unwrap() = Some(Arc::new(move |header, request| {
        kept.answer(&standin, &header.kind, header.generation.0, &request)
    }));
    jobs
}

fn looping() -> Vec<String> {
    ["/bin/sh", "-c", "while :; do date; sleep 1; done"]
        .map(str::to_owned)
        .to_vec()
}

/// What the guest shows of every job, read by another command.
async fn listed(serving: &Serving, directory: &std::path::Path) -> Vec<Value> {
    let other = client_of(serving, directory).doing(Doing::new("job-list"));
    let listed = crate::jobs::list(&other, true).await.unwrap();
    listed["jobs"].as_array().unwrap().clone()
}

/// The change token a job change the guest was asked for names.
fn change_of(asked: &Value) -> Token {
    Token::parse(asked["change"].as_str().unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_runs_on_after_the_command_that_started_it_and_ends_with_its_generation_as_gone() {
    let (serving, saying, client, directory) = proven().await;
    let jobs = keeping(&serving, &saying);
    let generation = live(&serving);

    let started = crate::jobs::start(&client, looping(), Duration::from_secs(120), None)
        .await
        .unwrap();
    drop(client);

    assert_eq!(started["ok"], true, "{started}");
    assert_eq!(started["state"], "running", "{started}");
    let id = JobId::parse(started["id"].as_str().unwrap()).unwrap();
    let asked = jobs.asked();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0]["uid"], 502, "the account its proof names");
    assert_eq!(asked[0]["lifetimeMs"], 120_000);
    assert_eq!(
        asked[0]["session"],
        json!(SessionPin {
            security_session_id: 42,
            display_id: 7
        })
    );
    let change = change_of(&asked[0]);
    assert!(admitted(&serving).iter().any(|header| {
        header.kind
            == LaneKind::Change {
                change: change.clone(),
            }
            && header.generation.0 == generation
    }));
    until("settled with the guest", || {
        settled(&serving).contains(&change)
    })
    .await;

    // Its command gone, another reads it running, and its log growing.
    let running = listed(&serving, directory.path()).await;
    assert_eq!(running.len(), 1, "{running:?}");
    assert_eq!(running[0]["state"], "running", "{running:?}");
    let reading = client_of(&serving, directory.path()).doing(Doing::new("job-log"));
    let first = crate::jobs::log(&reading, &id, 4096, None).await.unwrap();
    let second = crate::jobs::log(&reading, &id, 4096, None).await.unwrap();
    let (first, second) = (
        first["output"].as_str().unwrap().to_owned(),
        second["output"].as_str().unwrap().to_owned(),
    );
    assert!(
        second.len() > first.len() && second.starts_with(&first),
        "{first:?} then {second:?}"
    );
    assert!(reading.controller.pending().unwrap().is_empty());

    // Its generation lost: it ended with it.
    restarted(&serving).await;
    let ended = listed(&serving, directory.path()).await;
    assert_eq!(ended[0]["state"], "ended", "{ended:?}");
    assert_eq!(ended[0]["how"], "gone", "{ended:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_mid_start_leaves_its_change_pending_and_the_job_ended_by_the_person() {
    let (serving, saying, client, directory) = proven().await;
    let jobs = keeping(&serving, &saying);
    jobs.unanswered.store(true, SeqCst);
    let starting = tokio::spawn({
        let client = client.clone();
        async move { crate::jobs::start(&client, looping(), Duration::from_secs(120), None).await }
    });
    let change = asked_for(&serving).await;

    client.controller.note_stop(true).unwrap();
    let stopped = starting.await.unwrap().unwrap_err();

    assert_eq!(stopped.code, Code::Stopped, "{stopped:?}");
    assert!(stopped.uncertain, "{stopped:?}");
    assert!(
        stopped.details["next"]
            .as_str()
            .is_some_and(|next| next.contains("stays pending until work answers")),
        "{stopped:?}"
    );
    assert_eq!(client.controller.pending().unwrap(), [change.as_str()]);
    until("the guest stopped", || {
        serving.fixture.guest.standin.stops.load(SeqCst) > 0
    })
    .await;
    let ended = listed(&serving, directory.path()).await;
    assert_eq!(ended[0]["state"], "ended", "{ended:?}");
    assert_eq!(ended[0]["how"], "person", "{ended:?}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!settled(&serving).contains(&change), "still claimed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_start_for_a_generation_that_ended_is_refused_unsaid_and_its_claim_cleared() {
    let (serving, saying, client, _directory) = proven().await;
    let jobs = keeping(&serving, &saying);
    let stale = live(&serving);
    restarted(&serving).await;
    let mutation = Mutation::JobStart {
        start: atlas_core::protocol::JobStart {
            argv: looping(),
            lifetime: Duration::from_secs(120),
            name: None,
            task: None,
            session: None,
        },
        uid: 502,
        pin: SessionPin {
            security_session_id: 42,
            display_id: 7,
        },
    };

    let refused = client.mutate_vm(&mutation, stale, None).await.unwrap();

    assert_eq!(refused["code"], "session-changed", "{refused}");
    assert_eq!(refused["sent"], false);
    assert!(client.controller.pending().unwrap().is_empty(), "cleared");
    assert!(jobs.asked().is_empty(), "nothing said");
    assert!(
        admitted(&serving)
            .iter()
            .all(|header| !matches!(header.kind, LaneKind::Change { .. }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_stops_lost_reply_is_recovered_from_the_guests_record_and_settled() {
    let (serving, saying, client, _directory) = proven().await;
    let jobs = keeping(&serving, &saying);
    let started = crate::jobs::start(&client, looping(), Duration::from_secs(120), None)
        .await
        .unwrap();
    let id = JobId::parse(started["id"].as_str().unwrap()).unwrap();
    serving.fixture.guest.standin.dropping.store(1, SeqCst);

    let stopped = crate::jobs::stop(&client, &id, Duration::from_secs(1), None)
        .await
        .unwrap();

    assert_eq!(stopped["ok"], true, "{stopped}");
    assert_eq!(stopped["state"], "ended", "{stopped}");
    assert_eq!(stopped["how"], "stopped", "{stopped}");
    let asked = jobs.asked();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert_eq!(asked[1]["action"], "stop");
    assert_eq!(asked[1]["id"], json!(id));
    assert_eq!(
        serving.fixture.guest.standin.dropping.load(SeqCst),
        0,
        "dropped"
    );
    assert!(client.controller.pending().unwrap().is_empty());
    let stop = change_of(&asked[1]);
    until("settled with the guest", || {
        settled(&serving).contains(&stop)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guests_word_on_its_leftover_work_decides_and_one_it_cannot_tell_holds() {
    let (serving, saying, client, _directory) = proven().await;
    let jobs = keeping(&serving, &saying);
    let said = |leftovers: Option<&'static str>| *jobs.leftovers.lock().unwrap() = leftovers;

    assert_eq!(client.leftovers().await.unwrap(), Leftovers::None);
    crate::jobs::require_none(&client).await.unwrap();

    for (word, read) in [
        ("unknown", Leftovers::Unknown),
        ("running", Leftovers::Running),
        ("later", Leftovers::Unknown),
    ] {
        said(Some(word));
        assert_eq!(client.leftovers().await.unwrap(), read, "{word}");
        let held = crate::jobs::require_none(&client).await.unwrap_err();
        assert_eq!(held.code, Code::Busy, "{word}: {held:?}");
        assert_eq!(held.details["running"], "job");
    }

    // A job the guest still runs holds it though the guest said none, and
    // one ended there does not.
    said(None);
    crate::jobs::start(&client, looping(), Duration::from_secs(120), None)
        .await
        .unwrap();
    assert_eq!(client.leftovers().await.unwrap(), Leftovers::Running);
    said(Some("none"));
    assert_eq!(
        crate::jobs::require_none(&client).await.unwrap_err().code,
        Code::Busy
    );
    restarted(&serving).await;
    crate::jobs::require_none(&client).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reads_a_vms_jobs_need_are_carried_and_every_other_read_is_still_refused() {
    let (serving, saying, client, _directory) = proven().await;
    keeping(&serving, &saying);
    let read = |request: Value| json!({"op": "read", "request": request});

    let carried = [
        json!({"op": "job", "action": "show"}),
        json!({"op": "job", "action": "show", "id": JobId::new()}),
        json!({"op": "job", "action": "log", "id": JobId::new(), "bytes": 16}),
        json!({"op": "leftovers"}),
    ];
    for request in carried {
        let answered = asked(&serving.socket, &read(request.clone())).await;
        assert_ne!(answered[0]["code"], "denied", "{request}: {answered:?}");
        assert_ne!(answered[0]["echo"], request, "{request}");
    }
    let refused = [
        json!({"op": "job", "action": "stop-all", "reason": "person"}),
        json!({"op": "job", "action": "start", "argv": ["/bin/sh"]}),
        json!({"op": "job"}),
        json!({"op": "apps"}),
        json!({"op": "elements"}),
        json!({"op": "run", "argv": ["/bin/sh"]}),
    ];
    let before = admitted(&serving).len();
    for request in refused {
        let answered = asked(&serving.socket, &read(request.clone())).await;
        assert_eq!(answered[0]["code"], "denied", "{request}: {answered:?}");
        let unsent = client.link.call(&request).await.unwrap_err();
        assert_eq!(unsent.error, client.unreachable().error, "{request}");
    }
    assert_eq!(admitted(&serving).len(), before, "no lane opened");
    let everyone = crate::jobs::stop_every(&client, atlas_core::protocol::StopReason::Person).await;
    assert_eq!(everyone.asked, 0);
    assert!(!everyone.warnings().is_empty(), "never said as done");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_vms_status_says_its_guests_stop_work_and_leftovers() {
    let (serving, saying, client, _directory) = proven().await;
    keeping(&serving, &saying);
    *saying.hello.lock().unwrap() = Some(|hello| {
        hello["work"] = json!({"state": "running", "jobs": 1, "changes": 1, "supervisors": 0});
    });
    crate::jobs::start(&client, looping(), Duration::from_secs(120), None)
        .await
        .unwrap();

    let status = client.status().await;

    assert_eq!(status["stop"]["state"], "clear", "{status}");
    assert_eq!(status["work"]["state"], "running", "{status}");
    assert_eq!(status["work"]["jobs"], 1);
    assert_eq!(status["leftovers"], "running");
    let reading = crate::overview::read(&client).await;
    let overview = &reading.value["overview"];
    assert_eq!(overview["guest"]["leftovers"], "running", "{overview}");
    assert_eq!(overview["guest"]["work"]["state"], "running");
    assert_eq!(overview["jobs"]["running"], 1, "{overview}");
    assert_eq!(overview["workMayRun"], true, "the person's Stop offered");
    assert!(
        overview["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Work an agent started is still running")),
        "{overview}"
    );
}
