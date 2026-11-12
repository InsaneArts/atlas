//! The helper's background job as its controller installs, replaces and
//! restarts it, each action run by a worker inside the helper login.
use super::{account::problem, reported_pid};
use crate::{client::Client, setup};
use atlas_core::{
    account::{Account, account_with_uid},
    app_bundle,
    error::{Error, Result},
};
use atlas_daemon::identity::Identity;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn this_account() -> Result<Account> {
    account_with_uid(nix::unistd::getuid().as_raw())?
        .ok_or_else(|| problem("This account cannot be resolved"))
}

/// The helper's background job as this Atlas writes it, for `helper` and
/// the app's `executable`, with the layout it lives in.
fn helper_agent(
    helper: &Account,
    executable: &Path,
) -> Result<(
    atlas_daemon::user::setup::Agent,
    atlas_daemon::user::setup::Layout,
)> {
    let controller = this_account()?;
    let layout = atlas_daemon::user::setup::Layout::new(helper.uid, helper.home.clone());
    let agent = atlas_daemon::user::setup::Agent::managed(
        &layout,
        executable.to_path_buf(),
        controller.uid,
    );

    Ok((agent, layout))
}

/// Run one setup-helper request as the helper, inside its own login, with
/// the administrator rights init holds.
fn as_helper(
    executable: &Path,
    helper: &Account,
    request: &atlas_daemon::user::setup::Request,
    expected: &Identity,
) -> Result<Value> {
    setup::request_as_helper(
        executable,
        helper,
        request,
        setup::Invocation::CachedLogin,
        expected,
    )?
    .map_err(|unfinished| {
        let error = problem(format!(
            "Setting up the helper failed: {}",
            unfinished.reason().unwrap_or("no reply")
        ));
        unfinished.behind(error)
    })
}

/// Install the helper's background job `agent` through `run`, which runs one
/// setup-helper request as the helper. An earlier Atlas's job (another app,
/// another account, an older version) is set aside first, its definition
/// kept as a backup, and comes back when the new one cannot go in.
///
/// # Errors
///
/// Returns the failure of `run`, or of restoring the earlier job.
pub fn install_job(
    run: impl Fn(&atlas_daemon::user::setup::Request) -> Result<Value>,
    agent: atlas_daemon::user::setup::Agent,
    logs: &Path,
) -> Result<Value> {
    let uid = agent.uid;
    let inspected = run(&atlas_daemon::user::setup::Request::Inspect {
        uid,
        labels: vec![atlas_daemon::user::setup::LABEL.to_owned()],
    })?;
    let earlier = atlas_daemon::user::setup::JobReport::from(
        &inspected["jobs"][atlas_daemon::user::setup::LABEL],
    );
    let expected = agent.plist_sha256(logs);
    // Installation accepts an inspected hash even without plistKind;
    // refresh separately requires a regular definition.
    let retired = earlier.sha256.filter(|sha| *sha != expected);
    if let Some(sha256) = &retired {
        run(&atlas_daemon::user::setup::Request::Retire {
            uid,
            label: atlas_daemon::user::setup::LABEL.to_owned(),
            sha256: sha256.clone(),
            missing_ok: false,
        })?;
    }

    let mut error = match run(&atlas_daemon::user::setup::Request::Install { agent }) {
        Ok(installed) => return Ok(installed),
        Err(error) => error,
    };

    let Some(sha256) = retired else {
        return Err(error);
    };

    // The earlier job comes back when the new one cannot go in. The new
    // definition install may have written goes first, or it would block
    // that return.
    let restored = run(&atlas_daemon::user::setup::Request::Retire {
        uid,
        label: atlas_daemon::user::setup::LABEL.to_owned(),
        sha256: expected,
        missing_ok: true,
    })
    .and_then(|_| {
        run(&atlas_daemon::user::setup::Request::Restore {
            uid,
            label: atlas_daemon::user::setup::LABEL.to_owned(),
            sha256,
        })
    });

    if let Err(lost) = restored {
        error.error = format!(
            "{}; the earlier helper job could not be put back: {}",
            error.error, lost.error
        );
    }

    Err(error)
}

/// [`as_helper`], owning what it needs to run on a worker of its own.
fn helper_runner(
    executable: &'static Path,
    helper: &Account,
    expected: &Identity,
) -> impl Fn(&atlas_daemon::user::setup::Request) -> Result<Value> + Send + 'static {
    let (helper, expected) = (helper.clone(), expected.clone());
    move |request| as_helper(executable, &helper, request, &expected)
}

/// Run `transaction` on a blocking worker of its own, which a caller that
/// stops waiting leaves running to its end. A worker that ends without an
/// answer leaves what it did unknown.
async fn on_worker<T: Send + 'static>(
    transaction: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(transaction)
        .await
        .map_err(|cause| {
            problem("Setting up the helper stopped before it finished")
                .with_source(cause)
                .uncertain()
        })?
}

