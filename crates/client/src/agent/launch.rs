//! Opening apps, files and URLs in the helper: what this side holds is
//! handed over, `open` launches it, and a launch is tried again when
//! launchd still holds a finished job for the app or the app is known only
//! by the task's own install.
use crate::{
    client::{AppList, AppRow, Client, Finished, Verified, WINDOW_LOOKUP},
    handoff, task,
};
use atlas_control::{Ownership, SharedLease};
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Browsers that open the URLs given after `--new-window` in a window of
/// their own when they are already running, by bundle ID: two agents' pages
/// then never share a window.
const OWN_WINDOW_BROWSERS: [&str; 5] = [
    "com.google.Chrome",
    "com.google.Chrome.canary",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.microsoft.edgemac",
];

/// How much of what `open` and its cleanup print is read.
const OUTPUT: usize = 4096;

/// `open` inside the helper session: an app by name, files or URLs, or both.
///
/// # Errors
///
/// Returns argument, ownership, identity, launch or recording failures. A lost
/// acknowledgement may follow a successful launch; an uncertain result must not be
/// replayed.
pub async fn open(
    client: &Client,
    app: Option<&str>,
    targets: &[String],
    environment: &[String],
) -> Result<Value> {
    open_in(
        client,
        app,
        targets,
        environment,
        &handoff::copies_folder(&client.desktop()),
    )
    .await
}

/// Open `url` in the browser `app` (by its name) for the person, under the
/// control their lease holds (`ownership`): nothing is handed over and no
/// task claims it. With `new_window`, the page gets a window of its own: a
/// web page is the argument of the browser started anew, as Chromium
/// browsers pass it to the copy already running; one of the browser's own
/// pages, which Chromium ignores as an argument, opens as a URL once the
/// new window shows, filling its empty tab.
/// Each launch, a retry included, is sent only while the person's `lease`
/// still holds its token, checked once connected and held until it is
/// sent; success is said only while it still holds.
///
/// # Errors
///
/// Returns `lease-lost` once the lease ran out or was released, identity,
/// launch and recording failures, and `spawn-failed` when macOS could not
/// open it. A lost acknowledgement may follow a launch that went ahead; its
/// outcome stays pending as any run's.
pub async fn open_for_person(
    client: &Client,
    ownership: &Ownership,
    lease: (&SharedLease, &str),
    app: &str,
    url: &str,
    new_window: bool,
) -> Result<Value> {
    let open_url = [
        "-a".to_owned(),
        app.to_owned(),
        "--".to_owned(),
        url.to_owned(),
    ];
    if !new_window {
        return launch_for_person(client, ownership, lease, &open_url).await;
    }
    let mut new = vec!["-n".to_owned(), "-a".to_owned(), app.to_owned()];
    new.extend(["--args".to_owned(), "--new-window".to_owned()]);
    if web_page(url) {
        new.push(url.to_owned());
        return launch_for_person(client, ownership, lease, &new).await;
    }
    let verified = client.verify().await?;
    let before = windows_of(client, &verified, app).await;
    launch_for_person(client, ownership, lease, &new).await?;
    let waited = Instant::now();
    while windows_of(client, &verified, app).await <= before && waited.elapsed() < NEW_WINDOW_WAIT {
        tokio::time::sleep(NEW_WINDOW_POLL).await;
    }
    launch_for_person(client, ownership, lease, &open_url).await
}

/// How long a browser's own page waits for the window opened for it, before
/// it opens wherever the browser puts it.
const NEW_WINDOW_WAIT: Duration = Duration::from_secs(5);

/// How often the helper's windows are read while it waits.
const NEW_WINDOW_POLL: Duration = Duration::from_millis(100);

/// How many windows `app`, by its name, shows in the helper `verified`;
/// none when they can't be read.
async fn windows_of(client: &Client, verified: &Verified, app: &str) -> usize {
    client
        .windows_in(Some(verified), false, WINDOW_LOOKUP)
        .await
        .map_or(0, |shown| {
            shown
                .windows
                .iter()
                .filter(|window| window.owner.as_deref() == Some(app))
                .count()
        })
}

