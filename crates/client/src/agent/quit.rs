//! Quitting apps in the helper: each process is signalled only while it is
//! still the one listed, and waited for until none is left; closing one app
//! or every app also removes the copies `open` made of them.
use crate::{
    app_selection::{Closing, Process, atlas_bundle, selected},
    client::{AppList, Client},
    handoff, task,
};
use atlas_control::Ownership;
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// The names of the selected processes that were (or, with `quit` false,
/// were not) among the `signalled` ones, sorted, each once.
pub(super) fn names_by_quit(processes: &[Process], signalled: &[u64], quit: bool) -> Vec<String> {
    sorted(
        processes
            .iter()
            .filter(|process| signalled.contains(&process.pid) == quit)
            .map(|process| process.name.clone()),
    )
}

/// `names` sorted, each once.
fn sorted(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = names.collect();
    names.sort();
    names.dedup();

    names
}

/// The processes the quit script signalled, one ID per line of its output.
pub(super) fn signalled(output: &[u8]) -> Vec<u64> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// How long `close` waits for the apps it signalled to quit.
pub(crate) const QUIT_WAIT: Duration = Duration::from_secs(10);

/// In the helper, each process is signalled only while it is still the
/// executable listed, so a reused process ID is left alone, and, when a
/// folder is given with it, only while that folder still has the node it
/// was proven to have (`stat` without `-L` looks at a link itself, never
/// through it). It prints each process it signalled.
pub(super) const QUIT: &str = r#"while [ $# -ge 4 ]; do
  pid=$1 exe=$2 folder=$3 node=$4; shift 4
  [ -n "$exe" ] || continue
  [ "$(/bin/ps -o comm= -p "$pid")" = "$exe" ] || continue
  if [ -n "$folder" ]; then
    [ "$(/usr/bin/stat -f %d:%i "$folder" 2>/dev/null)" = "$node" ] || continue
  fi
  /bin/kill "$pid" && echo "$pid"
done
exit 0"#;

/// What quitting apps did.
pub(crate) struct Quit {
    /// The processes that were signalled.
    pub(crate) signalled: Vec<u64>,
    /// The first valid app list in which none of them was left, which
    /// copies are judged against.
    pub(crate) after: AppList,
}

/// How waiting for apps to quit ended.
pub(crate) enum Quitting {
    /// None is left.
    Gone(Quit),
    /// These were still running when the wait ran out.
    TimedOut { still: Vec<String> },
}