/// Prepare and install under one claim, including rollback dispatches, on
/// a worker that holds exclusion until all of it is done. The claim is
/// settled, still under exclusion, only once its success is heard here: a
/// caller that stops waiting leaves it pending.
async fn prepare_and_install(
    client: &Client,
    controller_name: &str,
    agent: atlas_daemon::user::setup::Agent,
    logs: &Path,
    run: impl Fn(&atlas_daemon::user::setup::Request) -> Result<Value> + Send + 'static,
) -> Result<Value> {
    let ownership = client.service_change(true).await?;
    let controller = client.controller.clone();
    let (controller_name, logs) = (controller_name.to_owned(), logs.to_path_buf());
    let (_ownership, pending, installed) = on_worker(move || {
        let pending = controller.quarantine()?;
        let installed = {
            // Executor claims bind to the thread that dispatches them.
            let _executors = atlas_control::ExecutorClaims::new(std::slice::from_ref(&pending))?;
            run(&atlas_daemon::user::setup::Request::Prepare {
                uid: agent.uid,
                controller: controller_name,
            })
            .and_then(|_| install_job(run, agent, &logs))
        };
        Ok((ownership, pending, installed))
    })
    .await?;

    let installed = installed?;
    client.controller.clear_if(&pending)?;
    Ok(installed)
}

/// Restart the helper daemon (launchd starts a fresh process) and wait for
/// the new one to answer. No other controller may act meanwhile, and the
/// restart stays pending until a new daemon is seen; if that never happens
/// it is left for recovery, like any action of unknown outcome.
///
/// # Errors
///
/// Returns identity, ownership, restart or readiness failures. A timeout does not
/// acknowledge unfinished helper work.
pub async fn restart_helper(client: &Client) -> Result<()> {
    let _exclusion = client.service_change(true).await?;
    let before = client.status().await["daemon"]["pid"].clone();
    let transport = atlas_core::transport::Transport {
        path: client.config.socket_path.clone(),
        timeout: Duration::from_secs(10),
        peer_uid: client.desktop().user().uid,
    };

    let pending = client.controller.quarantine()?;
    let reply = transport.call(&json!({"op": "restart"})).await?;
    if reply["ok"] != true {
        // A refusal changed nothing, unless the helper says it may have.
        let uncertain = reply["uncertain"] == true;
        if !uncertain {
            client.controller.clear_if(&pending)?;
        }

        let error = problem(format!(
            "The helper did not restart: {}",
            reply["error"].as_str().unwrap_or("no reason given")
        ));

        return Err(if uncertain { error.uncertain() } else { error });
    }

    if !new_daemon_answers(client, &before).await {
        // The restart stays pending: its outcome is unknown.
        return Err(
            problem("The helper did not start again after a restart. Run atlas doctor.")
                .uncertain(),
        );
    }

    client.controller.clear_if(&pending)?;
    Ok(())
}

/// Whether a daemon other than the one whose pid was `before` answers
/// within 30 seconds.
async fn new_daemon_answers(client: &Client, before: &Value) -> bool {
    let deadline = Instant::now() + Duration::from_secs(30);

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let pid = client.status().await["daemon"]["pid"].clone();
        if pid.is_number() && pid != *before {
            return true;
        }

        if Instant::now() >= deadline {
            return false;
        }
    }
}

/// [`new_daemon_answers`] from a blocking worker, on a runtime of its own:
/// the look goes on while the runtime that started the worker ends, which
/// waits for it. A runtime that cannot be made sees no new daemon.
fn new_daemon_answers_on_worker(client: &Client, before: &Value) -> bool {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .is_ok_and(|runtime| runtime.block_on(new_daemon_answers(client, before)))
}