/// One `open` with `arguments` for the person, under their lease.
async fn launch_for_person(
    client: &Client,
    ownership: &Ownership,
    lease: (&SharedLease, &str),
    arguments: &[String],
) -> Result<Value> {
    let mut argv = vec!["/usr/bin/open".to_owned()];
    argv.extend_from_slice(arguments);
    let leased = Some(lease);
    let mut finished = client
        .run_finished_under(ownership, leased, argv.clone(), OUTPUT)
        .await?;
    if !finished.succeeded && cleared_stale_launch(client, ownership, leased, &finished).await? {
        finished = client
            .run_finished_under(ownership, leased, argv, OUTPUT)
            .await?;
    }
    // A lease that ran out meanwhile ends the person's flow, opened or not.
    lease.0.lock().await.require(lease.1)?;
    if !finished.succeeded {
        return Err(not_opened(&finished, &[]));
    }
    Ok(json!({"ok": true, "opened": true}))
}

/// [`open`], pruning `open`'s old copies under `copies`.
pub(super) async fn open_in(
    client: &Client,
    app: Option<&str>,
    targets: &[String],
    environment: &[String],
    copies: &Path,
) -> Result<Value> {
    if app.is_none() && targets.is_empty() {
        return Err(Error::new(
            Code::Usage,
            "Name an app with --app, a file or URL, or both",
        ));
    }

    let mut argv = open_argv(environment)?;
    let wanted = wanted(app, targets);
    // Before the helper is asked anything: the person's sign-ins and secrets never reach it, by the path given (a link
    // named like a store) or the one it leads to.
    crate::credential_files::refuse(given_paths(app, targets, &wanted))?;
    handoff::refuse_targets(&wanted)?;
    let started = Instant::now();

    // Copying, launching and pruning happen under one ownership, so no other
    // open, close or controller interleaves; only commands agents started
    // with `run` go on beside it.
    let ownership = Arc::new(client.controller.acquire_beside_commands()?);
    let verified = client.verify().await?;
    let listed = client.apps_in(&verified).await?;
    let running = listed.rows();
    task::settle_copies_gone(client, handoff::prune_in(copies, running));

    let handed = handoff::hand_over(client, &ownership, wanted, running).await?;
    let copied = handed.copied;
    // Web pages for a browser already running get a window of their own:
    // the app as the helper opens it, by its name or the path handed over.
    let own_window = app.is_some()
        && !targets.is_empty()
        && targets.iter().all(|target| web_page(target))
        && handed
            .paths
            .first()
            .and_then(|app| running_app(&listed, app))
            .and_then(AppRow::bundle_id)
            .is_some_and(|bundle| OWN_WINDOW_BROWSERS.contains(&bundle));
    let mut given = handed.paths.into_iter();

    // Where the app's name or path is in `argv`, for a launch tried again
    // from the task's own install.
    let mut named_at = None;
    if own_window {
        argv.push("-n".to_owned());
    }
    if app.is_some() {
        argv.push("-a".to_owned());
        named_at = Some(argv.len());
        argv.push(given.next().unwrap_or_default());
    }

    if own_window {
        argv.extend(["--args".to_owned(), "--new-window".to_owned()]);
        argv.extend(given);
    } else if !targets.is_empty() {
        argv.push("--".to_owned());
        argv.extend(given);
    }

    let (finished, from_install) = launch(client, &ownership, app, argv, named_at).await?;

    let opened = finished.succeeded;
    // An app opened by name is only listed for its task: which of its
    // processes the launch started, nothing can tell. One opened from the
    // task's own install is quit by its cleanup, as the install's.
    if let Some(app) =
        app.filter(|app| opened && !from_install && copied.is_empty() && !app.contains('/'))
    {
        let launch = task::Owned::Launch {
            app: atlas_core::name::bounded(app),
        };
        let _ = task::Claims::of(client).claim(&launch, task::State::Made);
    }

    if !opened {
        return Ok(json!(not_opened(&finished, &copied)));
    }

    // A newly launched app was absent from the pre-launch list. Its
    // identity comes from the helper after the successful launch; a read
    // that fails must not turn an already completed launch into a failure.
    if let Some(name) = client
        .doing
        .as_ref()
        .and_then(crate::doing::Doing::app_name)
        && let Ok(current) = client.verify().await
        && current.session().epoch == verified.session().epoch
        && let Ok(apps) = client.apps_in(&verified).await
        && let Some(app) = super::target::named_app(&apps, name)
    {
        client
            .record_app_completion(app.name(), app.bundle_id(), started.elapsed())
            .await;
    }

    let mut value = json!({ "ok": true, "opened": true });
    if !copied.is_empty() {
        value["copied"] = json!(copied);
    }

    Ok(value)
}

