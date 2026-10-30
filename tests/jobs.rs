//! Background jobs with real processes: started through Atlas's own
//! launcher, supervised, stopped by their identity and found again by a
//! new supervisor. Each test keeps its jobs in a folder of its own and ends
//! every process it made.
#![cfg(target_os = "macos")]

use atlas::{
    daemon::{
        job::Jobs,
        protocol::{Expected, How, JobId, JobStart, JobView, StopReason},
    },
    process,
};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use serde_json::Value;
use std::{
    os::unix::fs::PermissionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

fn uid() -> u32 {
    nix::unistd::getuid().as_raw()
}

fn jobs_in(folder: &std::path::Path) -> Arc<Jobs> {
    Arc::new(Jobs::open(folder, env!("CARGO_BIN_EXE_atlas").into(), uid()).unwrap())
}

fn start(argv: &[&str]) -> JobStart {
    JobStart {
        argv: argv.iter().map(|argument| (*argument).to_owned()).collect(),
        lifetime: Duration::from_secs(60),
        name: None,
        task: None,
        session: None,
    }
}

async fn started(jobs: &Arc<Jobs>, argv: &[&str]) -> Value {
    let reply = jobs.start(start(argv)).await;
    assert_eq!(reply["ok"], true, "{reply}");
    reply["job"].clone()
}

fn id_of(job: &Value) -> JobId {
    JobId::parse(job["id"].as_str().unwrap()).unwrap()
}

fn expected_of(job: &Value) -> Expected {
    serde_json::from_value::<JobView>(job.clone())
        .unwrap()
        .expected()
        .unwrap()
}

fn pid_of(job: &Value) -> i32 {
    job["pid"].as_i64().unwrap() as i32
}

/// Watch `jobs` until job `id` has ended, or `within` passes.
fn ended(jobs: &Jobs, id: &JobId, within: Duration) -> Value {
    let until = Instant::now() + within;
    loop {
        jobs.watch_once(None);
        let shown = jobs.show(Some(id))["job"].clone();
        if shown["state"] == "ended" || Instant::now() >= until {
            return shown;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until `ready` holds, for at most five seconds.
fn eventually(mut ready: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    ready()
}

fn alive(pid: i32) -> bool {
    process::info(pid).unwrap().is_some_and(|info| !info.zombie)
}

fn end_group(pgid: i32) {
    let _ = killpg(Pid::from_raw(pgid), Signal::SIGKILL);
}

#[tokio::test]
async fn a_started_job_keeps_running_after_its_request_ends() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sleep", "30"]).await;
    assert_eq!(job["state"], "running");
    assert_eq!(job["name"], "sleep");
    std::thread::sleep(Duration::from_millis(300));
    jobs.watch_once(None);
    assert!(alive(pid_of(&job)));
    assert_eq!(jobs.running(), 1);
    end_group(pid_of(&job));
}

#[tokio::test]
async fn a_job_runs_in_a_session_and_group_of_its_own() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sleep", "30"]).await;
    let pid = pid_of(&job);
    assert_eq!(process::session(pid), Some(pid));
    assert_eq!(process::info(pid).unwrap().unwrap().pgid, pid);
    assert_ne!(pid, nix::unistd::getpgrp().as_raw());
    end_group(pid);
}

#[tokio::test]
async fn a_launch_that_cannot_exec_is_refused_and_leaves_nothing_running() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());

    let missing = jobs.start(start(&["/no/such/program"])).await;
    assert_eq!(missing["code"], "spawn-failed", "{missing}");
    assert_eq!(missing["uncertain"], false);

    let program = folder.path().join("not-a-program");
    std::fs::write(&program, "#!/no/such/interpreter\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let broken = jobs.start(start(&[program.to_str().unwrap()])).await;
    assert_eq!(broken["code"], "spawn-failed", "{broken}");
    assert_eq!(broken["uncertain"], false);

    let shown = jobs.show(None);
    assert_eq!(shown["jobs"], serde_json::json!([]), "no record is left");
    assert_eq!(jobs.running(), 0);
}

#[tokio::test]
async fn stopping_ends_the_whole_group_and_reports_what_left_it() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    // The escaping process's parent lingers, so it is seen while its link
    // to the job is still there to see. The escaped process says its own
    // PID, which exec keeps, so the test knows it exactly.
    let said = folder.path().join("escaped.pid");
    let script = format!(
        "sleep 60 & /usr/bin/perl -MPOSIX -e 'if (fork) {{ sleep 1; exit }} POSIX::setsid(); open(my $f, \">\", q({})) or die; print $f $$; close($f); exec q(/bin/sleep), 61' & wait",
        said.display()
    );
    let job = started(&jobs, &["/bin/sh", "-c", &script]).await;
    let id = id_of(&job);
    let pgid = pid_of(&job);
    let escaped = || {
        let pid: i32 = std::fs::read_to_string(&said).ok()?.trim().parse().ok()?;
        process::info(pid)
            .unwrap()
            .filter(|info| !info.zombie && info.name == "sleep" && info.pgid != pgid)
    };
    assert!(eventually(|| {
        jobs.watch_once(None);
        escaped().is_some()
    }));
    // Seen while its parent still runs in the job's group.
    for _ in 0..5 {
        jobs.watch_once(None);
        std::thread::sleep(Duration::from_millis(50));
    }

    let reply = jobs.stop(&id, &expected_of(&job), Duration::from_secs(2));
    assert_eq!(reply["signalled"], "SIGTERM", "{reply}");
    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["state"], "ended", "{shown}");
    assert_eq!(shown["how"], "stopped");
    assert!(
        process::group(pgid)
            .unwrap()
            .iter()
            .all(|member| member.zombie),
        "the group is gone"
    );
    let left = shown["left"].as_array().unwrap();
    assert_eq!(left.len(), 1, "{shown}");
    assert_eq!(left[0]["name"], "sleep");
    let escaped = escaped().expect("it still runs");
    assert_eq!(left[0]["pid"], escaped.instance.pid);
    let _ = kill(Pid::from_raw(escaped.instance.pid), Signal::SIGKILL);
}