/// Replace the helper's background job an earlier Atlas wrote with this
/// version's, from outside the daemon it runs (through sudo, as installing
/// it does): the earlier definition is kept as a backup, and comes back if
/// the new one cannot go in. Pending until a new daemon answers, like a
/// restart. The replacement and the look for the new daemon run on a worker
/// that holds exclusion and the claim until both are done, even when nobody
/// waits for it any more; the claim is settled, still under exclusion, only
/// once its success is heard here, so a caller that stops waiting leaves it
/// pending.
async fn replace_job(
    client: &Client,
    helper: &Account,
    agent: atlas_daemon::user::setup::Agent,
    layout: &atlas_daemon::user::setup::Layout,
    run: impl Fn(&atlas_daemon::user::setup::Request) -> Result<Value> + Send + 'static,
) -> Result<()> {
    let exclusion = client.service_change(true).await?;
    let before = client.status().await["daemon"]["pid"].clone();
    let (controller, looking) = (client.controller.clone(), client.clone());
    let logs = layout.paths().logs();
    let (_exclusion, pending, answered) = on_worker(move || {
        let pending = controller.quarantine()?;
        {
            // Executor claims bind to the thread that dispatches them.
            let _executors = atlas_control::ExecutorClaims::new(std::slice::from_ref(&pending))?;
            install_job(run, agent, &logs).map_err(Error::uncertain)?;
        }
        let answered = new_daemon_answers_on_worker(&looking, &before);
        Ok((exclusion, pending, answered))
    })
    .await?;

    if !answered {
        return Err(problem(format!(
            "The helper's background job was replaced but the helper did not start again. Its \
             log is {} in {}'s account; run atlas init again after checking it.",
            layout
                .paths()
                .logs()
                .join(format!("{}.log", atlas_daemon::user::setup::LABEL))
                .display(),
            helper.name
        ))
        .uncertain());
    }

    client.controller.clear_if(&pending)?;
    Ok(())
}

/// Put the helper's daemon on this version before checking it: its
/// background job replaced when another version of Atlas wrote it, else
/// the daemon restarted when it still runs an earlier build (an update
/// whose handover did not finish).
///
/// # Errors
///
/// Returns account-identity, installed-build, ownership or helper-refresh failures.
/// Unacknowledged privileged changes retain their claim.
pub async fn refresh_helper(
    client: &Client,
    helper: &Account,
    expected: &Identity,
    mut progress: impl FnMut(setup::Progress<'_>) -> Result<()>,
) -> Result<Option<Value>> {
    let executable = Path::new(app_bundle::EXECUTABLE);
    let (agent, layout) = helper_agent(helper, executable)?;
    let expected_hash = agent.plist_sha256(&layout.paths().logs());
    let job = setup::migrate::managed_job(client).await?;

    if job.outdated(&expected_hash) {
        progress(setup::Progress::Administrator(
            "  macOS asks for your password once, to replace the helper's background job with \
             this version's.",
        ))?;

        progress(setup::Progress::Waiting(
            "Replacing the helper's background job…",
        ))?;
        replace_job(
            client,
            helper,
            agent,
            &layout,
            helper_runner(executable, helper, expected),
        )
        .await?;
        progress(setup::Progress::Done(
            "Helper job",
            "replaced with this version's",
        ))?;
        return Ok(Some(
            json!({"step": "daemon", "label": atlas_daemon::user::setup::LABEL, "replaced": true}),
        ));
    }

    let pid = reported_pid(&client.status().await["daemon"]["pid"]);
    if pid.is_some_and(|pid| !app_bundle::runs_current(pid, executable)) {
        progress(setup::Progress::Waiting(
            "Restarting the helper on this version…",
        ))?;
        client.restart().await?;
        progress(setup::Progress::Done("Helper", "restarted on this version"))?;
        return Ok(Some(json!({"step": "restart"})));
    }

    Ok(None)
}

/// Prepare and install the chosen helper under exclusion and one executor claim.
/// Refusals and missing acknowledgements preserve the pending generation.
///
/// # Errors
///
/// Returns account, identity, installation or helper-layout preparation failures. Work
/// already completed remains available to the recovery workflow.
pub async fn prepare_helper(
    client: &Client,
    helper: &Account,
    expected: &Identity,
) -> Result<PathBuf> {
    let executable = Path::new(app_bundle::EXECUTABLE);
    let controller = this_account()?;
    let (agent, layout) = helper_agent(helper, executable)?;
    prepare_and_install(
        client,
        &controller.name,
        agent,
        &layout.paths().logs(),
        helper_runner(executable, helper, expected),
    )
    .await?;
    Ok(layout
        .paths()
        .logs()
        .join(format!("{}.log", atlas_daemon::user::setup::LABEL)))
}

/// Ready first-login preferences only while the chosen helper is logged out.
/// An incomplete reply retains its executor-bound pending mutation.
///
/// # Errors
///
/// Returns account-identity, privileged-executor or helper-installation failures. An
/// executor whose exit is unknown retains ownership.
pub fn first_login_setup(client: &Client, helper: &Account, expected: &Identity) -> Result<Value> {
    let executable = Path::new(app_bundle::EXECUTABLE);
    setup::executor::setup_transaction(&client.controller, || {
        let request = atlas_daemon::user::setup::Request::FirstLogin {
            uid: helper.uid,
            opener: false,
        };
        setup::request_as_helper(
            executable,
            helper,
            &request,
            setup::Invocation::BeforeLogin,
            expected,
        )?
        .map_err(|unfinished| {
            unfinished
                .behind(problem("The helper did not acknowledge first-login setup"))
                .uncertain()
        })
    })
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
