//! Allow records consent; the requesting Atlas owns and drives setup.
use super::{Pairing, as_root, problem, request, signed_executable};
use crate::{
    control::{Controller, ExecutorClaims},
    error::Result,
    native::endpoint,
    setup::{self, helper::Request},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

pub(super) fn listening(uid: u32) -> bool {
    std::os::unix::net::UnixStream::connect(socket(uid)).is_ok()
}

fn socket(uid: u32) -> PathBuf {
    PathBuf::from(format!("/Users/Shared/Atlas-accept-{uid}/setup.sock"))
}

fn verify(pairing: &Pairing) -> Result<()> {
    if request().as_ref() != Some(pairing) {
        return Err(problem(
            "The helper request changed; choose the helper again",
        ));
    }
    let helper = setup::account_with_uid(pairing.helper_uid)?
        .ok_or_else(|| problem("The chosen helper no longer exists"))?;
    if setup::identity::current(&helper).as_ref() != Some(&pairing.helper_identity)
        || setup::administrator(&helper.name) != Some(false)
    {
        return Err(problem("The chosen helper account changed before setup"));
    }
    Ok(())
}

/// Check the Allow entry point against fresh account/request observations.
/// The startup seam runs the same unprivileged process protocol in tests.
pub(super) fn allow(
    shown: (u32, &str),
    here: (u32, &str),
    configured: bool,
    administrator: Option<bool>,
    pairing: Option<Pairing>,
    start: impl FnOnce(&Pairing) -> Result<Value>,
) -> Result<Value> {
    if configured {
        return Err(problem(
            "This account has Atlas settings of its own, so it cannot be a helper",
        ));
    }
    let pairing = pairing
        .filter(|p| p.helper_uid == here.0 && p.helper_name == here.1)
        .ok_or_else(|| problem("No account has chosen this one as its helper"))?;
    if pairing.controller_uid != shown.0 || pairing.controller_name != shown.1 {
        return Err(problem(
            "The account asking changed since this window showed it; open Atlas again",
        ));
    }
    if administrator != Some(false) {
        return Err(problem(
            "This account administers the Mac; a helper must be a standard account",
        ));
    }
    start(&pairing)
}

/// Only start a helper-owned listener. No authorization, layout preparation,
/// job changes or rollback runs on the Allow side of the handoff.
pub(super) fn signal(pairing: &Pairing) -> Result<Value> {
    if accepted(pairing) && listening(pairing.helper_uid) {
        return Ok(json!({"ok":true,"accepted":true}));
    }
    let mut command = Command::new(signed_executable()?);
    command.args(["onboarding", "accept-worker", "--ticket", &pairing.ticket]);
    signal_using(&mut command)
}

fn signal_using(command: &mut Command) -> Result<Value> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| problem("No setup pipe"))?;
    let receiver = crate::init::forward(output);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    let ready = loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(chunk) => bytes.extend(chunk),
            Err(_) => break false,
        }
        if bytes.contains(&b'\n') || bytes.len() > 64 {
            break bytes == b"ready\n";
        }
    };
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        return Err(problem(
            "Allow could not be confirmed. Open Atlas here to retry; any recorded acceptance is kept.",
        ).uncertain());
    }
    Ok(json!({"ok":true,"accepted":true}))
}

