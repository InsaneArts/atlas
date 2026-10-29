//! Which process a command about an app is for: a process ID, or a name
//! found among the helper's windows and running apps.
use super::windows::verified_window_list;
use crate::{
    client::{Client, Verified},
    error::{Code, Error, Result},
    native::windows::{MENU_LAYER, Window},
    ui::text::same_text,
};
use serde_json::Value;

/// The app an element command is about: a PID, or a name that must resolve
/// to exactly one process with a window of its own on screen.
#[derive(Debug, Clone)]
pub enum Target {
    /// The app with this name, whatever its case.
    Name(String),
    /// The process with this ID.
    Pid(i64),
}

/// The processes named `name` that have windows on screen, by the kind of
/// window: ordinary ones first, then floating panels and alerts, then open
/// menus. An app showing only one of those is found by it.
pub(super) fn app_pids(windows: &[Window], name: &str) -> Vec<i64> {
    let candidates: Vec<&Window> = windows
        .iter()
        .filter(|window| {
            window.is_apps_or_menu()
                && window
                    .owner
                    .as_deref()
                    .is_some_and(|owner| same_text(owner, name))
        })
        .collect();

    let tiers: [&dyn Fn(&Window) -> bool; 3] = [
        &|window| window.layer == 0,
        &|window| window.layer != MENU_LAYER,
        &|_| true,
    ];

    let mut pids: Vec<i64> = tiers
        .iter()
        .map(|tier| {
            candidates
                .iter()
                .filter(|window| tier(window))
                .map(|window| window.pid)
                .collect::<Vec<_>>()
        })
        .find(|pids| !pids.is_empty())
        .unwrap_or_default();
    pids.sort_unstable();
    pids.dedup();

    pids
}

/// The one process among `pids` a command about `name` is for.
pub(super) fn one_pid(name: &str, pids: &[i64]) -> Result<i64> {
    match pids {
        [pid] => Ok(*pid),
        [] => Err(Error::new(
            Code::Elements,
            format!("No window of {name} is on screen"),
        )),
        _ => Err(Error::new(
            Code::Elements,
            format!("Several {name} processes have windows ({pids:?}); pass --pid"),
        )),
    }
}

/// The process a command is about: the one with the app's windows, or,
/// when none of its windows is open, the active app of that name (its
/// menu bar is still on screen). Several apps of the name with windows
/// stay ambiguous. It is found in the helper `verified` describes, which
/// the caller verified just before, so it is not verified again for each
/// list it reads.
pub(super) async fn target_pid(
    client: &Client,
    verified: &Verified,
    target: &Target,
) -> Result<i64> {
    let listed = match target {
        Target::Pid(_) => Vec::new(),
        Target::Name(_) => verified_window_list(client, verified).await?,
    };
    target_pid_among(client, verified, target, &listed).await
}

/// [`target_pid`] with the helper's windows `listed` already read; a
/// process ID needs none of them.
pub(super) async fn target_pid_among(
    client: &Client,
    verified: &Verified,
    target: &Target,
    listed: &[Window],
) -> Result<i64> {
    let name = match target {
        Target::Pid(pid) => {
            let apps = client.apps_in(verified).await?;
            let running = apps["apps"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|app| app["pid"] == *pid);
            if !running {
                return Err(Error::new(
                    Code::Elements,
                    format!(
                        "No app in the helper has the process ID {pid}; run atlas apps to see them"
                    ),
                ));
            }

            return Ok(*pid);
        }
        Target::Name(name) => name,
    };

    let pids = app_pids(listed, name);
    if !pids.is_empty() {
        return one_pid(name, &pids);
    }

    let apps = client.apps_in(verified).await?;
    let named: Vec<&Value> = apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|app| app_named(app, name))
        .collect();
    if let Some(pid) = named
        .iter()
        .find(|app| app["active"] == true)
        .and_then(|app| app["pid"].as_i64())
    {
        return Ok(pid);
    }

    Err(Error::new(
        Code::Elements,
        if named.is_empty() {
            format!("{name} is not running: run atlas open --app {name}")
        } else {
            format!(
                "No window of {name} is on screen: open one in it, or run atlas wait --app {name}"
            )
        },
    ))
}

/// Whether an `apps` row is the app called `name`: by the name it shows,
/// which may be localized, or by the file name of its executable or of its
/// own (innermost) bundle.
pub(super) fn app_named(app: &Value, name: &str) -> bool {
    let shown = app["name"].as_str().is_some_and(|own| same_text(own, name));
    let executable = app["executable"].as_str().map(std::path::Path::new);
    let bundle = executable.and_then(|path| {
        path.ancestors()
            .find(|ancestor| ancestor.extension().is_some_and(|ext| ext == "app"))
    });

    shown
        || [executable, bundle]
            .into_iter()
            .flatten()
            .filter_map(|path| path.file_stem()?.to_str())
            .any(|own| same_text(own, name))
}
