//! Shared application-selection policy for close-all and held logout.
use crate::ui::text::same_text;
use serde_json::{Value, json};

/// Whether a running app (as `apps` lists it) belongs to macOS or to Atlas
/// itself; neither is ever closed. Atlas is known by its bundle identifier
/// too, so a development build never closes the installed app.
fn protected(running: &Value, atlas: Option<&str>) -> bool {
    let executable = running["executable"].as_str().unwrap_or("");
    (["/System/Library/", "/usr/", "/Library/Apple/"]
        .iter()
        .any(|system| executable.starts_with(system))
        && !executable.starts_with("/usr/local/"))
        || atlas.is_some_and(|bundle| executable.starts_with(bundle))
        || running["bundleId"].as_str().is_some_and(|id| {
            id == crate::setup::IDENTIFIER
                || id
                    .strip_prefix(crate::setup::IDENTIFIER)
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

/// Atlas's own app bundle, whose processes `close` leaves alone.
pub(crate) fn atlas_bundle() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| crate::update::app_of(&exe))
        .map(|bundle| bundle.to_string_lossy().into_owned())
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
pub(crate) fn selected(apps: &Value, closing: Closing, atlas: Option<&str>) -> Vec<Process> {
    apps["apps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|running| match closing {
            Closing::App(app) => running["name"]
                .as_str()
                .is_some_and(|name| same_text(name, app)),
            // An app someone opened: the main program of an app bundle (not
            // a helper or service inside one), with a Dock icon, or a menu
            // bar app that is not part of macOS (its own, such as Siri,
            // start with the session). Background-only processes are
            // services, not apps left open.
            Closing::All => {
                let executable = running["executable"].as_str().unwrap_or("");
                let apple = running["bundleId"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("com.apple."));

                app_program(executable)
                    && !protected(running, atlas)
                    && match running["policy"].as_str() {
                        Some("regular") => true,
                        Some("accessory") => !apple && !executable.starts_with("/System/"),
                        _ => false,
                    }
            }
        })
        .filter_map(|running| {
            Some(Process {
                pid: running["pid"].as_u64()?,
                executable: running["executable"].as_str().unwrap_or("").to_owned(),
                name: running["name"].as_str().unwrap_or("").to_owned(),
                protected: protected(running, atlas),
                within: None,
            })
        })
        .collect()
}

/// The apps someone opened in the helper and left open, by name: what
/// `close --all` would quit.
pub(crate) fn left_open(apps: &Value) -> Vec<String> {
    selected(apps, Closing::All, atlas_bundle().as_deref())
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
mod tests {
    use super::*;

    #[test]
    fn apps_lists_apps_and_third_party_menu_bar_apps_unless_all() {
        let reply = json!({"ok": true, "apps": [
            {"name": "TextEdit", "pid": 1, "policy": "regular",
             "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit"},
            {"name": "Opie", "pid": 2, "policy": "accessory",
             "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
            {"name": "Siri", "pid": 3, "policy": "accessory",
             "executable": "/System/Library/CoreServices/Siri.app/Contents/MacOS/Siri"},
            {"name": "loginwindow", "pid": 4, "policy": "accessory",
             "executable": "/usr/libexec/loginwindow"},
            {"name": "cfprefsd", "pid": 5, "policy": "prohibited",
             "executable": "/usr/sbin/cfprefsd"},
        ]});

        let shown = listed(reply.clone(), false);
        let names: Vec<&str> = shown["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|app| app["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["TextEdit", "Opie"]);
        assert_eq!(shown["more"], 3);
        assert_eq!(shown["ok"], true);

        let every = listed(reply, true);
        assert_eq!(every["apps"].as_array().unwrap().len(), 5);
        assert_eq!(every["more"], 0);
    }

    #[test]
    fn a_process_that_cannot_be_placed_is_left_out_unless_all() {
        let reply = json!({"ok": true, "apps": [
            {"name": "Unnamed agent", "pid": 6, "policy": "accessory"},
            {"name": "No policy", "pid": 7,
             "executable": "/Applications/Tool.app/Contents/MacOS/Tool"},
            {"name": "Odd policy", "pid": 8, "policy": "unknown",
             "executable": "/Applications/Odd.app/Contents/MacOS/Odd"},
        ]});

        let shown = listed(reply.clone(), false);
        assert_eq!(shown["apps"], json!([]));
        assert_eq!(shown["more"], 3);

        assert_eq!(listed(reply, true)["apps"].as_array().unwrap().len(), 3);
        assert_eq!(
            listed(json!({"ok": false, "code": "apps"}), false),
            json!({"ok": false, "code": "apps"}),
            "a failed reply is passed on as it is"
        );
    }

    use serde_json::json;

    #[test]
    fn app_name_comparison_folds_case_beyond_ascii_and_matches_whole_names() {
        assert!(same_text("TextEdit", "textedit"));
        assert!(same_text("ÉDITEUR", "éditeur"));
        assert!(!same_text("TextEdit", "Text"));
    }

    #[test]
    fn selection_omits_invalid_process_ids_and_preserves_input_order() {
        let app = |pid: Value| {
            json!({
                "name": "Notes", "pid": pid, "policy": "regular",
                "executable": "/Applications/Notes.app/Contents/MacOS/Notes",
            })
        };
        let mut missing = app(Value::Null);
        missing.as_object_mut().unwrap().remove("pid");
        let apps = json!({"apps": [
            app(json!(42)),
            app(json!(-1)),
            missing,
            app(json!("7")),
            app(json!(true)),
            app(json!(1.5)),
            app(Value::Null),
            app(json!(7)),
            app(json!(u64::MAX)),
        ]});

        let processes = selected(&apps, Closing::All, None);
        assert_eq!(
            processes
                .into_iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            [42, 7, u64::MAX]
        );
    }

    /// `close --all` quits what sessions leave open, never macOS or Atlas.
    #[test]
    fn closing_everything_spares_macos_and_atlas() {
        let app = |name: &str, policy: &str, executable: &str| json!({"name": name, "pid": 10, "policy": policy, "executable": executable});
        let apps = json!({"apps": [
            app("Finder", "regular", "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"),
            app("loginwindow", "accessory", "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"),
            app("Siri AI", "accessory", "/System/Applications/Siri AI.app/Contents/MacOS/Siri AI"),
            app("universalaccessd", "prohibited", "/usr/sbin/universalaccessd"),
            app("Atlas", "accessory", "/Applications/Atlas.app/Contents/MacOS/atlas"),
            app("Helper", "prohibited", "/Applications/Other.app/Contents/MacOS/Helper"),
            app("Calendar", "regular", "/System/Applications/Calendar.app/Contents/MacOS/Calendar"),
            app("Safari", "regular", "/Applications/Safari.app/Contents/MacOS/Safari"),
            app("Opie QA", "accessory", "/Users/Shared/Opie QA.app/Contents/MacOS/Opie QA"),
        ]});

        let names = |closing| {
            selected(&apps, closing, Some("/Applications/Atlas.app"))
                .into_iter()
                .map(|process| process.name)
                .collect::<Vec<_>>()
        };

        assert_eq!(names(Closing::All), ["Calendar", "Safari", "Opie QA"]);
        let open = json!({"apps": [apps["apps"][0], apps["apps"][7], apps["apps"][8]]});
        assert_eq!(
            left_open(&open),
            ["Safari", "Opie QA"],
            "a held log-out names what close --all would quit"
        );

        let services = json!({"apps": [
            app("Raycast Web Content", "accessory", "/Users/me/com.apple.WebKit.WebContent"),
            app("Slack Helper", "accessory", "/Applications/Slack.app/Contents/Frameworks/Slack Helper.app/Contents/MacOS/Slack Helper"),
            app("Updater", "accessory", "/Applications/Opie.app/Contents/Frameworks/Sparkle.framework/Versions/B/Updater.app/Contents/MacOS/Updater"),
            app("Nameless", "regular", ""),
        ]});

        assert!(
            selected(&services, Closing::All, None).is_empty(),
            "helpers, services and apps without a program are not apps left open"
        );

        assert!(app_program("/Applications/Notes.app/Contents/MacOS/Notes"));
        assert!(app_program(
            "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator"
        ));
        assert!(!app_program("Notes.app/Contents/MacOS/Notes"));

        assert!(!protected(
            &json!({"executable": "/usr/local/Cellar/emacs/Emacs.app/Contents/MacOS/Emacs"}),
            None
        ));
        assert!(protected(
            &json!({"executable": "/usr/libexec/sharingd"}),
            None
        ));

        assert_eq!(names(Closing::App("Finder")), ["Finder"], "refused later");
        assert_eq!(names(Closing::App("opie qa")), ["Opie QA"], "any case");
        assert!(names(Closing::App("Opie")).is_empty(), "whole names only");
        assert!(selected(&apps, Closing::App("Finder"), None)[0].protected);

        // A development build, outside the app, still spares the installed
        // Atlas by its identifier.
        let installed = json!({"executable": "/Applications/Atlas.app/Contents/MacOS/atlas",
            "bundleId": "com.insanearts.atlas"});
        assert!(protected(&installed, None));
        let window = json!({"executable": "/Applications/Atlas.app/Contents/Helpers/Atlas.app/x",
            "bundleId": "com.insanearts.atlas.window"});
        assert!(protected(&window, None));
        let other = json!({"executable": "/Applications/Other.app/x",
            "bundleId": "com.insanearts.atlasish"});
        assert!(!protected(&other, None));
    }
}
