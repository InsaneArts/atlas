use super::*;
use std::io::{BufRead, BufReader, Cursor};

// The test harness writes its own lines around a subprocess test. Only
// JSON lines belong to the production worker protocol exercised here.
struct JsonInput<R> {
    input: R,
    bytes: Cursor<Vec<u8>>,
}
impl<R: BufRead> Read for JsonInput<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let count = self.bytes.read(output)?;
            if count > 0 {
                return Ok(count);
            }
            let mut line = Vec::new();
            if self.input.read_until(b'\n', &mut line)? == 0 {
                return Ok(0);
            }
            if line.starts_with(b"{") {
                self.bytes = Cursor::new(line);
            }
        }
    }
}
struct Channel<R, W>(JsonInput<R>, W);
impl<R: BufRead, W> Read for Channel<R, W> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(bytes)
    }
}
impl<R, W: Write> Write for Channel<R, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.1.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.1.flush()
    }
}
fn channel<R: BufRead, W>(input: R, output: W) -> Channel<R, W> {
    Channel(
        JsonInput {
            input,
            bytes: Cursor::new(Vec::new()),
        },
        output,
    )
}
fn pairing() -> atlas_daemon::user::setup::pairing::Pairing {
    atlas_daemon::user::setup::pairing::Pairing {
        ticket: "a".repeat(atlas_core::files::TOKEN_BYTES * 2),
        controller_uid: nix::unistd::getuid().as_raw(),
        controller_name: "requester".into(),
        helper_uid: 7001,
        helper_name: "helper".into(),
        helper_identity: atlas_daemon::identity::Identity {
            uid: 7001,
            name: "helper".into(),
            generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
        },
    }
}

#[test]
fn acceptance_reads_only_a_bounded_plain_file_without_waiting_for_a_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("accepted.json");
    let mut pairing = pairing();
    pairing.helper_uid = nix::unistd::getuid().as_raw();
    let bytes = serde_json::to_vec(&pairing).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(accepted_at(&path, &pairing));

    let link = dir.path().join("link.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(!accepted_at(&link, &pairing));
    fs::write(&path, [bytes, vec![b' '; 64 * 1024]].concat()).unwrap();
    assert!(!accepted_at(&path, &pairing));

    fs::remove_file(&path).unwrap();
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
    assert!(!accepted_at(&path, &pairing));
}
fn agent(dir: &Path) -> atlas_daemon::user::setup::Agent {
    atlas_daemon::user::setup::Agent {
        uid: 7001,
        controller_uid: pairing().controller_uid,
        label: atlas_daemon::user::setup::LABEL.into(),
        program: dir.join("Atlas"),
        socket: dir.join("helper/c.sock"),
    }
}
fn worker(dir: &Path, stage: &str) -> atlas_client::test_support::Fixture {
    atlas_client::test_support::Fixture::spawn(
        dir,
        atlas_core::test_support::rerun(
            "user::onboarding::acceptance::tests::an_acceptance_test_worker",
        )
        .arg("--nocapture")
        .env("ATLAS_ACCEPTANCE_TEST_DIRECTORY", dir)
        .env("ATLAS_ACCEPTANCE_TEST_STAGE", stage)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null()),
    )
    .unwrap()
}

#[test]
fn interrupted_acceptance_keeps_the_requesters_claim_until_its_worker_ends() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::interrupted_acceptance_keeps_the_requesters_claim_until_its_worker_ends",
    ) {
        return;
    }
    for stage in [
        "prepare",
        "retire",
        "install",
        "restore",
        "opener",
        "kickstart",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, &dir.path().join("requester/state"));
        let mut worker = worker(dir.path(), stage);
        let pid = worker.caller.id();
        worker
            .request(
                atlas_core::test_support::rerun(
                    "user::onboarding::acceptance::tests::an_acceptance_test_requester",
                )
                .arg("--nocapture")
                .env("ATLAS_ACCEPTANCE_TEST_DIRECTORY", dir.path())
                .env("ATLAS_ACCEPTANCE_TEST_PID", pid.to_string())
                .stderr(Stdio::null()),
            )
            .unwrap();
        worker.ready().unwrap();
        let before = controller.pending().unwrap();
        worker.interrupt().unwrap();
        let excluded = controller.acquire(true).is_err();
        let after = controller.pending().unwrap();
        worker.finish().unwrap();
        worker.caller.wait().unwrap();
        assert_eq!(before.len(), 1, "a claim precedes {stage}");
        assert_eq!(after, before, "the interrupted claim survives {stage}");
        assert!(
            excluded,
            "the still-running worker excludes teardown after {stage}"
        );
        assert!(
            controller.acquire(true).is_ok(),
            "worker ended after {stage}"
        );
        assert_eq!(
            controller.pending().unwrap(),
            before,
            "unknown outcomes require recovery"
        );
        assert!(!dir.path().join("helper/state").exists());
    }
}