/// The start of `open`'s command line: each of `environment`, which must be
/// NAME=VALUE, passed to what it opens.
fn open_argv(environment: &[String]) -> Result<Vec<String>> {
    let mut argv = vec!["/usr/bin/open".to_owned()];

    for variable in environment {
        if variable
            .split_once('=')
            .is_none_or(|(name, _)| name.is_empty())
        {
            return Err(Error::new(
                Code::Usage,
                format!("--env takes NAME=VALUE, not {variable}"),
            ));
        }

        argv.extend(["--env".to_owned(), variable.clone()]);
    }

    Ok(argv)
}

/// The one app running in the helper that `app` names: by its name, or,
/// for a path, the app whose executable is inside that bundle. Several
/// processes of it must agree on its bundle.
fn running_app<'a>(listed: &'a AppList, app: &str) -> Option<&'a AppRow> {
    if !app.contains('/') {
        return super::target::named_app(listed, app);
    }

    let inside = Path::new(app).join("Contents/MacOS");
    let mut running = listed.rows().iter().filter(|row| {
        row.executable()
            .is_some_and(|executable| Path::new(executable).starts_with(&inside))
    });
    let first = running.next()?;
    running
        .all(|row| row.bundle_id() == first.bundle_id())
        .then_some(first)
}

/// Whether `target` is a web page's address.
fn web_page(target: &str) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        target
            .get(..scheme.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(scheme))
    })
}

/// The arguments of `open` to check as given, beside `wanted`, what they resolved to: each file named, whether or
/// not it exists (a missing `.env` is no URL), and whatever exists here, however it looks. An app given by name is
/// the helper's to look up.
fn given_paths<'a>(
    app: Option<&'a str>,
    targets: &'a [String],
    wanted: &[handoff::Target],
) -> Vec<&'a Path> {
    app.into_iter()
        .map(|app| (app, app.contains('/')))
        .chain(
            targets
                .iter()
                .map(|target| (target.as_str(), !handoff::has_scheme(target))),
        )
        .zip(wanted)
        .filter(|((_, file), target)| *file || matches!(target, handoff::Target::Path(_)))
        .map(|((given, _), _)| Path::new(given))
        .collect()
}

/// What `open` hands over to the helper: the app first, then each file or
/// URL.
fn wanted(app: Option<&str>, targets: &[String]) -> Vec<handoff::Target> {
    // A name is the helper's to look up; a path is this side's file.
    let app = app.map(|app| {
        if app.contains('/') {
            handoff::resolve(app)
        } else {
            handoff::Target::Name(app.to_owned())
        }
    });

    app.into_iter()
        .chain(targets.iter().map(|target| handoff::resolve(target)))
        .collect()
}

/// Run `open`'s `argv`, again once a finished job launchd still held for
/// the app is removed, and from the task's own install when LaunchServices
/// does not know the app named at `named_at`. How the last run ended, and
/// whether that launch was from the install.
async fn launch(
    client: &Client,
    ownership: &Ownership,
    app: Option<&str>,
    mut argv: Vec<String>,
    named_at: Option<usize>,
) -> Result<(Finished, bool)> {
    let mut finished = client.run_finished(ownership, argv.clone(), OUTPUT).await?;

    if !finished.succeeded && cleared_stale_launch(client, ownership, None, &finished).await? {
        finished = client.run_finished(ownership, argv.clone(), OUTPUT).await?;
    }

    // An app this task installed that LaunchServices does not know by its
    // name yet opens from where the install is.
    if let (Some(app), Some(at)) = (app.filter(|app| !app.contains('/')), named_at)
        && !finished.succeeded
        && finished.said().contains("Unable to find application named")
        && let Some(path) = crate::install_app::installed_path(client, app).await
    {
        argv[at] = path;
        let finished = client.run_finished(ownership, argv, OUTPUT).await?;
        return Ok((finished, true));
    }

    Ok((finished, false))
}