/// A helper-owned, nonprivileged setup worker. Its only persistent pre-handoff
/// change is a repeatable consent record; it never opens the person's files.
pub async fn accept_worker(ticket: &str) -> Result<Value> {
    signed_executable()?;
    let pairing = request()
        .filter(|p| p.ticket == ticket)
        .ok_or_else(|| problem("The helper request changed before Allow"))?;
    let uid = nix::unistd::getuid().as_raw();
    if uid != pairing.helper_uid || nix::unistd::geteuid().as_raw() != uid {
        return Err(problem("Allow must run as the chosen helper"));
    }
    verify(&pairing)?;
    let path = socket(uid);
    let (listener, _endpoint) = endpoint::bind(
        &path,
        uid,
        "setup acceptance",
        endpoint::Directory::Shared(0o755),
    )
    .await?;
    verify(&pairing)?;
    crate::config::write_atomic(
        &path.with_file_name("accepted.json"),
        &serde_json::to_vec(&pairing)?,
        0o644,
    )?;
    println!("ready");
    std::io::stdout().flush()?;
    // Idle listeners retire when the request changes, so a later choice can
    // reuse the endpoint without waiting for an earlier Allow to time out.
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    loop {
        if super::read_request(Path::new(super::REQUEST)).as_ref() != Some(&pairing) {
            return Err(problem("The helper request changed before setup"));
        }
        if std::time::Instant::now() >= deadline {
            return Err(problem("Open Atlas in the helper to resume acceptance"));
        }
        let (stream, _) =
            match tokio::time::timeout(Duration::from_secs(1), listener.accept()).await {
                Ok(connection) => connection?,
                Err(_) => continue,
            };
        if stream.peer_cred()?.uid() != pairing.controller_uid {
            continue;
        }
        let mut stream = stream.into_std()?;
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let guard = setup::helper::Guard {
            account: Some(pairing.helper_identity.clone()),
            logged_out: false,
        };
        // Synchronous work finishes before the worker exits, including after a
        // lost channel. The requester binds this PID before its first command.
        return serve_channel(
            &mut stream,
            &pairing.ticket,
            || verify(&pairing),
            |action| setup::helper::run_guarded(&serde_json::to_string(action)?, &guard),
        )
        .map(|()| json!({"ok":true}));
    }
}

/// A stale consent record is visible but never authorizes a different request.
pub(super) fn accepted(pairing: &Pairing) -> bool {
    let path = socket(pairing.helper_uid).with_file_name("accepted.json");
    accepted_at(&path, pairing)
}

fn accepted_at(path: &Path, pairing: &Pairing) -> bool {
    use std::os::unix::fs::MetadataExt;
    let read = || -> Option<Pairing> {
        let directory = fs::symlink_metadata(path.parent()?).ok()?;
        if !directory.is_dir()
            || directory.uid() != pairing.helper_uid
            || directory.mode() & 0o022 != 0
            || crate::acl::allows_write(path.parent()?).ok()?
        {
            return None;
        }
        let mut file = crate::files::open_plain(path)?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != pairing.helper_uid
            || metadata.mode() & 0o022 != 0
        {
            return None;
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 64 * 1024 {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    };
    read().as_ref() == Some(pairing)
}

fn receive(channel: &mut impl Read) -> Result<Value> {
    let mut bytes = Vec::new();
    for _ in 0..crate::daemon::protocol::MAX_REQUEST {
        let mut byte = [0];
        channel.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return Ok(serde_json::from_slice(&bytes)?);
        }
        bytes.push(byte[0]);
    }
    Err(problem("Setup reply is too large"))
}

fn send(channel: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *channel, value)?;
    channel.write_all(b"\n")?;
    channel.flush()?;
    Ok(())
}

/// Reuse the typed setup worker protocol, with a bounded, authenticated channel.
fn serve_channel(
    channel: &mut (impl Read + Write),
    ticket: &str,
    verify: impl Fn() -> Result<()>,
    run: impl Fn(&Request) -> Result<Value>,
) -> Result<()> {
    send(channel, &json!({"ok":true,"ticket":ticket}))?;
    loop {
        let value = receive(channel)?;
        if value["ticket"] != ticket {
            return Err(problem("The helper request changed"));
        }
        verify()?;
        if value["complete"] == true {
            send(channel, &json!({"ok":true}))?;
            return Ok(());
        }
        let action: Request = serde_json::from_value(value["request"].clone())?;
        if matches!(action, Request::FirstLogin { .. }) {
            return Err(problem("First-login setup is not an acceptance operation"));
        }
        let outcome = run(&action)
            .unwrap_or_else(|error| json!({"ok":false,"error":error.error,"uncertain":true}));
        send(channel, &outcome)?;
    }
}