#[test]
fn acceptance_excludes_competing_teardown_through_completion_and_rollback() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::acceptance_excludes_competing_teardown_through_completion_and_rollback",
    ) {
        return;
    }
    for stage in [
        "prepare",
        "retire",
        "install",
        "restore",
        "opener",
        "kickstart",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let controller = Controller::at(7001, &dir.path().join("requester/state"));
        let previous = controller.quarantine().unwrap();
        let generation = previous.generation().to_owned();
        drop(previous);
        let mut worker = worker(dir.path(), stage);
        let pid = worker.caller.id();
        worker
            .request(
                atlas_core::test_support::rerun(
                    "user::onboarding::acceptance::tests::an_acceptance_test_requester",
                )
                .arg("--nocapture")
                .env("ATLAS_ACCEPTANCE_TEST_DIRECTORY", dir.path())
                .env("ATLAS_ACCEPTANCE_TEST_PID", pid.to_string())
                .stderr(Stdio::null()),
            )
            .unwrap();
        worker.ready().unwrap();
        let excluded = controller.acquire(true).is_err();
        let pending = controller.pending().unwrap();
        worker.finish().unwrap();
        worker.wait().unwrap();
        let succeeded = fs::read_to_string(dir.path().join("result")).unwrap() == "true";
        assert!(excluded, "teardown must not enter during {stage}");
        assert_eq!(
            fs::read_to_string(dir.path().join("ownership-checked")).unwrap(),
            "true",
            "ownership precedes the claim and every dispatch: {stage}"
        );
        assert_eq!(pending.len(), 2, "{stage}");
        assert_eq!(succeeded, stage != "restore", "{stage}");
        let remaining = controller.pending().unwrap();
        assert!(
            remaining.contains(&generation),
            "unrelated pending work stays"
        );
        assert_eq!(
            remaining.len(),
            if stage == "restore" { 2 } else { 1 },
            "{stage}"
        );
    }
}

#[test]
fn acceptance_refuses_dispatch_during_teardown_or_after_the_pairing_changes() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::acceptance_refuses_dispatch_during_teardown_or_after_the_pairing_changes",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(7001, dir.path());
    let teardown = controller.acquire(true).unwrap();
    let mut output = Vec::new();
    let result = drive(
        &controller,
        nix::unistd::getpid(),
        channel(Cursor::new(b""), &mut output),
        &pairing(),
        || Ok(()),
        agent(dir.path()),
        dir.path(),
    );
    assert!(result.is_err());
    assert!(
        output.is_empty(),
        "teardown acquired first, so no worker command runs"
    );
    drop(teardown);
    let result = drive(
        &controller,
        nix::unistd::getpid(),
        channel(Cursor::new(b""), &mut output),
        &pairing(),
        || Err(problem("Teardown removed the request")),
        agent(dir.path()),
        dir.path(),
    );
    assert!(result.is_err());
    assert!(output.is_empty(), "pairing is rechecked under ownership");
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn acceptance_makes_zero_authorization_calls() {
    let pairing = pairing();
    let (result, calls) = atlas_native::approval::without_authorization(|| {
        allow(
            (pairing.controller_uid, &pairing.controller_name),
            (pairing.helper_uid, &pairing.helper_name),
            false,
            Some(false),
            Some(pairing.clone()),
            |_| signal_using(Command::new("/bin/sh").args(["-c", "printf 'ready\\n'"])),
        )
    });
    assert_eq!(
        calls, 0,
        "Allow must not ask for another administrator approval"
    );
    assert_eq!(result.unwrap(), json!({"ok":true,"accepted":true}));
}

