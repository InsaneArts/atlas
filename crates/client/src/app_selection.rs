//! Shared application-selection policy for close-all and held logout.
use crate::client::{AppList, AppRow};
use atlas_core::{
    desktop::{Desktop, Kind},
    name::same_text,
};
use serde_json::{Value, json};
use std::path::Path;

/// Whether a running app (as `apps` lists it) belongs to macOS or to Atlas
/// itself; neither is ever closed. Atlas is known by its bundle identifier
/// too, so a development build never closes the installed app.
fn protected(running: &AppRow, atlas: Option<&str>) -> bool {
    let executable = running.executable().unwrap_or("");
    (["/System/Library/", "/usr/", "/Library/Apple/"]
        .iter()
        .any(|system| executable.starts_with(system))
        && !executable.starts_with("/usr/local/"))
        || atlas.is_some_and(|bundle| executable.starts_with(bundle))
        || running.bundle_id().is_some_and(|id| {
            id == atlas_core::app_bundle::IDENTIFIER
                || id
                    .strip_prefix(atlas_core::app_bundle::IDENTIFIER)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
}

/// Whether `executable` is the main program of an app someone can open:
/// `/Applications/Notes.app/Contents/MacOS/Notes`, or an app shipped inside
/// another (Simulator in Xcode), but not a helper app, framework program or
/// XPC service inside an app.
fn app_program(executable: &str) -> bool {
    let Some((bundle, program)) = executable.rsplit_once(".app/Contents/MacOS/") else {
        return false;
    };

    executable.starts_with('/')
        && !program.is_empty()
        && !program.contains('/')
        && ![
            "/Contents/Frameworks/",
            "/Contents/XPCServices/",
            "/Contents/Library/",
            "/Contents/Helpers/",
            "/Contents/PlugIns/",
            ".xpc/",
        ]
        .iter()
        .any(|inside| bundle.contains(inside))
}

/// Atlas's own app bundle in `desktop`, whose processes `close` leaves
/// alone.
pub(crate) fn atlas_bundle_in(desktop: &Desktop) -> Option<String> {
    match desktop.kind() {
        Kind::User(login) => crate::user::shared::atlas_bundle(login),
    }
}

/// Whether the app `name` is installed in `desktop`'s Applications folder.
/// Apps elsewhere are not looked for.
pub fn installed(desktop: &Desktop, name: &str) -> bool {
    match desktop.kind() {
        Kind::User(login) => crate::user::shared::in_applications(login, name),
    }
}

/// Whether the app `name` is installed in the folder `applications`.
pub(crate) fn installed_in(applications: &Path, name: &str) -> bool {
    applications.join(format!("{name}.app")).is_dir()
}

/// Which running apps `close` quits.
#[derive(Debug, Clone, Copy)]
pub enum Closing<'a> {
    /// Every process of the app with this name.
    App(&'a str),
    /// Every app that is not part of macOS or of Atlas: what earlier
    /// sessions left open.
    All,
}

/// A running app's process that `close` selected.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Process {
    pub(crate) pid: u64,
    pub(crate) executable: String,
    pub(crate) name: String,
    /// Part of macOS or of Atlas, so never closed.
    pub(crate) protected: bool,
    /// The folder it must still run from when it is signalled, by the node
    /// that folder had when it was proven to be the one a cleanup owns;
    /// none for a process chosen by name.
    pub(crate) within: Option<Within>,
}

/// A folder by its path and the node (`DEVICE:INODE`) it was proven to
/// have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Within {
    pub(crate) folder: String,
    pub(crate) node: String,
}

/// The running apps `closing` selects.
pub(crate) fn selected(apps: &AppList, closing: Closing, atlas: Option<&str>) -> Vec<Process> {
    apps.rows()
        .iter()
        .filter(|running| match closing {
            Closing::App(app) => running.name().is_some_and(|name| same_text(name, app)),
            // An app someone opened: the main program of an app bundle (not
            // a helper or service inside one), with a Dock icon, or a menu
            // bar app that is not part of macOS (its own, such as Siri,
            // start with the session). Background-only processes are
            // services, not apps left open.
            Closing::All => {
                let executable = running.executable().unwrap_or("");
                let apple = running
                    .bundle_id()
                    .is_some_and(|id| id.starts_with("com.apple."));

                app_program(executable)
                    && !protected(running, atlas)
                    && match running.policy() {
                        Some("regular") => true,
                        Some("accessory") => !apple && !executable.starts_with("/System/"),
                        _ => false,
                    }
            }
        })
        .map(|running| Process {
            pid: running.pid(),
            executable: running.executable().unwrap_or("").to_owned(),
            name: running.name().unwrap_or("").to_owned(),
            protected: protected(running, atlas),
            within: None,
        })
        .collect()
}

/// The apps someone opened in `desktop` and left open, by name: what
/// `close --all` would quit.
pub(crate) fn left_open(apps: &AppList, desktop: &Desktop) -> Vec<String> {
    selected(apps, Closing::All, atlas_bundle_in(desktop).as_deref())
        .into_iter()
        .filter(|process| !process.protected)
        .map(|process| process.name)
        .collect()
}

/// The helper's `apps` reply as `atlas apps` shows it: apps with a Dock
/// icon and menu bar apps that are not part of macOS, unless `all`, with
/// `more` saying how many processes were left out.
pub fn listed(mut reply: Value, all: bool) -> Value {
    let Some(apps) = reply.get_mut("apps").and_then(Value::as_array_mut) else {
        return reply;
    };
    let before = apps.len();

    if !all {
        apps.retain(|app| {
            let executable = app["executable"].as_str().unwrap_or("");
            match app["policy"].as_str() {
                Some("regular") => true,
                Some("accessory") => {
                    !executable.is_empty()
                        && !executable.starts_with("/System/")
                        && !executable.starts_with("/usr/")
                }
                _ => false,
            }
        });
    }

    let more = before - apps.len();
    reply["more"] = json!(more);
    reply
}

#[cfg(test)]
#[path = "app_selection_tests.rs"]
mod tests;