/// The person's status loop observes acceptance and finishes setup through the
/// worker. Only this account opens Controller::new and its durable claims.
pub(super) async fn resume(config: &crate::config::Config, pairing: Pairing) -> Result<()> {
    if !accepted(&pairing) {
        return Err(problem("The recorded acceptance changed before setup"));
    }
    let stream = tokio::net::UnixStream::connect(socket(pairing.helper_uid)).await?;
    let credentials = stream.peer_cred()?;
    if credentials.uid() != pairing.helper_uid {
        return Err(problem("The setup worker is not the helper"));
    }
    let pid = credentials
        .pid()
        .filter(|pid| *pid > 1)
        .ok_or_else(|| problem("The setup worker process is unknown"))?;
    let stream = stream.into_std()?;
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let controller = Controller::new(config.helper_uid)?;
    let helper = setup::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The helper no longer exists"))?;
    let layout = setup::helper::Layout::new(helper.uid, helper.home);
    let mut agent = super::expected_agent(&layout, pairing.controller_uid)?;
    agent.program = signed_executable()?;
    let client = crate::client::Client::new(config.clone())?;
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        drive(
            &controller,
            nix::unistd::Pid::from_raw(pid),
            stream,
            &pairing,
            || {
                runtime.block_on(client.require_service_idle())?;
                verify(&pairing)
            },
            agent,
            &layout.logs(),
        )
    })
    .await
    .map_err(|_| problem("The setup connection ended").uncertain())?
}

fn drive(
    controller: &Controller,
    executor: nix::unistd::Pid,
    mut channel: impl Read + Write,
    pairing: &Pairing,
    verify: impl FnOnce() -> Result<()>,
    agent: setup::helper::Agent,
    logs: &Path,
) -> Result<()> {
    let ticket = pairing.ticket.as_str();
    let _ownership = controller.acquire(true)?;
    verify()?;
    let hello = receive(&mut channel)?;
    if hello["ok"] != true || hello["ticket"] != ticket {
        return Err(problem("The acceptance is for an earlier request"));
    }
    let pending = controller.quarantine()?;
    let _executors = ExecutorClaims::new(std::slice::from_ref(&pending))?;
    crate::control::record_executor(executor)?;
    let channel = RefCell::new(channel);
    let outcome = (|| {
        let run = |action: &Request| -> Result<Value> {
            let mut stream = channel.borrow_mut();
            send(&mut *stream, &json!({"ticket":ticket,"request":action}))?;
            let reply = receive(&mut *stream)?;
            if reply["ok"] != true {
                return Err(
                    problem(reply["error"].as_str().unwrap_or("Helper setup failed")).uncertain(),
                );
            }
            Ok(reply)
        };
        complete_setup(&pairing.controller_name, agent, logs, run)?;
        let mut stream = channel.borrow_mut();
        send(&mut *stream, &json!({"ticket":ticket,"complete":true}))?;
        if receive(&mut *stream)?["ok"] != true {
            return Err(problem("Setup acknowledgement was lost").uncertain());
        }
        Ok(())
    })();
    // Closing the channel stops future commands. Retain ownership while any
    // dispatched filesystem/launchd operation finishes after a lost reply.
    drop(channel);
    as_root::wait_until_ended(executor);
    if outcome.is_ok() {
        controller.clear_if(&pending)?;
    }
    outcome
}

fn complete_setup(
    controller_name: &str,
    agent: setup::helper::Agent,
    logs: &Path,
    run: impl Fn(&Request) -> Result<Value>,
) -> Result<Value> {
    let uid = agent.uid;
    run(&Request::Prepare {
        uid,
        controller: controller_name.to_owned(),
    })?;
    let installed = crate::init::install_job(&run, uid, agent, logs)?;
    run(&Request::ForgetOpener { uid })?;
    if installed["pid"].is_null() {
        run(&Request::Kickstart { uid })?;
    }
    Ok(installed)
}

fn receipt(uid: u32) -> Result<PathBuf> {
    Ok(crate::config::home()?
        .join("Library/Application Support/Atlas")
        .join(format!("helper-{uid}.acceptance.json")))
}

/// Account observations and the existing handoff, substituted in acceptance tests.
pub(super) trait Observation {
    fn accepted(&self, pairing: &Pairing) -> bool;
    fn receipt(&self, uid: u32) -> Result<PathBuf>;
    async fn job(&self) -> Result<Value>;
    async fn resume(&self, pairing: Pairing) -> Result<()>;
    fn pending(&self) -> Result<Vec<String>>;
}