#[tokio::test]
async fn a_job_that_ignores_term_is_killed_after_its_grace() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sh", "-c", "trap '' TERM; sleep 60"]).await;
    let id = id_of(&job);
    std::thread::sleep(Duration::from_millis(300));
    let reply = jobs.stop(&id, &expected_of(&job), Duration::from_secs(1));
    assert_eq!(reply["signalled"], "SIGTERM", "{reply}");
    std::thread::sleep(Duration::from_millis(500));
    jobs.watch_once(None);
    assert_eq!(jobs.show(Some(&id))["job"]["state"], "stopping");

    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["state"], "ended", "{shown}");
    assert_eq!(shown["signal"], "SIGKILL");
    assert_eq!(shown["exitSignal"], "SIGKILL");
}

#[tokio::test]
async fn the_lifetime_ends_a_job() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let mut asked = start(&["/bin/sleep", "60"]);
    asked.lifetime = Duration::from_secs(1);
    let reply = jobs.start(asked).await;
    let id = id_of(&reply["job"]);
    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["how"], "timeout", "{shown}");
}

#[tokio::test]
async fn a_stop_by_the_person_ends_every_job() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let first = started(&jobs, &["/bin/sleep", "60"]).await;
    let second = started(&jobs, &["/bin/sleep", "61"]).await;
    let reply = jobs.stop_all(StopReason::Person);
    assert_eq!(reply["stopping"], 2, "{reply}");
    for job in [first, second] {
        let shown = ended(&jobs, &id_of(&job), Duration::from_secs(8));
        assert_eq!(shown["how"], "person", "{shown}");
    }

    let third = started(&jobs, &["/bin/sleep", "62"]).await;
    let shown = {
        jobs.watch_once(Some(How::Person));
        ended(&jobs, &id_of(&third), Duration::from_secs(8))
    };
    assert_eq!(shown["how"], "person", "a saved Stop ends it too: {shown}");

    let fourth = started(&jobs, &["/bin/sleep", "63"]).await;
    jobs.watch_once(Some(How::OnConsole));
    let shown = ended(&jobs, &id_of(&fourth), Duration::from_secs(8));
    assert_eq!(shown["how"], "on-console", "{shown}");
}

#[tokio::test]
async fn a_job_outlives_its_supervisor_and_is_adopted_again() {
    let folder = tempfile::tempdir().unwrap();
    let job = {
        let jobs = jobs_in(folder.path());
        started(&jobs, &["/bin/sleep", "60"]).await
    };
    // The first supervisor's child is collected by nobody now; another
    // process stands in for launchd, which reaps it in the helper.
    let id = id_of(&job);
    let jobs = jobs_in(folder.path());
    let shown = jobs.show(Some(&id))["job"].clone();
    assert_eq!(shown["state"], "running", "{shown}");
    assert_eq!(shown["adopted"], true);
    assert!(alive(pid_of(&job)));

    let reply = jobs.stop(&id, &expected_of(&job), Duration::from_secs(2));
    assert_eq!(reply["signalled"], "SIGTERM", "{reply}");
    // The process is a child of this test's process, so it is reaped here
    // as launchd would reap it in the helper.
    let pid = Pid::from_raw(pid_of(&job));
    let _ = tokio::task::spawn_blocking(move || nix::sys::wait::waitpid(pid, None)).await;
    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["state"], "ended", "{shown}");
    assert_eq!(shown["exitCode"], Value::Null);
}

#[tokio::test]
async fn the_leaders_status_is_kept_until_its_group_ends() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sh", "-c", "sleep 2 & exit 3"]).await;
    let id = id_of(&job);
    std::thread::sleep(Duration::from_millis(500));
    jobs.watch_once(None);
    let shown = jobs.show(Some(&id))["job"].clone();
    assert_eq!(shown["state"], "running", "its child still runs: {shown}");
    assert!(
        process::info(pid_of(&job))
            .unwrap()
            .is_some_and(|leader| leader.zombie),
        "the leader is held unreaped"
    );

    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["how"], "exited", "{shown}");
    assert_eq!(shown["exitCode"], 3);
}

