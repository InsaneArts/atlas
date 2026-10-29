//! Opening apps, files and URLs in the helper: what this side holds is
//! handed over, `open` launches it, and a launch is tried again when
//! launchd still holds a finished job for the app or the app is known only
//! by the task's own install.
use crate::{
    client::Client,
    control::Ownership,
    error::{Error, Result},
    handoff, task,
    transport::{accepted, run_output},
};
use serde_json::{Value, json};
use std::path::Path;

/// `open` inside the helper session: an app by name, files or URLs, or both.
pub async fn open(
    client: &Client,
    app: Option<&str>,
    targets: &[String],
    environment: &[String],
) -> Result<Value> {
    open_in(client, app, targets, environment, Path::new(handoff::ROOT)).await
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
            "usage",
            "Name an app with --app, a file or URL, or both",
        ));
    }

    let mut argv = open_argv(environment)?;

    // Copying, launching and pruning happen under one ownership, so no other
    // open, close or controller interleaves; only commands agents started
    // with `run` go on beside it.
    let ownership = client.controller.acquire_beside_commands()?;
    let running = handoff::Running::listed(&client.apps().await?);
    task::settle_copies_gone(client, handoff::prune_in(copies, &running));

    let handed = handoff::hand_over(client, &ownership, wanted(app, targets), &running).await?;
    let copied = handed.copied;
    let mut given = handed.paths.into_iter();

    // Where the app's name or path is in `argv`, for a launch tried again
    // from the task's own install.
    let mut named_at = None;
    if app.is_some() {
        argv.push("-a".to_owned());
        named_at = Some(argv.len());
        argv.push(given.next().unwrap_or_default());
    }

    if !targets.is_empty() {
        argv.push("--".to_owned());
        argv.extend(given);
    }

    let (reply, from_install) = launch(client, &ownership, app, argv, named_at).await?;

    let opened = reply["exitCode"] == 0;
    // An app opened by name is only listed for its task: which of its
    // processes the launch started, nothing can tell. One opened from the
    // task's own install is quit by its cleanup, as the install's.
    if let Some(app) =
        app.filter(|app| opened && !from_install && copied.is_empty() && !app.contains('/'))
    {
        let launch = task::Owned::Launch {
            app: crate::doing::bounded(app),
        };
        let _ = task::Claims::of(client).claim(&launch, task::State::Made);
    }

    if !opened {
        return Ok(json!(not_opened(&reply, &copied)));
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
                "usage",
                format!("--env takes NAME=VALUE, not {variable}"),
            ));
        }

        argv.extend(["--env".to_owned(), variable.clone()]);
    }

    Ok(argv)
}

/// What `open` hands over to the helper: the app first, then each file or
/// URL.
fn wanted(app: Option<&str>, targets: &[String]) -> Vec<handoff::Target> {
    // A name is the helper's to look up; a path is this side's file.
    let app_target = app.map(|app| {
        if app.contains('/') {
            handoff::resolve(app)
        } else {
            handoff::Target::Name(app.to_owned())
        }
    });
    let mut wanted: Vec<handoff::Target> = app_target.into_iter().collect();
    wanted.extend(targets.iter().map(|target| handoff::resolve(target)));

    wanted
}

/// Run `open`'s `argv`, again once a finished job launchd still held for
/// the app is removed, and from the task's own install when LaunchServices
/// does not know the app named at `named_at`. Its last reply, and whether
/// that launch was from the install.
async fn launch(
    client: &Client,
    ownership: &Ownership,
    app: Option<&str>,
    mut argv: Vec<String>,
    named_at: Option<usize>,
) -> Result<(Value, bool)> {
    let mut reply = accepted(client.run_owned(ownership, argv.clone()).await?)?;

    if reply["exitCode"] != 0 && cleared_stale_launch(client, ownership, &reply).await? {
        reply = accepted(client.run_owned(ownership, argv.clone()).await?)?;
    }

    // An app this task installed that LaunchServices does not know by its
    // name yet opens from where the install is.
    if let (Some(app), Some(at)) = (app.filter(|app| !app.contains('/')), named_at)
        && reply["exitCode"] != 0
        && unknown_app(&reply)
        && let Some(path) = crate::installs::installed_path(client, app).await
    {
        argv[at] = path;
        reply = accepted(client.run_owned(ownership, argv).await?)?;
        return Ok((reply, true));
    }

    Ok((reply, false))
}

/// A launch that failed, said with its code and with the `copied` copies
/// made for it, which stay reported: the launch may still be on its way (a
/// Gatekeeper question, say), and close removes them.
fn not_opened(reply: &Value, copied: &[crate::handoff::Copied]) -> Error {
    let mut error = if reply["timedOut"] == true {
        Error::new(
            "wait-failed",
            "macOS did not finish opening that in time; it may still be launching: \
             run atlas wait --app NAME, or look with atlas apps",
        )
        .with("timedOut", true)
    } else {
        let output = run_output(reply, 4096).unwrap_or_default();
        let said = String::from_utf8_lossy(&output).trim().to_owned();
        Error::new(
            "spawn-failed",
            if said.is_empty() {
                "macOS could not open that in the helper".to_owned()
            } else {
                said
            },
        )
    };

    error = error.with("opened", false);
    if !copied.is_empty() {
        error = error.with("copied", copied);
    }

    error
}

/// Whether a failed `open` said LaunchServices knows no app by the name
/// it was given.
fn unknown_app(reply: &Value) -> bool {
    let output = run_output(reply, 4096).unwrap_or_default();
    String::from_utf8_lossy(&output).contains("Unable to find application named")
}

/// After a failed `open`, remove the finished job launchd still holds for
/// its app, when that is why it failed: an app opened again just as it quit
/// can leave one, and every later launch fails until it is gone. Whether
/// one was removed, so the launch is worth trying again.
async fn cleared_stale_launch(
    client: &Client,
    ownership: &Ownership,
    reply: &Value,
) -> Result<bool> {
    let output = run_output(reply, 4096).unwrap_or_default();
    let said = String::from_utf8_lossy(&output);

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
    let cleared = accepted(client.run_owned(ownership, argv).await?)?;

    Ok(!run_output(&cleared, 4096).unwrap_or_default().is_empty())
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