struct ObservedHelper {
    directory: tempfile::TempDir,
    controller: Controller,
    inspection_unavailable: std::cell::Cell<bool>,
}

impl ObservedHelper {
    fn receipt(&self, _: u32) -> Result<PathBuf> {
        Ok(self.directory.path().join("receipt.json"))
    }

    async fn job(&self) -> Result<atlas_daemon::user::setup::launchd::JobReport> {
        if self.inspection_unavailable.replace(false) {
            return Err(problem("The helper is not answering"));
        }
        let path = self.directory.path().join("job.json");
        let value: Value = serde_json::from_slice(&fs::read(path)?)?;
        Ok(atlas_daemon::user::setup::launchd::JobReport::from(&value))
    }

    async fn resume(&self, pairing: atlas_daemon::user::setup::pairing::Pairing) -> Result<()> {
        let root = self.directory.path();
        let controller = &self.controller;
        std::thread::scope(|scope| {
            // Reap the worker on failure before the scope joins its requester.
            let mut worker = worker(root, "repair");
            let pid = nix::unistd::Pid::from_raw(worker.caller.id().try_into().unwrap());
            let stream = channel(
                BufReader::new(worker.caller.stdout.take().unwrap()),
                worker.caller.stdin.take().unwrap(),
            );
            let requester = scope.spawn(move || {
                drive(
                    controller,
                    pid,
                    stream,
                    &pairing,
                    || {
                        assert!(controller.acquire(true).is_err());
                        Ok(())
                    },
                    agent(root),
                    root,
                )
            });
            worker.ready().unwrap();
            assert_eq!(self.controller.pending().unwrap().len(), 2);
            assert!(self.controller.acquire(true).is_err());
            worker.finish().unwrap();
            worker.wait().unwrap();
            requester.join().unwrap()
        })
    }

    fn pending(&self) -> Result<Vec<String>> {
        self.controller.pending()
    }
}

/// The status loop's decision over this helper's observations.
async fn observe(
    observed: &ObservedHelper,
    accepted: atlas_daemon::user::setup::pairing::Pairing,
) -> Value {
    reconcile(
        accepted == pairing(),
        observed.receipt(accepted.helper_uid),
        accepted,
        async || observed.job().await,
        async |pairing| observed.resume(pairing).await,
        || observed.pending(),
    )
    .await
}