/// How `open`'s message ends for an app that runs in one account at a time
/// and is already open in another (`kLSMultipleSessionsNotSupportedErr`).
/// Only the end is matched: the app's name and path come before it.
const ONE_ACCOUNT_AT_A_TIME: &str = "with error -10829.";

/// A launch that failed, said with its code and with the `copied` copies
/// made for it, which stay reported: the launch may still be on its way (a
/// Gatekeeper question, say), and close removes them.
fn not_opened(finished: &Finished, copied: &[crate::handoff::Copied]) -> Error {
    let mut error = if finished.timed_out {
        Error::new(
            Code::WaitFailed,
            "macOS did not finish opening that in time; it may still be launching: \
             run atlas wait --app NAME, or look with atlas apps",
        )
        .with("timedOut", json!(true))
    } else {
        let said = finished.said();
        let said = said.trim();
        let error = if said.ends_with(ONE_ACCOUNT_AT_A_TIME) {
            Error::new(
                Code::SpawnFailed,
                "That app runs in one account at a time, and it is already open in another \
                 account on this Mac.",
            )
            .with(
                "next",
                json!(atlas_core::error::advice::OPEN_IN_ANOTHER_ACCOUNT),
            )
        } else {
            Error::new(
                Code::SpawnFailed,
                if said.is_empty() {
                    "macOS could not open that in the helper"
                } else {
                    said
                },
            )
        };
        match finished.unread() {
            Some(cause) => error.with_source(cause),
            None => error,
        }
    };

    error = error.with("opened", json!(false));
    if !copied.is_empty() {
        // Non-Unicode paths leave the whole copied detail null, as before.
        let copied: Option<Vec<Value>> = copied
            .iter()
            .map(|copy| Some(json!({"from": copy.from.to_str()?, "to": copy.to.to_str()?})))
            .collect();
        error = error.with("copied", copied.map_or(Value::Null, Value::Array));
    }

    error
}

/// After a failed `open`, remove the finished job launchd still holds for
/// its app, when that is why it failed: an app opened again just as it quit
/// can leave one, and every later launch fails until it is gone. Whether
/// one was removed, so the launch is worth trying again.
async fn cleared_stale_launch(
    client: &Client,
    ownership: &Ownership,
    lease: Option<(&SharedLease, &str)>,
    failed: &Finished,
) -> Result<bool> {
    let said = failed.said();
    let Some(bundle) = stale_launch(&said) else {
        return Ok(false);
    };

    let argv = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        CLEAR_STALE_JOB.to_owned(),
        "sh".to_owned(),
        bundle.to_owned(),
    ];
    let cleared = client
        .run_finished_under(ownership, lease, argv, OUTPUT)
        .await?;

    Ok(!cleared.output().is_empty())
}

/// The app whose launch launchd refused because it still holds that app's
/// finished job (error 37, "Operation already in progress"), from what
/// `open` said.
pub(super) fn stale_launch(said: &str) -> Option<&str> {
    if !said.contains("OSLaunchdErrorDomain Code=37") {
        return None;
    }

    let rest = said.split_once("The application ")?.1;
    let end = rest.find(".app ")? + ".app".len();
    let app = &rest[..end];

    app.starts_with('/').then_some(app)
}

/// In the helper, remove launchd's jobs for the app at `$1` that are not
/// running (none of its windows or processes go with them): exactly the
/// labels `application.<its bundle id>.<n>.<n>`, never an app whose bundle
/// id merely begins with it. It prints only the labels it removed. The next
/// launch submits a fresh job.
const CLEAR_STALE_JOB: &str = r#"id=$(/usr/bin/plutil -extract CFBundleIdentifier raw -o - "$1/Contents/Info.plist" 2>/dev/null) || exit 0
case "$id" in *[!A-Za-z0-9.-]*|'') exit 0;; esac
/bin/launchctl list | /usr/bin/awk -v prefix="application.$id." '
  $1 == "-" && index(tolower($3), tolower(prefix)) == 1 &&
  substr($3, length(prefix) + 1) ~ /^[0-9]+\.[0-9]+$/ { print $3 }' |
while read -r label; do
  /bin/launchctl remove "$label" 2>/dev/null && echo "$label"
done
exit 0"#;

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