impl Observation for crate::config::Config {
    fn accepted(&self, pairing: &Pairing) -> bool {
        accepted(pairing)
    }

    fn receipt(&self, uid: u32) -> Result<PathBuf> {
        receipt(uid)
    }

    async fn job(&self) -> Result<Value> {
        let client = crate::client::Client::new(self.clone())?;
        setup::migrate::managed_job(&client).await
    }

    async fn resume(&self, pairing: Pairing) -> Result<()> {
        resume(self, pairing).await
    }

    fn pending(&self) -> Result<Vec<String>> {
        Controller::new(self.helper_uid)?.pending()
    }
}

pub(super) async fn observe(config: &impl Observation, pairing: Pairing) -> Value {
    let accepted = config.accepted(&pairing);
    if !accepted {
        return json!({"accepted":false});
    }
    let result = async {
        let receipt = config.receipt(pairing.helper_uid)?;
        let expected = serde_json::to_vec(&pairing)?;
        // A receipt acknowledges an earlier handoff, not today's job. A
        // silent daemon cannot confirm it either; retry through the same
        // helper-owned listener and requester-owned transaction in that case.
        if fs::read(&receipt).ok().as_ref() == Some(&expected)
            && config.job().await.is_ok_and(|job| {
                job["plistKind"] == "regular"
                    && job["loaded"] == true
                    && job["pid"].as_u64().is_some_and(|pid| pid > 1)
                    && job["path"].as_str().is_some()
                    && job["path"] == job["plist"]
            })
        {
            return Ok(());
        }
        config.resume(pairing).await?;
        crate::config::save_private(&receipt, &expected)
    }
    .await;
    match result {
        Ok(()) => json!({"accepted":true,"complete":true}),
        Err(error) => json!({"accepted":true,"complete":false,"error":error.error,
            "pending": config.pending().ok(),
            "recovery":"Open Atlas in the helper to retry; return here to finish or recover pending setup."}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Cursor},
        process::Child,
    };

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
    fn pairing() -> Pairing {
        Pairing {
            ticket: "a".repeat(crate::files::TOKEN_BYTES * 2),
            controller_uid: nix::unistd::getuid().as_raw(),
            controller_name: "requester".into(),
            helper_uid: 7001,
            helper_name: "helper".into(),
            helper_identity: setup::identity::Identity {
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
    fn agent(dir: &Path) -> setup::helper::Agent {
        setup::helper::Agent {
            uid: 7001,
            controller_uid: pairing().controller_uid,
            label: setup::helper::LABEL.into(),
            program: dir.join("Atlas"),
            socket: dir.join("helper/c.sock"),
        }
    }
    fn worker(dir: &Path, stage: &str) -> Child {
        crate::test_support::rerun("onboarding::acceptance::tests::an_acceptance_test_worker")
            .arg("--nocapture")
            .env("ATLAS_ACCEPTANCE_TEST_DIRECTORY", dir)
            .env("ATLAS_ACCEPTANCE_TEST_STAGE", stage)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
    fn paused(dir: &Path) -> bool {
        for _ in 0..500 {
            if dir.join("paused").exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn interrupted_acceptance_keeps_the_requesters_claim_until_its_worker_ends() {
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::interrupted_acceptance_keeps_the_requesters_claim_until_its_worker_ends",
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
            let mut requester = crate::test_support::rerun(
                "onboarding::acceptance::tests::an_acceptance_test_requester",
            )
            .arg("--nocapture")
            .env("ATLAS_ACCEPTANCE_TEST_DIRECTORY", dir.path())
            .env("ATLAS_ACCEPTANCE_TEST_PID", worker.id().to_string())
            .stdin(Stdio::from(worker.stdout.take().unwrap()))
            .stdout(Stdio::from(worker.stdin.take().unwrap()))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
            let reached = paused(dir.path());
            let before = controller.pending().unwrap();
            let _ = requester.kill();
            let _ = requester.wait();
            let excluded = controller.acquire(true).is_err();
            let after = controller.pending().unwrap();
            fs::write(dir.path().join("continue"), b"").unwrap();
            let _ = worker.wait();
            assert!(reached, "{stage}");
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
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::acceptance_excludes_competing_teardown_through_completion_and_rollback",
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
            let pid = nix::unistd::Pid::from_raw(worker.id().try_into().unwrap());
            let stream = channel(
                BufReader::new(worker.stdout.take().unwrap()),
                worker.stdin.take().unwrap(),
            );
            let owned = controller.clone();
            let root = dir.path().to_owned();
            let requester = std::thread::spawn(move || {
                drive(
                    &owned,
                    pid,
                    stream,
                    &pairing(),
                    || {
                        fs::write(
                            root.join("ownership-checked"),
                            owned.acquire(true).is_err().to_string(),
                        )?;
                        Ok(())
                    },
                    agent(&root),
                    &root,
                )
            });
            let reached = paused(dir.path());
            let excluded = controller.acquire(true).is_err();
            let pending = controller.pending().unwrap();
            fs::write(dir.path().join("continue"), b"").unwrap();
            let _ = worker.wait();
            let result = requester.join().unwrap();
            assert!(reached, "{stage}");
            assert!(excluded, "teardown must not enter during {stage}");
            assert_eq!(
                fs::read_to_string(dir.path().join("ownership-checked")).unwrap(),
                "true",
                "ownership precedes the claim and every dispatch: {stage}"
            );
            assert_eq!(pending.len(), 2, "{stage}");
            assert_eq!(result.is_ok(), stage != "restore", "{stage}");
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
        let (result, calls) = crate::approval::without_authorization(|| {
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

    impl Observation for ObservedHelper {
        fn accepted(&self, accepted: &Pairing) -> bool {
            accepted == &pairing()
        }

        fn receipt(&self, _: u32) -> Result<PathBuf> {
            Ok(self.directory.path().join("receipt.json"))
        }

        async fn job(&self) -> Result<Value> {
            if self.inspection_unavailable.replace(false) {
                return Err(problem("The helper is not answering"));
            }
            Ok(serde_json::from_slice(&fs::read(
                self.directory.path().join("job.json"),
            )?)?)
        }

        async fn resume(&self, pairing: Pairing) -> Result<()> {
            let root = self.directory.path();
            let mut worker = worker(root, "repair");
            let pid = nix::unistd::Pid::from_raw(worker.id().try_into().unwrap());
            let mut stream = channel(
                BufReader::new(worker.stdout.take().unwrap()),
                worker.stdin.take().unwrap(),
            );
            let worker = std::thread::spawn(move || worker.wait().unwrap());
            let result = drive(
                &self.controller,
                pid,
                &mut stream,
                &pairing,
                || {
                    assert!(self.controller.acquire(true).is_err());
                    Ok(())
                },
                agent(root),
                root,
            );
            let status = worker.join().unwrap();
            assert!(status.success(), "worker: {status}; handoff: {result:?}");
            result
        }

        fn pending(&self) -> Result<Vec<String>> {
            self.controller.pending()
        }
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
        crate::config::save_private(&receipt, &expected).unwrap();
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
        let (result, calls) = crate::approval::without_authorization(|| {
            runtime.block_on(observe(&observed, pairing()))
        });
        assert_eq!(calls, 0, "repair must reuse consent without authorization");
        assert_eq!(result, json!({"accepted":true,"complete":true}));
        let repaired = runtime.block_on(observed.job()).unwrap();
        assert_eq!(
            repaired["plistKind"], "regular",
            "repair restores the plist"
        );
        assert_eq!(repaired["loaded"], true, "repair loads the helper job");
        assert_eq!(repaired["pid"], 42, "repair starts the helper job");
        assert_eq!(
            fs::read_to_string(observed.directory.path().join("authorization-calls")).unwrap(),
            "0",
            "the unprivileged worker must not request authorization either"
        );
        assert_eq!(observed.pending().unwrap(), vec![generation]);
        assert_eq!(fs::read(&receipt).unwrap(), expected);
        assert!(observed.controller.acquire(true).is_ok());

        fs::remove_file(observed.directory.path().join("changed")).unwrap();
        let (result, calls) = crate::approval::without_authorization(|| {
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
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::observing_a_matching_receipt_repairs_a_missing_helper_job_without_authorization",
        ) {
            return;
        }
        observe_repair("absent", false, None, false);
        observe_repair("absent", true, Some(42), false);
    }

    #[test]
    fn observing_a_matching_receipt_repairs_a_stopped_helper_job_without_authorization() {
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::observing_a_matching_receipt_repairs_a_stopped_helper_job_without_authorization",
        ) {
            return;
        }
        observe_repair("regular", true, None, false);
        observe_repair("regular", false, None, false);
    }

    #[test]
    fn observing_a_matching_receipt_retries_when_the_helper_cannot_report_its_job() {
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::observing_a_matching_receipt_retries_when_the_helper_cannot_report_its_job",
        ) {
            return;
        }
        observe_repair("absent", false, None, true);
    }

    fn serve_repair_worker(dir: &Path) {
        let controller = Controller::at(7001, &dir.join("requester/state"));
        let run = |request: &Request| {
            assert!(controller.acquire(true).is_err());
            assert_eq!(controller.pending().unwrap().len(), 2);
            let path = dir.join("job.json");
            let mut job: Value = serde_json::from_slice(&fs::read(&path)?)?;
            match request {
                Request::Inspect { .. } => {
                    return Ok(json!({"ok":true,"jobs":{setup::helper::LABEL:job}}));
                }
                Request::Install { .. } => {
                    job["plistKind"] = json!("regular");
                    job["loaded"] = json!(true);
                    job["pid"] = Value::Null;
                }
                Request::Kickstart { .. } => job["pid"] = json!(42),
                Request::Prepare { .. } | Request::ForgetOpener { .. } => {}
                _ => panic!("Unexpected repair request"),
            }
            fs::write(dir.join("changed"), serde_json::to_vec(request)?)?;
            fs::write(path, serde_json::to_vec(&job)?)?;
            Ok(json!({"ok":true,"pid":job["pid"]}))
        };
        let (result, calls) = crate::approval::without_authorization(|| {
            serve_channel(
                &mut channel(BufReader::new(std::io::stdin()), std::io::stdout()),
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
        if !crate::test_support::isolated(
            "onboarding::acceptance::tests::acceptance_refuses_a_stale_ticket_before_dispatching_setup",
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
        let Some(dir) = std::env::var_os("ATLAS_ACCEPTANCE_TEST_DIRECTORY").map(PathBuf::from)
        else {
            return;
        };
        let pid = std::env::var("ATLAS_ACCEPTANCE_TEST_PID")
            .unwrap()
            .parse()
            .unwrap();
        let _ = drive(
            &Controller::at(7001, &dir.join("requester/state")),
            nix::unistd::Pid::from_raw(pid),
            channel(BufReader::new(std::io::stdin()), std::io::stdout()),
            &pairing(),
            || Ok(()),
            agent(&dir),
            &dir,
        );
    }

    #[test]
    fn an_acceptance_test_worker() {
        let Some(dir) = std::env::var_os("ATLAS_ACCEPTANCE_TEST_DIRECTORY").map(PathBuf::from)
        else {
            return;
        };
        let stage = std::env::var("ATLAS_ACCEPTANCE_TEST_STAGE").unwrap();
        if stage == "repair" {
            serve_repair_worker(&dir);
            return;
        }
        let run = |request: &Request| {
            let action = match request {
                Request::Prepare { .. } => "prepare",
                Request::Retire { .. } => "retire",
                Request::Install { .. } => "install",
                Request::Restore { .. } => "restore",
                Request::ForgetOpener { .. } => "opener",
                Request::Kickstart { .. } => "kickstart",
                _ => "inspect",
            };
            if action == "inspect" {
                return Ok(
                    json!({"ok":true,"jobs":{setup::helper::LABEL:{"plistSha256":"b".repeat(64)}}}),
                );
            }
            fs::write(dir.join("changed"), action).unwrap();
            if action == stage {
                fs::write(dir.join("paused"), action).unwrap();
                while !dir.join("continue").exists() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if action == "install" && stage == "restore" {
                return Err(problem("Installation refused"));
            }
            Ok(json!({"ok":true,"pid":null}))
        };
        let _ = serve_channel(
            &mut channel(BufReader::new(std::io::stdin()), std::io::stdout()),
            &pairing().ticket,
            || Ok(()),
            run,
        );
    }
}