fn observe_repair(plist_kind: &str, loaded: bool, pid: Option<u32>, unavailable: bool) {
    let directory = tempfile::tempdir().unwrap();
    let controller = Controller::at(7001, &directory.path().join("requester/state"));
    let previous = controller.quarantine().unwrap();
    let generation = previous.generation().to_owned();
    drop(previous);
    let observed = ObservedHelper {
        directory,
        controller,
        inspection_unavailable: std::cell::Cell::new(unavailable),
    };
    let receipt = observed.receipt(7001).unwrap();
    let expected = serde_json::to_vec(&pairing()).unwrap();
    atlas_core::files::save_private(&receipt, &expected).unwrap();
    fs::write(
        observed.directory.path().join("job.json"),
        serde_json::to_vec(&json!({
            "plistKind":plist_kind,"loaded":loaded,"pid":pid,
            "path":"/helper/Library/LaunchAgents/helper.plist",
            "plist":"/helper/Library/LaunchAgents/helper.plist",
        }))
        .unwrap(),
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let initial = runtime.block_on(observed.job());
    if unavailable {
        assert!(initial.is_err(), "an unreported job is not a missing job");
        observed.inspection_unavailable.set(true);
    } else {
        let initial = initial.unwrap();
        assert_eq!(
            initial.definition,
            atlas_daemon::user::setup::launchd::JobReport::from(&json!({"plistKind": plist_kind}))
                .definition
        );
        assert_eq!(initial.loaded, Some(loaded));
        assert_eq!(initial.pid, pid.map(u64::from));
    }
    assert_eq!(observed.pending().unwrap(), vec![generation.clone()]);
    let (result, calls) = atlas_native::approval::without_authorization(|| {
        runtime.block_on(observe(&observed, pairing()))
    });
    assert_eq!(calls, 0, "repair must reuse consent without authorization");
    assert_eq!(result, json!({"accepted":true,"complete":true}));
    let repaired = runtime.block_on(observed.job()).unwrap();
    assert_eq!(
        repaired.definition,
        atlas_daemon::user::setup::launchd::Definition::Regular,
        "repair restores the plist"
    );
    assert_eq!(repaired.loaded, Some(true), "repair loads the helper job");
    assert_eq!(repaired.pid, Some(42), "repair starts the helper job");
    assert_eq!(
        fs::read_to_string(observed.directory.path().join("authorization-calls")).unwrap(),
        "0",
        "the unprivileged worker must not request authorization either"
    );
    assert_eq!(observed.pending().unwrap(), vec![generation]);
    assert_eq!(fs::read(&receipt).unwrap(), expected);
    assert!(observed.controller.acquire(true).is_ok());

    fs::remove_file(observed.directory.path().join("changed")).unwrap();
    let (result, calls) = atlas_native::approval::without_authorization(|| {
        runtime.block_on(observe(&observed, pairing()))
    });
    assert_eq!(calls, 0);
    assert_eq!(result, json!({"accepted":true,"complete":true}));
    assert!(
        !observed.directory.path().join("changed").exists(),
        "a matching receipt and healthy job skip repeated setup"
    );
}

#[test]
fn observing_a_matching_receipt_repairs_a_missing_helper_job_without_authorization() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::observing_a_matching_receipt_repairs_a_missing_helper_job_without_authorization",
    ) {
        return;
    }
    observe_repair("absent", false, None, false);
    observe_repair("absent", true, Some(42), false);
}

#[test]
fn observing_a_matching_receipt_repairs_a_stopped_helper_job_without_authorization() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::observing_a_matching_receipt_repairs_a_stopped_helper_job_without_authorization",
    ) {
        return;
    }
    observe_repair("regular", true, None, false);
    observe_repair("regular", false, None, false);
}

#[test]
fn observing_a_matching_receipt_retries_when_the_helper_cannot_report_its_job() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::observing_a_matching_receipt_retries_when_the_helper_cannot_report_its_job",
    ) {
        return;
    }
    observe_repair("absent", false, None, true);
}

fn serve_repair_worker(dir: &Path, output: fs::File) {
    let controller = Controller::at(7001, &dir.join("requester/state"));
    let run = |request: atlas_daemon::user::setup::Request| {
        assert!(controller.acquire(true).is_err());
        assert_eq!(controller.pending().unwrap().len(), 2);
        if matches!(request, atlas_daemon::user::setup::Request::Prepare { .. }) {
            atlas_client::test_support::executor_ready(dir, || {})?;
        }
        let path = dir.join("job.json");
        let mut job: Value = serde_json::from_slice(&fs::read(&path)?)?;
        match request {
            atlas_daemon::user::setup::Request::Inspect { .. } => {
                return Ok(json!({"ok":true,"jobs":{atlas_daemon::user::setup::LABEL:job}}));
            }
            atlas_daemon::user::setup::Request::Install { .. } => {
                job["plistKind"] = json!("regular");
                job["loaded"] = json!(true);
                job["pid"] = Value::Null;
            }
            atlas_daemon::user::setup::Request::Kickstart { .. } => job["pid"] = json!(42),
            atlas_daemon::user::setup::Request::Prepare { .. }
            | atlas_daemon::user::setup::Request::ForgetOpener { .. } => {}
            _ => panic!("Unexpected repair request"),
        }
        fs::write(dir.join("changed"), serde_json::to_vec(&request)?)?;
        fs::write(path, serde_json::to_vec(&job)?)?;
        Ok(json!({"ok":true,"pid":job["pid"]}))
    };
    let (result, calls) = atlas_native::approval::without_authorization(|| {
        serve_channel(
            &mut channel(BufReader::new(std::io::stdin()), output),
            &pairing().ticket,
            || Ok(()),
            run,
        )
    });
    fs::write(dir.join("authorization-calls"), calls.to_string()).unwrap();
    result.unwrap();
}