/// Signal `processes`, then wait up to `limit` until `left` (given an app
/// list and the processes signalled) names none still running in a fresh,
/// valid app list. A list that cannot be read after the signal ends the
/// wait with its error, naming what was signalled: nothing is judged
/// without proof the apps exited.
pub(crate) async fn quit_until(
    client: &Client,
    ownership: &Ownership,
    processes: &[Process],
    left: impl Fn(&AppList, &[u64]) -> Vec<String>,
    limit: Duration,
) -> Result<Quitting> {
    let mut argv = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        QUIT.to_owned(),
        "sh".to_owned(),
    ];
    for process in processes {
        let (folder, node) = process
            .within
            .as_ref()
            .map(|within| (within.folder.clone(), within.node.clone()))
            .unwrap_or_default();
        argv.extend([
            process.pid.to_string(),
            process.executable.clone(),
            folder,
            node,
        ]);
    }

    let finished = client.run_finished(ownership, argv, 64 * 1024).await?;
    if finished.timed_out || !finished.succeeded {
        return Err(Error::new(
            Code::WaitFailed,
            "The helper did not finish quitting the apps; run atlas apps to see what is open",
        ));
    }

    // Only the processes signalled are waited for: one whose program did
    // not check out was left alone.
    let signalled = self::signalled(finished.output());
    let deadline = Instant::now() + limit;

    loop {
        let after = client
            .running_apps()
            .await
            .map_err(|error| unconfirmed(error, &names_by_quit(processes, &signalled, true)))?;
        let still = left(&after, &signalled);
        if still.is_empty() {
            return Ok(Quitting::Gone(Quit { signalled, after }));
        }
        if Instant::now() >= deadline {
            return Ok(Quitting::TimedOut { still });
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// `error`, from reading the app list after `names` were asked to quit, as
/// what it leaves: they may or may not have exited, and their copies stay.
/// The signal's own reply was received, so it is not uncertain.
pub(super) fn unconfirmed(mut error: Error, names: &[String]) -> Error {
    let named: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty())
        .collect();
    let asked = match (names.is_empty(), named.is_empty()) {
        (true, _) => "Atlas could not read the helper's apps to confirm the apps it was closing \
                      exited"
            .to_owned(),
        (false, true) => "Atlas asked the apps it was closing to quit but could not read the \
                          helper's apps to confirm they exited"
            .to_owned(),
        (false, false) => format!(
            "Atlas asked {} to quit but could not read the helper's apps to confirm they exited",
            named.join(", ")
        ),
    };
    error.error = format!(
        "{asked} ({}). Their copies were kept; run atlas close --app NAME again.",
        error.error
    );
    error.with("signalled", json!(named))
}

/// Quit apps in the helper: one by name (every process of it), or with
/// [`Closing::All`] every app that is not part of macOS or Atlas. Waits
/// until they are gone, then removes the copies `open` made of them.
///
/// # Errors
///
/// Returns selection, ownership, identity or close failures. A refusal can follow earlier
/// completed closes; unknown outcomes retain their recovery records.
pub async fn close(client: &Client, closing: Closing<'_>) -> Result<Value> {
    close_in(client, closing, Path::new(handoff::ROOT)).await
}

/// [`close`], with `open`'s copies under `copies`.
pub(super) async fn close_in(
    client: &Client,
    closing: Closing<'_>,
    copies: &Path,
) -> Result<Value> {
    if let Closing::App(app) = closing
        && app.contains('/')
    {
        return Err(Error::new(
            Code::Usage,
            "close takes the app's name as atlas apps lists it, not a path",
        ));
    }

    let ownership = client.controller.acquire_beside_commands()?;
    let started = Instant::now();
    let atlas = atlas_bundle();
    let apps = client.running_apps().await?;
    let processes = selected(&apps, closing, atlas.as_deref());
    // Keep only identity, before the app disappears from the running list.
    let identity = match closing {
        Closing::App(name) => super::target::named_app(&apps, name).map(|app| {
            (
                app.name().map(str::to_owned),
                app.bundle_id().map(str::to_owned),
            )
        }),
        Closing::All => None,
    };

    // The helper's session and Atlas's own services are never closed.
    if let Closing::App(app) = closing
        && processes.iter().any(|process| process.protected)
    {
        return Err(Error::new(
            Code::Input,
            format!("Atlas does not close {app}: it is part of macOS or of Atlas"),
        ));
    }

    // What runs in the helper once the apps are gone, from the list that
    // proved it; with nothing to quit, what ran already.
    let (after, quit) = if processes.is_empty() {
        (apps, Vec::new())
    } else {
        // One app is closed once no process of its name is left (a relaunch
        // counts); everything once each process signalled is gone, whatever
        // started since.
        let left = |apps: &AppList, pids: &[u64]| {
            sorted(
                selected(apps, closing, atlas.as_deref())
                    .into_iter()
                    .filter(|process| match closing {
                        Closing::App(_) => true,
                        Closing::All => pids.contains(&process.pid),
                    })
                    .map(|process| process.name),
            )
        };
        let quit = match quit_until(client, &ownership, &processes, left, QUIT_WAIT).await? {
            Quitting::Gone(quit) => quit,
            Quitting::TimedOut { still } => {
                return Err(Error::new(
                    Code::WaitFailed,
                    format!(
                        "{} did not quit within {} seconds",
                        still.join(", "),
                        QUIT_WAIT.as_secs()
                    ),
                ));
            }
        };
        (quit.after, quit.signalled)
    };

    let names = match closing {
        Closing::App(app) => vec![app.to_owned()],
        Closing::All => names_by_quit(&processes, &quit, true),
    };
    let removed = remove_copies(client, copies, &names, &after);
    let reply = closed(closing, &processes, &quit, names, removed);

    if let Some((name, bundle_id)) = identity {
        client
            .record_app_completion(name.as_deref(), bundle_id.as_deref(), started.elapsed())
            .await;
    }
    Ok(reply)
}

/// Remove the copies of `names`, once `after` proved those apps gone.
fn remove_copies(
    client: &Client,
    copies: &Path,
    names: &[String],
    after: &AppList,
) -> handoff::Removed {
    // A copy is never removed while an app of its names runs: an app that
    // came back, or another one, may be running from it. An agent's close
    // keeps the copies another agent session made.
    let running = handoff::Running::listed(after);
    let session = client.host_session.as_ref().map(|session| session.key());
    let mut removed = handoff::Removed::default();
    for name in names {
        let gone = handoff::remove_copies_in(copies, name, &running, session);
        removed.folders.extend(gone.folders);
        removed.kept_others += gone.kept_others;
        removed.failed.extend(gone.failed);
    }
    task::settle_copies_gone(client, removed.tokens());

    removed
}

/// The reply to a close: what it closed, skipped and removed.
fn closed(
    closing: Closing<'_>,
    processes: &[Process],
    signalled: &[u64],
    names: Vec<String>,
    removed: handoff::Removed,
) -> Value {
    let mut reply = json!({
        "ok": true,
        "closed": !processes.is_empty(),
        "removed": removed.folders,
    });
    if removed.kept_others > 0 {
        reply["keptOthers"] = json!(removed.kept_others);
    }
    // The app closed: a copy that would not go is said, not dropped.
    if !removed.failed.is_empty() {
        reply["failed"] = json!(removed.failed);
    }

    match closing {
        Closing::App(app) => {
            reply["app"] = json!(app);
            if processes.is_empty() {
                reply["reason"] = json!(format!("{app} is not running"));
            }
        }
        Closing::All => {
            reply["closed"] = json!(!names.is_empty());
            reply["apps"] = json!(names);
            let skipped = names_by_quit(processes, signalled, false);
            if !skipped.is_empty() {
                reply["skipped"] = json!(skipped);
            }
        }
    }

    reply
}
