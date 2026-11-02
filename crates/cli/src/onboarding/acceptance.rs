//! Allow records consent; the requesting Atlas owns and drives setup.
use super::{MAX_RECORD, Pairing, problem, request};
use atlas_client::setup::{self, executor};
use atlas_control::{Controller, ExecutorClaims};
use atlas_core::{account, app_bundle::signed_executable, error::Result};
use atlas_daemon::{
    helper_setup,
    helper_setup::{JobReport, Request},
};
use atlas_native::native::endpoint;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
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
    let helper = account::account_with_uid(pairing.helper_uid)?
        .ok_or_else(|| problem("The chosen helper no longer exists"))?;
    if helper_setup::identity::current(&helper).as_ref() != Some(&pairing.helper_identity)
        || account::administrator(&helper.name) != Some(false)
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
    let receiver = executor::forward(output);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    let ready = loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
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
        )
        .uncertain());
    }
    Ok(json!({"ok":true,"accepted":true}))
}

/// A helper-owned, nonprivileged setup worker. Its only persistent pre-handoff
/// change is a repeatable consent record; it never opens the person's files.
///
/// # Errors
///
/// Rejects an invalid, expired or mismatched ticket and returns account, consent or worker
/// failures. Unknown privileged work retains its recovery claim.
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
    atlas_core::files::write_atomic(
        &path.with_file_name("accepted.json"),
        &serde_json::to_vec(&pairing)?,
        0o644,
    )?;
    println!("ready");
    std::io::stdout().flush()?;
    // Idle listeners retire when the request changes, so a later choice can
    // reuse the endpoint without waiting for an earlier Allow to time out.
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        if super::read_request(Path::new(super::REQUEST)).as_ref() != Some(&pairing) {
            return Err(problem("The helper request changed before setup"));
        }
        if Instant::now() >= deadline {
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
        let guard = helper_setup::Guard {
            account: Some(pairing.helper_identity.clone()),
            logged_out: false,
        };
        // Synchronous work finishes before the worker exits, including after a
        // lost channel. The requester binds this PID before its first command.
        return serve_channel(
            &mut stream,
            &pairing.ticket,
            || verify(&pairing),
            |action| helper_setup::run_guarded(action, &guard),
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
            || atlas_core::acl::allows_write(path.parent()?).ok()?
        {
            return None;
        }
        let file = atlas_core::files::open_plain(path)?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file()
            || metadata.uid() != pairing.helper_uid
            || metadata.mode() & 0o022 != 0
        {
            return None;
        }
        serde_json::from_slice(&atlas_core::files::read_limited(file, MAX_RECORD)?).ok()
    };
    read().as_ref() == Some(pairing)
}

fn receive(channel: &mut impl Read) -> Result<Value> {
    let mut bytes = Vec::new();
    for _ in 0..atlas_core::protocol::MAX_REQUEST {
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
    run: impl Fn(Request) -> Result<Value>,
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
        if !action.allowed_in_acceptance() {
            return Err(problem("First-login setup is not an acceptance operation"));
        }
        let outcome = run(action)
            .unwrap_or_else(|error| json!({"ok":false,"error":error.error,"uncertain":true}));
        send(channel, &outcome)?;
    }
}

/// The person's status loop observes acceptance and finishes setup through the
/// worker. Only this account opens Controller::new and its durable claims.
pub(super) async fn resume(config: &atlas_core::config::Config, pairing: Pairing) -> Result<()> {
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
    let helper = account::account_with_uid(config.helper_uid)?
        .ok_or_else(|| problem("The helper no longer exists"))?;
    let layout = helper_setup::Layout::new(helper.uid, helper.home);
    let mut agent = super::expected_agent(&layout, pairing.controller_uid)?;
    agent.program = signed_executable()?;
    let client = atlas_client::client::Client::new(config.clone())?;
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
    .map_err(|error| {
        problem("The setup connection ended")
            .uncertain()
            .with_source(error)
    })?
}

fn drive(
    controller: &Controller,
    worker: nix::unistd::Pid,
    mut channel: impl Read + Write,
    pairing: &Pairing,
    verify: impl FnOnce() -> Result<()>,
    agent: helper_setup::Agent,
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
    atlas_control::record_executor(worker)?;
    let channel = RefCell::new(channel);
    let outcome = (|| {
        let run = |action: &Request| -> Result<Value> {
            let mut stream = channel.borrow_mut();
            send(&mut *stream, &json!({"ticket":ticket,"request":action}))?;
            helper_setup::acknowledged(receive(&mut *stream)?).map_err(|unfinished| {
                problem(unfinished.reason().unwrap_or("Helper setup failed")).uncertain()
            })
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
    executor::wait_until_ended(worker);
    if outcome.is_ok() {
        controller.clear_if(&pending)?;
    }
    outcome
}

fn complete_setup(
    controller_name: &str,
    agent: helper_setup::Agent,
    logs: &Path,
    run: impl Fn(&Request) -> Result<Value>,
) -> Result<()> {
    let uid = agent.uid;
    run(&Request::Prepare {
        uid,
        controller: controller_name.to_owned(),
    })?;
    let installed = setup::job::install_job(&run, agent, logs)?;
    run(&Request::ForgetOpener { uid })?;
    if installed["pid"].is_null() {
        run(&Request::Kickstart { uid })?;
    }
    Ok(())
}

fn receipt(uid: u32) -> Result<PathBuf> {
    Ok(atlas_core::config::home()?
        .join("Library/Application Support/Atlas")
        .join(format!("helper-{uid}.acceptance.json")))
}

/// Where setup stands for `pairing` in the person's status loop.
pub(super) async fn observe(config: &atlas_core::config::Config, pairing: Pairing) -> Value {
    reconcile(
        accepted(&pairing),
        receipt(pairing.helper_uid),
        pairing,
        async || {
            let client = atlas_client::client::Client::new(config.clone())?;
            setup::migrate::managed_job(&client).await
        },
        async |pairing| resume(config, pairing).await,
        || Controller::new(config.helper_uid)?.pending(),
    )
    .await
}

/// Finish an `accepted` handoff unless `receipt` and today's `job` show it
/// done already, repairing through `resume`.
async fn reconcile(
    accepted: bool,
    receipt: Result<PathBuf>,
    pairing: Pairing,
    job: impl AsyncFnOnce() -> Result<JobReport>,
    resume: impl AsyncFnOnce(Pairing) -> Result<()>,
    pending: impl FnOnce() -> Result<Vec<String>>,
) -> Value {
    if !accepted {
        return json!({"accepted":false});
    }
    let result = async {
        let receipt = receipt?;
        let expected = serde_json::to_vec(&pairing)?;
        // A receipt acknowledges an earlier handoff, not today's job. A
        // silent daemon cannot confirm it either; retry through the same
        // helper-owned listener and requester-owned transaction in that case.
        if fs::read(&receipt).ok().as_ref() == Some(&expected)
            && job().await.is_ok_and(|job| job.running())
        {
            return Ok(());
        }
        resume(pairing).await?;
        atlas_core::files::save_private(&receipt, &expected)
    }
    .await;
    match result {
        Ok(()) => json!({"accepted":true,"complete":true}),
        Err(error) => json!({"accepted":true,"complete":false,"error":error.error,
            "pending": pending().ok(),
            "recovery":"Open Atlas in the helper to retry; return here to finish or recover pending setup."}),
    }
}

#[cfg(test)]
mod tests;