#[test]
fn acceptance_refuses_a_stale_ticket_before_dispatching_setup() {
    if !atlas_core::test_support::isolated(
        "user::onboarding::acceptance::tests::acceptance_refuses_a_stale_ticket_before_dispatching_setup",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let controller = Controller::at(7001, dir.path());
    let mut output = Vec::new();
    let hello = serde_json::to_vec(&json!({"ok":true,"ticket":"old"})).unwrap();
    let mut bytes = hello;
    bytes.push(b'\n');
    assert!(
        drive(
            &controller,
            nix::unistd::getpid(),
            channel(Cursor::new(bytes), &mut output),
            &pairing(),
            || Ok(()),
            agent(dir.path()),
            dir.path()
        )
        .is_err()
    );
    assert!(output.is_empty());
    assert!(controller.pending().unwrap().is_empty());
}

#[test]
fn an_acceptance_test_requester() {
    let Some(dir) = std::env::var_os("ATLAS_ACCEPTANCE_TEST_DIRECTORY").map(PathBuf::from) else {
        return;
    };
    let pid = std::env::var("ATLAS_ACCEPTANCE_TEST_PID")
        .unwrap()
        .parse()
        .unwrap();
    // Let drive close the protocol while libtest keeps its own output.
    let output = fs::File::from(nix::unistd::dup(std::io::stdout()).unwrap());
    nix::unistd::dup2_stdout(std::io::stderr()).unwrap();
    let controller = Controller::at(7001, &dir.join("requester/state"));
    let result = drive(
        &controller,
        nix::unistd::Pid::from_raw(pid),
        channel(BufReader::new(std::io::stdin()), output),
        &pairing(),
        || {
            fs::write(
                dir.join("ownership-checked"),
                controller.acquire(true).is_err().to_string(),
            )?;
            Ok(())
        },
        agent(&dir),
        &dir,
    );
    fs::write(dir.join("result"), result.is_ok().to_string()).unwrap();
}

#[test]
fn an_acceptance_test_worker() {
    let Some(dir) = std::env::var_os("ATLAS_ACCEPTANCE_TEST_DIRECTORY").map(PathBuf::from) else {
        return;
    };
    let stage = std::env::var("ATLAS_ACCEPTANCE_TEST_STAGE").unwrap();
    // Close the protocol before libtest reports completion on stderr.
    let output = fs::File::from(nix::unistd::dup(std::io::stdout()).unwrap());
    nix::unistd::dup2_stdout(std::io::stderr()).unwrap();
    if stage == "repair" {
        serve_repair_worker(&dir, output);
        return;
    }
    let run = |request: atlas_daemon::user::setup::Request| {
        let action = match request {
            atlas_daemon::user::setup::Request::Prepare { .. } => "prepare",
            atlas_daemon::user::setup::Request::Retire { .. } => "retire",
            atlas_daemon::user::setup::Request::Install { .. } => "install",
            atlas_daemon::user::setup::Request::Restore { .. } => "restore",
            atlas_daemon::user::setup::Request::ForgetOpener { .. } => "opener",
            atlas_daemon::user::setup::Request::Kickstart { .. } => "kickstart",
            _ => "inspect",
        };
        if action == "inspect" {
            return Ok(
                json!({"ok":true,"jobs":{atlas_daemon::user::setup::LABEL:{"plistSha256":"b".repeat(64)}}}),
            );
        }
        fs::write(dir.join("changed"), action).unwrap();
        if action == stage {
            atlas_client::test_support::executor_ready(&dir, || {})?;
        }
        if action == "install" && stage == "restore" {
            return Err(problem("Installation refused"));
        }
        Ok(json!({"ok":true,"pid":null}))
    };
    let _ = serve_channel(
        &mut channel(BufReader::new(std::io::stdin()), output),
        &pairing().ticket,
        || Ok(()),
        run,
    );
}