#[tokio::test]
async fn a_job_that_ends_at_once_is_reported_ended_with_its_exit() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let reply = jobs
        .start(start(&["/bin/sh", "-c", "echo hello; exit 3"]))
        .await;
    let id = id_of(&reply["job"]);
    let shown = ended(&jobs, &id, Duration::from_secs(5));
    assert_eq!(shown["exitCode"], 3, "{shown}");
    let log = jobs.log(&id, 100);
    let data = log["data"].as_str().unwrap();
    use base64::Engine;
    let text = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    assert_eq!(text, b"hello\n");
}

#[tokio::test]
async fn a_job_log_past_its_limit_starts_again() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = Arc::new(
        Jobs::open_with(
            folder.path(),
            env!("CARGO_BIN_EXE_atlas").into(),
            uid(),
            1000,
        )
        .unwrap(),
    );
    let job = started(
        &jobs,
        &["/bin/sh", "-c", "head -c 5000 /dev/zero; sleep 30"],
    )
    .await;
    let id = id_of(&job);
    assert!(eventually(|| {
        jobs.watch_once(None);
        jobs.show(Some(&id))["job"]["logResets"] == 1
    }));
    assert_eq!(jobs.log(&id, 100)["size"], 0);
    end_group(pid_of(&job));
}

#[tokio::test]
async fn nothing_but_its_own_group_is_ever_signalled() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let mut sibling = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let job = started(&jobs, &["/bin/sleep", "30"]).await;
    let id = id_of(&job);
    let mut stale = expected_of(&job);
    stale.start[1] ^= 1;
    let refused = jobs.stop(&id, &stale, Duration::from_secs(1));
    assert_eq!(refused["reason"], "identity", "{refused}");
    assert!(alive(pid_of(&job)), "a stop that disagrees signals nothing");

    jobs.stop(&id, &expected_of(&job), Duration::from_secs(1));
    let shown = ended(&jobs, &id, Duration::from_secs(8));
    assert_eq!(shown["how"], "stopped");
    jobs.stop_all(StopReason::Person);
    assert!(
        alive(sibling.id() as i32),
        "a process of the same name lives"
    );
    sibling.kill().unwrap();
    sibling.wait().unwrap();
}

#[tokio::test]
async fn a_record_that_cannot_be_read_is_shown_and_never_acted_on() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let id = JobId::new();
    std::fs::write(jobs.folder().join(format!("{id}.json")), b"{\"version\":2}").unwrap();
    let shown = jobs.show(Some(&id))["job"].clone();
    assert_eq!(shown["state"], "unknown", "{shown}");
    let refused = jobs.stop(
        &id,
        &Expected {
            boot: "x".into(),
            pid: 1,
            start: [1, 0],
            pgid: 1,
        },
        Duration::from_secs(1),
    );
    assert_eq!(refused["reason"], "unknown");
    let reopened = jobs_in(folder.path());
    assert_eq!(reopened.show(Some(&id))["job"]["state"], "unknown");
}

#[tokio::test]
async fn a_stop_the_client_leaves_is_finished_by_the_daemon() {
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sh", "-c", "trap '' TERM; sleep 60"]).await;
    let id = id_of(&job);
    std::thread::sleep(Duration::from_millis(300));
    // The client asks, hears TERM was sent, and goes away.
    assert_eq!(
        jobs.stop(&id, &expected_of(&job), Duration::from_secs(1))["signalled"],
        "SIGTERM"
    );
    // Only supervision runs from here.
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until && alive(pid_of(&job)) {
        jobs.watch_once(None);
        std::thread::sleep(Duration::from_millis(100));
    }
    jobs.watch_once(None);
    let shown = jobs.show(Some(&id))["job"].clone();
    assert_eq!(shown["state"], "ended", "{shown}");
    assert_eq!(shown["signal"], "SIGKILL");
}

#[tokio::test]
async fn a_set_user_id_style_member_of_the_helpers_own_is_still_stopped() {
    // A process whose saved user ID is the helper's is the helper's by the
    // kernel's own rule; here the job's own processes stand for it.
    let folder = tempfile::tempdir().unwrap();
    let jobs = jobs_in(folder.path());
    let job = started(&jobs, &["/bin/sleep", "60"]).await;
    let leader = process::info(pid_of(&job)).unwrap().unwrap();
    assert!(leader.owned_by(uid()));
    assert_eq!(leader.svuid, nix::unistd::geteuid().as_raw());
    jobs.stop(&id_of(&job), &expected_of(&job), Duration::from_secs(1));
    let shown = ended(&jobs, &id_of(&job), Duration::from_secs(8));
    assert_eq!(shown["how"], "stopped");
}
