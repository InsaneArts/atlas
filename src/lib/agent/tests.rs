use super::{
    elements::*, expect::*, guide::*, input::*, launch::*, quit::*, target::*, windows::*,
};
use crate::{
    actions::{Button, Expect},
    app_selection::{Closing, Process},
    error::{Code, Error, Holder, Result},
    interrupt::Interrupt,
    native::{
        ax::{Element, Frame, Stop},
        windows::{Bounds, Dock, MENU_BAR_LAYER, MENU_LAYER, Screen, Window, owning_window},
    },
    select::{ElementFilter, NameMatch},
    session::Display,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Duration};

/// The stand-in helper's answer to a read: TextEdit (812) with one
/// window and no elements.
fn textedit_reads(request: &Value) -> Option<Value> {
    use crate::test_support::completed;

    if request["op"] == "apps" {
        return Some(json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit"}]}));
    }
    let output = match request["argv"][1].as_str() {
        Some("windows-info") => json!({"ok": true, "windows": [{"number": 5, "pid": 812,
            "owner": "TextEdit", "title": null, "layer": 0, "alpha": 1.0,
            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}}]}),
        Some("elements-info") => json!({"ok": true, "elements": [], "truncated": false}),
        other => panic!("nothing else is read: {other:?}"),
    };
    Some(completed(&output.to_string()))
}

/// An app found by name or process ID is looked for in the helper the
/// command verified moments before: the lookup and the scan after it
/// verify the helper once between them, not once for each list read.
#[tokio::test]
async fn finding_an_app_verifies_the_helper_once() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    for target in [Target::Name("TextEdit".into()), Target::Pid(812)] {
        let before = helper.proofs();
        let listed = helper
            .serve(
                elements(&client, &target, &ElementFilter::default(), 10, false),
                textedit_reads,
            )
            .await
            .unwrap();
        assert_eq!(listed["pid"], 812, "{target:?}");
        assert_eq!(helper.proofs() - before, 1, "{target:?}");
    }
}

/// A daemon that cannot be told which session a read is for is not
/// trusted to be the one verified: each read verifies it again.
#[tokio::test]
async fn an_older_daemon_is_verified_again_for_each_read() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.older();
    let target = Target::Name("TextEdit".into());
    let listed = helper
        .serve(
            elements(&client, &target, &ElementFilter::default(), 10, false),
            textedit_reads,
        )
        .await
        .unwrap();
    assert_eq!(listed["pid"], 812);
    assert_eq!(
        helper.proofs(),
        3,
        "once, then before the lookup and the scan"
    );
}

/// An app found in one helper session is never captured in another:
/// its process ID means nothing there. Even an older daemon, which
/// cannot refuse the lookup itself, gets no capture asked of it.
#[tokio::test]
async fn an_app_is_captured_only_in_the_session_it_was_found_in() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.older().moving_after(1);
    let mut captures = 0;
    let error = helper
        .serve(
            screenshot(
                &client,
                Some(&Target::Name("TextEdit".into())),
                None,
                crate::client::Size::Standard,
                false,
            ),
            |request| {
                captures += usize::from(request["op"] == "screenshot");
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "session-changed", "{error}");
    assert_eq!(captures, 0, "no capture was asked for");
}

/// A helper that logged in again after the one verification is never
/// read as the one verified: looking for the app there is refused, so
/// nothing is listed, clicked or sent.
#[tokio::test]
async fn a_helper_that_changes_after_it_is_verified_is_refused() {
    use crate::test_support::{Helper, UID};

    let filter = ElementFilter {
        role: Some("AXButton".into()),
        name: None,
    };
    let key = json!([{"type": "key", "key": "a"}]);
    // Each command meets its own helper, which moves once it has been
    // verified the one time.
    let helpers = || {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        (directory, client, helper.moving_after(1))
    };
    for target in [Target::Name("TextEdit".into()), Target::Pid(812)] {
        let (_listing, client, helper) = helpers();
        let listed = helper
            .serve(
                elements(&client, &target, &filter, 10, false),
                textedit_reads,
            )
            .await;
        let (_clicking, clicker, helper) = helpers();
        let clicked = helper
            .serve(
                click_element(&clicker, &target, &filter, Button::Left, 1, &[]),
                textedit_reads,
            )
            .await;
        let (_sending, sender, helper) = helpers();
        let sent = helper
            .serve(
                act(
                    &sender,
                    Expected::new(Some(target.clone()), None, None)
                        .unwrap()
                        .as_ref(),
                    &key,
                ),
                textedit_reads,
            )
            .await;
        for outcome in [listed, clicked, sent] {
            let error = outcome.unwrap_err();
            assert_eq!(error.code, "session-changed", "{target:?}: {error}");
            assert!(!error.uncertain);
        }
        for client in [client, clicker, sender] {
            assert!(client.controller.pending().unwrap().is_empty());
        }
    }
}

/// Only a check of a point pays for reading the Dock: listing windows
/// and waiting for them do not ask for it.
#[test]
fn only_a_point_check_asks_the_window_probe_for_the_dock() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let skip = windows_probe(&client, DockRead::Skip);
    let read = windows_probe(&client, DockRead::Read);
    assert_eq!(skip[1..], ["windows-info"]);
    assert_eq!(read[1..], ["windows-info", "--dock"]);
}

/// A named click refused under the Dock says it is the Dock, and when
/// the Dock could not be read, that Atlas could not tell.
#[test]
fn a_named_click_under_the_dock_says_it_is_the_dock() {
    let dock = Window {
        number: 1,
        pid: 2,
        owner: Some("Dock".into()),
        title: None,
        layer: 20,
        alpha: 1.0,
        bounds: Bounds::display(1000.0, 800.0),
    };
    let app = Window {
        number: 3,
        owner: Some("TextEdit".into()),
        layer: 0,
        pid: 4,
        ..dock.clone()
    };
    let windows = [dock, app];
    let display = Bounds::display(1000.0, 800.0);
    let icons = Dock::Icons(vec![Bounds {
        x: 200.0,
        y: 740.0,
        width: 600.0,
        height: 60.0,
    }]);
    let said =
        |dock: Dock, y| hidden_element(&windows, 300.0, y, &Screen::new(display.clone(), dock));

    assert!(said(icons.clone(), 770.0).starts_with("The Dock covers"));
    assert!(said(Dock::Other, 300.0).starts_with("The Dock covers"));
    assert!(said(Dock::Unread, 300.0).contains("could not read where the Dock takes clicks"));
    assert!(said(icons, 300.0).starts_with("The element is not visible"));
}

/// The process a name resolves to from the window list alone.
fn app_pid(windows: &[Window], target: &Target) -> Result<i64> {
    match target {
        Target::Pid(pid) => Ok(*pid),
        Target::Name(name) => one_pid(name, &app_pids(windows, name)),
    }
}

/// An `apps` row is found by its shown name or its bundle's file name.
#[test]
fn an_app_row_is_named_by_its_shown_or_file_name() {
    let row = json!({
        "name": "Éditeur de texte",
        "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    });
    assert!(app_named(&row, "éditeur de texte"));
    assert!(app_named(&row, "TextEdit"));
    assert!(
        !app_named(&row, "Contents"),
        "a folder on the way is not a name"
    );
    assert!(!app_named(&json!({"name": "Finder"}), "TextEdit"));

    let nested = json!({
        "name": "Simulator",
        "executable": "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator",
    });

    assert!(
        !app_named(&nested, "Xcode"),
        "the bundle it lives in is not its name"
    );
}

fn display() -> Display {
    Display {
        width: 2560.0,
        height: 1440.0,
        scale: 1.0,
    }
}

fn window(number: i64, pid: i64, owner: &str, layer: i64) -> Window {
    Window {
        number,
        pid,
        owner: Some(owner.into()),
        title: None,
        layer,
        alpha: 1.0,
        bounds: Bounds {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        },
    }
}

#[test]
fn window_and_app_naming_different_apps_is_a_usage_error() {
    let document = window(4120, 812, "TextEdit", 0);
    let calculator = Target::Name("Calculator".into());
    let error = other_app(&document, &calculator);
    assert_eq!(error.code, "usage");
    assert!(
        error
            .error
            .starts_with("Window 4120 is TextEdit's, not Calculator's"),
        "{}",
        error.error
    );
    assert!(
        other_app(&document, &Target::Pid(900))
            .error
            .contains("not process 900's"),
    );

    let menu = window(4121, 812, "TextEdit", MENU_LAYER);
    let listed = [menu, document];
    assert_eq!(window_owner(&listed, 4120).unwrap().pid, 812);
    for missing in [4121, 9999] {
        assert_eq!(
            window_owner(&listed, missing).unwrap_err().code,
            "windows",
            "{missing}: an open menu is not a window to type into"
        );
    }
}

/// A helper showing a TextEdit document and Calculator, counting the
/// window lists it gives.
fn two_apps(lists: &std::cell::Cell<usize>) -> impl FnMut(&Value) -> Option<Value> + '_ {
    move |request| {
        use crate::test_support::completed;

        if request["op"] == "apps" {
            return Some(json!({"ok": true, "apps": [
                {"pid": 812, "name": "TextEdit"}, {"pid": 900, "name": "Calculator"}]}));
        }
        assert_eq!(request["argv"][1], "windows-info", "nothing else is read");
        lists.set(lists.get() + 1);
        let window = |number, pid, owner| {
            json!({"number": number, "pid": pid,
            "owner": owner, "title": null, "layer": 0, "alpha": 1.0,
            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}})
        };
        let listed = json!({"ok": true, "windows": [
            window(4120, 812, "TextEdit"), window(339, 900, "Calculator")]});
        Some(completed(&listed.to_string()))
    }
}

/// What `expected` asks the helper to check, found in a helper showing
/// [`two_apps`], with how many window lists it read.
async fn resolved(expected: Option<Expected>) -> (Result<Expect>, usize) {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let lists = std::cell::Cell::new(0);
    let expected = expected.unwrap();
    let outcome = helper
        .serve(
            async {
                let verified = client.verify().await?;
                expected.resolve(&client, &verified).await
            },
            two_apps(&lists),
        )
        .await;
    (outcome, lists.get())
}

#[tokio::test]
async fn an_expectation_is_resolved_in_the_helper_with_one_window_list() {
    let textedit = || Some(Target::Name("TextEdit".into()));
    let field = || Some("Email".to_owned());

    let (alone, lists) = resolved(Expected::new(None, Some(4120), field()).unwrap()).await;
    assert_eq!(
        serde_json::to_value(alone.unwrap()).unwrap(),
        json!({"pid": 812, "window": 4120, "field": "Email"}),
        "the window names its app"
    );
    assert_eq!(lists, 1);

    let (both, lists) = resolved(Expected::new(textedit(), Some(4120), None).unwrap()).await;
    assert_eq!(both.unwrap().pid(), 812);
    assert_eq!(lists, 1, "the app is found in the same list");

    let calculator = Some(Target::Name("Calculator".into()));
    let (other, _) = resolved(Expected::new(calculator, Some(4120), None).unwrap()).await;
    assert_eq!(other.unwrap_err().code, "usage");

    let (by_pid, _) =
        resolved(Expected::new(Some(Target::Pid(900)), Some(4120), None).unwrap()).await;
    assert_eq!(by_pid.unwrap_err().code, "usage");

    let (missing, _) = resolved(Expected::new(textedit(), Some(77), None).unwrap()).await;
    assert_eq!(missing.unwrap_err().code, "windows");

    let (app, _) = resolved(Expected::new(textedit(), None, field()).unwrap()).await;
    assert_eq!(
        serde_json::to_value(app.unwrap()).unwrap(),
        json!({"pid": 812, "field": "Email"})
    );
}

#[test]
fn a_field_needs_the_app_or_window_it_is_in() {
    let field = || Some("Email".to_owned());
    assert_eq!(
        Expected::new(None, None, field()).unwrap_err().code,
        "usage"
    );
    assert!(Expected::new(None, None, None).unwrap().is_none());
    assert!(Expected::new(None, Some(4120), field()).unwrap().is_some());
    assert!(
        Expected::new(Some(Target::Name("TextEdit".into())), None, field())
            .unwrap()
            .is_some()
    );
}

/// A menu bar title is judged against the menu bar, the Window Server's
/// window at the point to click, and only while that is shown; an app's
/// own window at that layer is not the bar.
#[test]
fn the_menu_bar_is_the_window_servers() {
    let bar = |owner: &str, alpha: f64| {
        let mut bar = window(3, 433, owner, MENU_BAR_LAYER);
        bar.alpha = alpha;
        bar.bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width: 1512.0,
            height: 30.0,
        };

        bar
    };
    let app = || window(9, 6, "Opie", 0);

    assert_eq!(
        menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 12.0),
        Some(3)
    );
    assert_eq!(
        menu_bar_window(&[bar("Window Server", 1.0), app()], 232.0, 60.0),
        None
    );
    assert_eq!(
        menu_bar_window(&[bar("Window Server", 0.0), app()], 232.0, 12.0),
        None
    );
    assert_eq!(
        menu_bar_window(&[bar("Opie", 1.0), app()], 232.0, 12.0),
        None
    );
}

#[test]
fn the_guide_names_only_real_commands() {
    for command in [
        "doctor",
        "readiness",
        "screenshot",
        "windows",
        "elements",
        "click",
        "type",
        "key",
        "drag",
        "scroll",
        "open",
        "wait",
        "run",
        "restart",
        "close",
        "cleanup",
        "activity",
        "install-app",
        "environment list",
    ] {
        assert!(GUIDE.contains(&format!("atlas {command}")), "{command}");
    }

    for command in [
        "doctor",
        "windows",
        "screenshot",
        "elements",
        "click",
        "wait",
        "open",
        "run",
        "restart",
        "close",
        "cleanup",
        "activity",
        "monitor",
    ] {
        assert!(CORE.contains(&format!("atlas {command}")), "{command}");
    }
}

/// The core with its lines joined, so a phrase may wrap anywhere.
fn core_text() -> String {
    CORE.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where the skill's references live, beside its SKILL.md.
fn references_folder() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/atlas/skills/atlas/references")
}

#[test]
fn the_skill_is_the_core_followed_by_its_references() {
    let skill = skill();
    let (_, body) = skill.split_once("\n---\n\n# Atlas\n\n").unwrap();
    let (_, core) = CORE.split_once('\n').unwrap();
    let rest = body.strip_prefix(core.trim_start()).unwrap();
    let mut expected = String::from("\n## References\n\nRead one when the task needs it:\n\n");
    for (file, about) in REFERENCES {
        expected.push_str(&format!(
            "- [references/{file}](references/{file}): {about}\n"
        ));
    }
    assert_eq!(rest, expected);
}

/// Every agent loads the whole skill at the start of each task. When it
/// grows past this, move detail into a reference rather than raise it.
#[test]
fn the_skill_stays_near_two_thousand_tokens() {
    let size = skill().len();
    assert!(size <= 8 * 1024, "the skill is {size} bytes");
}

#[test]
fn the_skill_core_keeps_every_rule_an_agent_needs_from_the_start() {
    let core = core_text();
    let actor = ["ATLAS", "ACTOR"].join("_");
    for rule in [
        "busy",
        "holder",
        "stopped",
        "\"uncertain\": true",
        "Do not run `atlas restart`",
        "on-console",
        "blocked",
        "session-changed",
        actor.as_str(),
        "privacy settings",
        "credentials",
        "atlas switch",
        "atlas logout",
        "Never repeat",
        "each app you opened",
        "unsaved work",
        "without a screenshot that shows it",
        "show_screen",
        "&&",
        "unchanged: true",
        "focus-changed",
        "Do what `next` says, then send only what was not.",
        "--app A",
        "`atlas click --app A --window N X Y`",
        "--wait",
        "`waited` says how long Atlas waited",
        "if `heldBy` is you, wait for your own command",
        "your shell tool's limit",
        "never end your turn with an Atlas command still running",
        "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours.",
        "yours: true",
        "Touch only windows you opened",
        "`unread: withheld`",
        "ask the person to unlock",
        "`ATLAS_SESSION=NAME` before every `atlas` command",
        "put `ATLAS_TASK=NAME` on every command too, cleanup included",
        "atlas activity",
        "atlas job start",
        "atlas collect",
        "check it, then `atlas cleanup --apply` stops its jobs",
    ] {
        assert!(core.contains(rule), "{rule}");
    }
}

/// Spans in backticks in the full guide and the error reference.
fn shown_spans() -> impl Iterator<Item = &'static str> {
    [GUIDE, ERRORS]
        .into_iter()
        .flat_map(|text| text.split('`').skip(1).step_by(2))
}

/// Between two of an agent's commands another may bring its own window
/// forward, so every keyboard or pointer example names the app it is for,
/// and every save or close names the window too, unless it is Finder's
/// or a sheet's.
#[test]
fn every_keyboard_and_pointer_example_the_guide_shows_names_its_app() {
    let examples: Vec<&str> = shown_spans()
        .filter(|span| {
            let mut words = span.split_whitespace();
            let command = matches!(
                (words.next(), words.next()),
                (
                    Some("atlas"),
                    Some("type" | "key" | "scroll" | "drag" | "click")
                )
            );
            // A command named in prose, with at most its flags, shows
            // no input.
            command && words.any(|word| !word.starts_with('-'))
        })
        .collect();
    assert!(examples.len() >= 10, "{examples:?}");
    for span in &examples {
        assert!(span.contains(" --app "), "`{span}` names no app");
        if span.contains("cmd+s") || span.contains("cmd+w") {
            assert!(span.contains(" --window N "), "`{span}` names no window");
        }
    }

    // A key named on its own (`key escape`) is an example too.
    for span in shown_spans() {
        assert!(!span.starts_with("key "), "`{span}` names no app");
    }
}

/// Every code the core gives its own bullet is one Atlas reports.
#[test]
fn every_code_the_core_bullets_is_a_real_code() {
    let (_, section) = CORE.split_once("## When something goes wrong").unwrap();
    let (section, _) = section.split_once("\n## ").unwrap();
    let mut codes = Vec::new();
    for bullet in section.lines().filter(|line| line.starts_with("- ")) {
        // Up to the first colon outside backticks.
        let mut quoted = false;
        let end = bullet
            .char_indices()
            .find(|&(_, c)| {
                quoted ^= c == '`';
                c == ':' && !quoted
            })
            .map_or(bullet.len(), |(index, _)| index);
        let head = &bullet[..end];
        codes.extend(
            head.split('`')
                .skip(1)
                .step_by(2)
                .filter(|span| span.chars().all(|c| c.is_ascii_lowercase() || c == '-')),
        );
    }
    for code in ["uncertain", "busy", "focus-changed", "stopped", "cancelled"] {
        assert!(codes.contains(&code), "{code}: {codes:?}");
    }
    for code in codes {
        assert!(crate::error::CODES.contains(&code), "{code}");
    }
}

/// What an agent sharing the helper reads only when it needs it: how
/// long Atlas waits, each host's time limit, and what not to touch.
#[test]
fn the_limits_say_how_long_to_wait_and_how_to_work_beside_another_agent() {
    let guide = GUIDE.split_whitespace().collect::<Vec<_>>().join(" ");
    for section in ["Time limits", "Working beside another agent"] {
        assert!(GUIDE.contains(&format!("\n### {section}\n")), "{section}");
    }
    for said in [
        "`ATLAS_WAIT`",
        "128 plus the signal's number",
        "a command that finished in time prints its ordinary result",
        "a longer `--wait` does not change that",
        "about 10 seconds to stop what is left",
        "Never end your turn with an Atlas command still running.",
        "`--app` checks the app, not the window.",
        "`click --name` takes no `--window`",
        "Your window is the one `atlas windows` did not list before you opened it.",
        "`causedBy`",
        "An `uncertain` you did not cause is not yours to recover",
        "Never restart over `yours: false`",
    ] {
        assert!(guide.contains(said), "{said}");
    }
}

/// A cleanup with no task named covers every task of the session, so
/// the guide never calls it this task's.
#[test]
fn a_cleanup_with_no_task_is_said_to_cover_the_whole_session() {
    let guide = GUIDE.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(core_text().contains(
        "`atlas cleanup` shows what else this session left (with `ATLAS_TASK`, that task's alone)"
    ));
    assert!(guide.contains("A plain `atlas cleanup` covers the whole session"));
    assert!(guide.contains("keep the same `ATLAS_TASK` through cleanup"));
    for wrong in [
        "cleanup` shows what else this task",
        "cleanup` shows everything this task",
    ] {
        assert!(!guide.contains(wrong), "{wrong}");
    }
}

/// The hosts whose time limits the guide gives are the ones the public
/// connection guide lists, and the other way round.
#[test]
fn the_guide_and_the_connection_docs_list_the_same_host_limits() {
    let (_, limits) = GUIDE.split_once("\n### Time limits\n").unwrap();
    let (limits, _) = limits.split_once("\n### ").unwrap();
    let mut in_guide: Vec<&str> = limits
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| line.split_once(':').map(|(host, _)| host))
        .collect();
    in_guide.sort_unstable();

    let docs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/agents.md"),
    )
    .unwrap();
    let (_, table) = docs
        .split_once("## Several agents and time limits")
        .unwrap();
    let (table, _) = table.split_once("\n## ").unwrap();
    let mut in_docs: Vec<&str> = table
        .lines()
        .filter_map(|line| line.strip_prefix('|'))
        .filter_map(|line| line.split('|').next())
        .map(str::trim)
        .filter(|host| !host.is_empty() && *host != "Host" && !host.starts_with('-'))
        .collect();
    in_docs.sort_unstable();

    assert_eq!(in_guide, in_docs);
    assert_eq!(in_guide.len(), 5, "{in_guide:?}");
}

#[test]
fn no_step_quits_or_discards_what_the_task_did_not_open() {
    let core = core_text();
    for (index, _) in core.match_indices("close --all") {
        assert!(
            core[..index].ends_with("never run `atlas "),
            "close --all is taught: {}",
            &core[index.saturating_sub(60)..index]
        );
    }
    assert!(!core.to_lowercase().contains("cmd+backspace"));
}

#[test]
fn the_full_guide_is_the_core_then_every_reference_written_for_it() {
    let mut expected = format!("{CORE}\n");
    for file in ["testing.md", "environment.md", "limits.md"] {
        assert!(
            REFERENCES.iter().any(|(listed, _)| *listed == file),
            "{file}"
        );
        let text = std::fs::read_to_string(references_folder().join(file)).unwrap();
        assert!(text.starts_with("## "), "{file} starts with its heading");
        expected.push_str(&text);
        expected.push('\n');
    }
    let rest = GUIDE.strip_prefix(&expected).unwrap();
    assert!(rest.starts_with("## References\n"), "{rest}");
    assert!(rest.contains("`atlas agent-guide --commands`"), "{rest}");
    assert!(rest.contains("`atlas agent-guide --errors`"), "{rest}");
}

#[test]
fn every_reference_the_skill_lists_is_in_its_folder_and_nothing_else_is() {
    let mut present: Vec<String> = std::fs::read_dir(references_folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    present.sort();
    let mut listed: Vec<String> = REFERENCES
        .iter()
        .map(|(file, _)| (*file).to_owned())
        .collect();
    listed.sort();
    assert_eq!(present, listed);
}

/// The core sends an agent to these sections by name: each heads a part
/// of the full guide and is how the skill's References list names it.
#[test]
fn every_section_the_core_points_to_exists() {
    let core = core_text();
    for section in ["Test your own app", "Give the app what it needs", "Limits"] {
        assert!(core.contains(&format!("read {section}")), "{section}");
        assert!(GUIDE.contains(&format!("\n## {section}\n")), "{section}");
        assert!(
            REFERENCES
                .iter()
                .any(|(_, about)| about.starts_with(section)),
            "{section}"
        );
    }
}

#[test]
fn agents_are_told_to_read_both_output_streams_on_failure() {
    for text in [GUIDE.to_owned(), skill()] {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("exit non-zero on failure; then read both stdout and stderr."));
    }
}

/// Agents read the skill's description from YAML front matter: it stays
/// one double-quoted line with nothing that would end or break it.
#[test]
fn the_skill_description_is_valid_front_matter() {
    assert!(!SKILL_DESCRIPTION.contains(['"', '\\', '\n']));
    assert!(skill().starts_with(&format!(
        "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n"
    )));
}

#[test]
fn sugar_produces_valid_actions() {
    assert_eq!(click(1.0, 2.0, Button::Right, 1, &[])[0]["button"], "right");
    assert_eq!(
        drag((1.0, 2.0), (3.0, 4.0), Button::Middle)[0]["button"],
        "middle"
    );

    for actions in [
        click(
            10.0,
            20.0,
            Button::Right,
            2,
            &["cmd".into(), "shift".into()],
        ),
        drag((1.0, 2.0), (300.0, 400.0), Button::Left),
        keys(&["cmd+s".into(), "return".into()]).unwrap(),
    ] {
        crate::actions::validate(&actions, &display()).unwrap();
    }
}

#[test]
fn typing_splits_long_text_on_grapheme_boundaries() {
    let family = "👨‍👩‍👧‍👦";
    let text = format!("{}{family}{}", "a".repeat(250), "b".repeat(300));
    let batches = type_batches(&text).unwrap();

    assert_eq!(batches.concat(), text);
    assert!(
        batches.iter().all(|t| t.chars().count() <= TYPE_BATCH),
        "each part is short enough to finish in time on a busy Mac"
    );
    assert!(
        batches.iter().any(|t| t.contains(family)),
        "cluster kept whole"
    );

    for batch in &batches {
        crate::actions::validate(&json!([{ "type": "type", "text": batch }]), &display()).unwrap();
    }

    let decomposed = "e\u{301}".repeat(200);
    let batches = type_batches(&decomposed).unwrap();
    assert!(batches.iter().all(|t| !t.starts_with('\u{301}')));
    assert_eq!(batches.concat(), decomposed);

    assert!(type_batches("").is_err());

    let long_cluster = format!("a{}", "\u{301}".repeat(99));
    let batches = type_batches(&format!("xy{long_cluster}z")).unwrap();
    assert_eq!(
        batches,
        ["xy", long_cluster.as_str(), "z"],
        "a long cluster goes alone"
    );
}

#[test]
fn text_is_typed_in_parts_of_at_most_sixteen_characters() {
    let plain = "x".repeat(64);
    let parts = type_parts(&plain);
    assert_eq!(parts.len(), 4);
    assert!(parts.iter().all(|part| part.chars().count() == TYPE_PART));

    let family = "👨‍👩‍👧‍👦";
    let text = format!("{}{family}{}", "a".repeat(14), "b".repeat(20));
    let parts = type_parts(&text);
    assert_eq!(parts.concat(), text);
    assert!(parts.iter().any(|part| part.contains(family)), "kept whole");
    assert!(parts.iter().all(|part| part.chars().count() <= TYPE_PART));

    let decomposed = "e\u{301}".repeat(20);
    let parts = type_parts(&decomposed);
    assert!(parts.iter().all(|part| !part.starts_with('\u{301}')));
    assert_eq!(parts.concat(), decomposed);

    let long_cluster = format!("a{}", "\u{301}".repeat(30));
    assert_eq!(
        type_parts(&format!("xy{long_cluster}z")),
        ["xy", long_cluster.as_str(), "z"],
        "a cluster longer than a part goes alone"
    );

    for batch in type_batches(&"y".repeat(200)).unwrap() {
        let parts: Vec<Value> = type_parts(&batch)
            .iter()
            .map(|part| json!({"type": "type", "text": part}))
            .collect();
        assert!(parts.len() <= TYPE_BATCH / TYPE_PART);
        crate::actions::validate(&Value::Array(parts), &display()).unwrap();
    }
}

#[test]
fn a_stopped_type_counts_the_parts_the_helper_performed() {
    let parts = type_parts(&"z".repeat(64));
    let refused = json!({"ok": false, "code": "focus-changed", "reason": "app",
        "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
        "performed": 2, "partial": false, "uncertain": false, "cleanupError": "kept out"});
    let reply = |value: &Value| Unconfirmed::from_reply(value);

    let report = stopped_typing(64, 3, 1, &parts, reply(&refused));
    assert_eq!(report["typed"], 64 + 32);
    assert_eq!(report["batches"], 3);
    assert_eq!(report["confirmedBatches"], 1);
    assert_eq!(report["partial"], true);
    assert_eq!(report["uncertain"], false);
    assert_eq!(report["code"], "focus-changed");
    assert_eq!(report["error"], "Calculator has keyboard focus");
    assert_eq!(report["reason"], "app");
    assert_eq!(report["found"], refused["found"]);
    assert!(
        report.get("cleanupError").is_none(),
        "only what agents act on"
    );

    let first = stopped_typing(
        0,
        1,
        0,
        &parts,
        reply(&json!({"performed": 0, "code": "focus-changed"})),
    );
    assert_eq!(
        (first["typed"].clone(), first["partial"].clone()),
        (json!(0), json!(false))
    );
    let broke = reply(&json!({"performed": 0, "partial": true, "uncertain": true}));
    assert_eq!(stopped_typing(0, 1, 0, &parts, broke)["partial"], true);

    let left = json!({"ok": false, "code": "focus-changed", "reason": "field",
        "found": {"app": "TextEdit", "pid": 812, "window": 4120,
            "field": {"role": "AXTextField", "name": "Second"}},
        "error": "The field with keyboard focus is not the one named First",
        "performed": 0, "partial": false, "uncertain": false});
    let report = stopped_typing(128, 4, 2, &parts, reply(&left));
    assert_eq!(
        (
            report["typed"].clone(),
            report["confirmedBatches"].clone(),
            report["partial"].clone(),
            report["uncertain"].clone()
        ),
        (json!(128), json!(2), json!(true), json!(false)),
        "a field left after two batches: those sent, exactly"
    );
    assert_eq!(report["reason"], "field");
    assert_eq!(report["found"], left["found"]);

    let lost = Error::new(Code::Transport, "Native reply timed out").uncertain();
    let report = stopped_typing(64, 2, 1, &parts, lost.into());
    assert_eq!(report["typed"], 64, "no part of a failed batch counts");
    assert_eq!(report["code"], "transport");
    assert_eq!(report["uncertain"], true);
    assert!(report.get("reason").is_none() && report.get("found").is_none());

    let busy = Error::new(Code::Busy, "held").held_by(Holder::Agent);
    assert_eq!(
        stopped_typing(0, 1, 0, &parts, busy.into())["holder"],
        "agent"
    );
    let busy = reply(&json!({"code": "busy", "holder": "person"}));
    assert_eq!(stopped_typing(0, 1, 0, &parts, busy)["holder"], "person");

    let unknown = stopped_typing(0, 1, 0, &parts, reply(&json!({"code": "made-up"})));
    assert_eq!(unknown["code"], "input");
    assert_eq!(unknown["error"], "The helper did not confirm the text");
}

#[tokio::test]
async fn typing_records_the_whole_commands_outcome_after_its_batches() {
    use crate::{activity_log, doing::Doing};

    let batches = type_batches(&"private typed text ".repeat(5)).unwrap();
    assert_eq!(batches.len(), 2);
    for (completed, performed, partial, uncertain) in [
        (0, 0, false, false),
        (0, 1, false, false),
        (0, 0, true, false),
        (1, 0, false, false),
        (1, 1, false, false),
        (1, 0, false, true),
        (2, 0, false, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let client = crate::test_support::client(directory.path()).doing(
            Doing::new("type")
                .app(Some("TextEdit"))
                .field(Some("Email")),
        );
        let interrupt = Interrupt::new(Duration::from_secs(10));
        let mut sent = 0;
        let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
            let reply = if sent < completed {
                json!({"ok": true, "performed": actions.as_array().unwrap().len(),
                    "app": {"name": "TextEdit", "bundleId": "com.apple.TextEdit"},
                    "field": {"role": "AXTextField"}})
            } else {
                json!({"ok": false, "code": "focus-changed", "reason": "field",
                    "performed": performed, "partial": partial, "uncertain": uncertain,
                    "error": "private failure message"})
            };
            client
                .record(
                    &json!({"op": "input", "actions": actions}),
                    &reply,
                    Duration::ZERO,
                )
                .await?;
            sent += 1;
            Ok(reply)
        })
        .await;

        let rows = activity_log::recent(client.output_dir().unwrap(), 5);
        assert_eq!(rows.len(), 1, "the terminal row replaces every batch");
        let row = &rows[0];
        assert_eq!(
            row.action_count,
            Some(6),
            "all requested type actions count"
        );
        assert_eq!(row.worked(), completed == 2);
        let shown = crate::overview::shown(row, 0);
        if completed > 0 {
            assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
        }
        if completed < 2 && !uncertain {
            let delivered = completed > 0 || performed > 0 || partial;
            assert_eq!(row.partial, Some(delivered));
            assert_eq!(shown.nothing_sent, !delivered);
            assert_eq!(row.reason.as_deref(), Some("field"));
        }
        if uncertain {
            assert!(shown.uncertain);
            assert!(!shown.nothing_sent);
        }
        if completed == 2 {
            assert_eq!(row.bundle_id.as_deref(), Some("com.apple.TextEdit"));
            assert_eq!(row.label.as_deref(), Some("Email"));
            assert_eq!(report["typed"], 95);
        } else {
            assert!(row.label.is_none(), "a refused field is never named");
        }
        let log =
            std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
        assert!(!log.contains("private typed text"));
        assert!(!log.contains("private failure message"));
    }
}

#[tokio::test]
async fn typing_without_a_command_context_keeps_only_its_batch_rows() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(100)).unwrap();
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        let reply = json!({"ok": true, "performed": actions.as_array().unwrap().len()});
        client
            .record(
                &json!({"op": "input", "actions": actions}),
                &reply,
                Duration::ZERO,
            )
            .await?;
        Ok(reply)
    })
    .await;
    assert_eq!(report["typed"], 100);
    let rows = crate::activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.command.is_none() && row.worked()));
}

#[tokio::test]
async fn typing_records_a_stop_between_batches_after_confirmed_text() {
    use crate::{activity_log, doing::Doing};
    use nix::sys::signal::Signal;

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path()).doing(Doing::new("type"));
    let interrupt = Interrupt::new(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(100)).unwrap();
    let mut sent = 0;
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        let reply = json!({"ok": true, "performed": actions.as_array().unwrap().len()});
        client
            .record(
                &json!({"op": "input", "actions": actions}),
                &reply,
                Duration::ZERO,
            )
            .await?;
        sent += 1;
        interrupt.ask(Signal::SIGTERM);
        Ok(reply)
    })
    .await;
    assert_eq!(sent, 1);
    assert_eq!(report["typed"], 64);
    let rows = activity_log::recent(client.output_dir().unwrap(), 5);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].coded(Code::Interrupted));
    assert!(rows[0].failed());
}

#[tokio::test]
async fn typing_stops_between_parts_and_says_how_much_was_typed() {
    use nix::sys::signal::Signal;

    let interrupt = Interrupt::new(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(200)).unwrap();
    assert_eq!(batches.len(), 4);
    let mut sent = 0;

    let report = type_each(&batches, &interrupt, async |actions: &Value| {
        sent += 1;
        interrupt.ask(Signal::SIGTERM);
        Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
    })
    .await;
    assert_eq!(sent, 1, "nothing more is sent once a stop was asked");
    assert_eq!(report["ok"], false);
    assert_eq!(report["code"], "interrupted");
    assert_eq!(report["uncertain"], false);
    assert_eq!(report["typed"], 64);
    assert_eq!(report["batches"], 4);
    assert_eq!(report["confirmedBatches"], 1);

    let interrupt = Interrupt::new(Duration::from_secs(10));
    let mut first = true;
    let report = type_each(&batches, &interrupt, async |actions: &Value| {
        if std::mem::take(&mut first) {
            Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
        } else {
            Err(interrupt.cut_short(Signal::SIGTERM))
        }
    })
    .await;
    assert_eq!(report["code"], "interrupted");
    assert_eq!(report["uncertain"], true, "the part under way was cut");
    assert_eq!(report["typed"], 64, "only confirmed parts count");
}

/// A look never asks the helper for longer than it allows a command,
/// however long the wait.
#[test]
fn a_look_stays_within_the_helpers_limit() {
    assert_eq!(probe_budget(Duration::ZERO), Duration::from_millis(500));
    assert_eq!(probe_budget(Duration::from_secs(9)), Duration::from_secs(9));
    assert_eq!(
        probe_budget(Duration::from_secs(86_400)),
        crate::client::RUN_TIMEOUT_MAX
    );

    let longest = crate::client::RUN_TIMEOUT_MAX.as_millis();
    let request = json!({"op": "run", "argv": ["x"], "timeoutMs": longest}).to_string();
    assert!(
        crate::daemon::protocol::parse(request.as_bytes()).is_ok(),
        "the helper accepts the longest look"
    );
}

/// Every command compares app names the same way, beyond ASCII too.
#[test]
fn app_names_match_in_any_case() {
    let windows = [window(1, 8, "Éditeur", 0)];
    assert_eq!(
        app_pid(&windows, &Target::Name("éditeur".into())).unwrap(),
        8
    );

    let filter = WindowFilter {
        app: Some("ÉDITEUR".into()),
        ..WindowFilter::default()
    };
    assert!(filter.matches(&windows[0]));
}

#[test]
fn window_filters_match_app_title_and_layer() {
    let mut edit = window(1, 2, "TextEdit", 0);
    edit.title = Some("Untitled — Edited".into());
    let mut menu = window(3, 4, "Window Server", 24);
    menu.title = Some("Menubar".into());

    let by_app = WindowFilter {
        app: Some("textedit".into()),
        ..Default::default()
    };
    assert!(by_app.matches(&edit));

    let by_title = WindowFilter {
        title: Some("untitled".into()),
        ..Default::default()
    };
    assert!(by_title.matches(&edit));
    assert!(!by_title.matches(&window(5, 6, "Preview", 0)));

    assert!(!WindowFilter::default().matches(&menu));
    assert!(
        WindowFilter {
            all: true,
            ..Default::default()
        }
        .matches(&menu)
    );

    let alert = window(7, 8, "CoreServicesUIAgent", 8);
    assert!(
        WindowFilter::default().matches(&alert),
        "an alert is listed"
    );
    assert!(
        !WindowFilter::default().matches(&window(9, 10, "Dock", 20)),
        "the Dock is not"
    );
}

fn element(role: &str, title: Option<&str>, description: Option<&str>) -> Element {
    Element {
        index: 0,
        depth: 1,
        window: None,
        role: Some(role.into()),
        subrole: None,
        title: title.map(Into::into),
        description: description.map(Into::into),
        value: None,
        identifier: None,
        enabled: Some(true),
        focused: None,
        selected: None,
        frame: None,
        unreadable: Vec::new(),
        truncated: Vec::new(),
        menu: false,
        mark: None,
    }
}

#[test]
fn element_filters_match_role_and_exact_name() {
    let save = element("AXButton", Some("Save"), None);
    let seven = element("AXButton", None, Some("7"));
    let label = element("AXStaticText", Some("Save"), None);
    let filter = |role: Option<&str>, name: Option<&str>| ElementFilter {
        role: role.map(Into::into),
        name: name.map(Into::into),
    };

    assert!(filter(Some("axbutton"), Some("save")).matches(&save));
    assert!(!filter(Some("AXButton"), Some("Save")).matches(&label));
    assert!(
        filter(None, Some("7")).matches(&seven),
        "description counts"
    );
    assert!(!filter(None, Some("Sav")).matches(&save), "names are exact");
    assert!(filter(None, None).matches(&label));

    let mut unread = element("AXButton", None, None);
    unread.unreadable = vec!["AXTitle".into()];
    assert!(!filter(Some("AXButton"), Some("Save")).matches(&unread));
    assert!(filter(Some("AXButton"), Some("Save")).might_match(&unread));
    assert!(!filter(Some("AXStaticText"), Some("Save")).might_match(&unread));
}

#[test]
fn a_label_is_named_by_its_text_but_a_field_is_not() {
    let filter = ElementFilter {
        role: None,
        name: Some("Main".into()),
    };
    let mut label = element("AXStaticText", None, None);
    label.value = Some("Main".into());
    assert!(filter.matches(&label) && filter.might_match(&label));

    let mut field = element("AXTextField", None, None);
    field.value = Some("Main".into());
    assert!(!filter.matches(&field), "typed text is not a name");
}

#[test]
fn a_named_click_reports_only_role_name_and_point() {
    let mut button = element("AXButton", Some("Save"), Some("Save the file"));
    button.identifier = Some("save".into());
    assert_eq!(
        clicked(&button, (25.0, 25.0)),
        json!({"at": [25, 25], "name": "Save", "role": "AXButton"})
    );

    let mut label = element("AXStaticText", Some(""), None);
    label.value = Some("Main".into());
    assert_eq!(
        clicked(&label, (3.0, 4.0)),
        json!({"at": [3, 4], "name": "Main", "role": "AXStaticText"})
    );

    let mut field = element("AXTextField", None, None);
    field.value = Some("typed".into());
    assert_eq!(
        clicked(&field, (3.0, 4.0)),
        json!({"at": [3, 4], "role": "AXTextField"})
    );
}

/// A label that joins several parts (SwiftUI's combined rows) is
/// named by its first part too, but a whole name is the better match.
#[test]
fn a_combined_label_is_named_by_its_first_part() {
    let filter = ElementFilter {
        role: None,
        name: Some("qa eject".into()),
    };
    let row = element("AXButton", None, Some("QA Eject, Option-Space, 6 Slices"));
    assert_eq!(filter.match_of(&row), Some(NameMatch::Leading));
    let whole = element("AXButton", Some("QA Eject"), None);
    assert_eq!(filter.match_of(&whole), Some(NameMatch::Exact));
    assert!(NameMatch::Exact > NameMatch::Leading);

    for other in [
        "QA Ejected, 6 Slices",
        "Eject, QA Eject",
        "QA Eject 6 Slices",
    ] {
        assert!(
            !filter.matches(&element("AXButton", None, Some(other))),
            "{other}"
        );
    }

    let mut field = element("AXTextField", None, None);
    field.value = Some("QA Eject, typed".into());
    assert!(!filter.matches(&field), "typed text is still not a name");

    let mut identified = element("AXButton", None, None);
    identified.identifier = Some("QA Eject, more".into());
    assert!(!filter.matches(&identified), "identifiers match whole");
}

#[test]
fn quit_output_names_each_signalled_app_once() {
    assert_eq!(signalled(b"12\n 34 \nnot a pid\n"), [12, 34]);

    let process = |pid, name: &str| Process {
        pid,
        executable: format!("/{pid}"),
        name: name.to_owned(),
        protected: false,
        within: None,
    };

    let processes = [
        process(1, "Notes"),
        process(2, "Notes"),
        process(3, "Safari"),
    ];
    assert_eq!(names_by_quit(&processes, &[1, 2], true), ["Notes"]);
    assert_eq!(names_by_quit(&processes, &[1, 2], false), ["Safari"]);
}

#[test]
fn open_menus_are_an_apps_own_windows() {
    let name = Target::Name("Opie".into());
    let mut menu = window(9, 6, "Opie", MENU_LAYER);
    assert_eq!(
        app_pid(std::slice::from_ref(&menu), &name).unwrap(),
        6,
        "a menu bar app with only its menu open"
    );

    let frame = Frame {
        x: 10.0,
        y: 20.0,
        width: 300.0,
        height: 200.0,
    };
    assert_eq!(
        owning_window(std::slice::from_ref(&menu), 6, &frame),
        Some(9)
    );

    menu.alpha = 0.0;
    assert!(
        app_pid(&[menu], &name).is_err(),
        "a closed menu is no window"
    );

    // Two processes of one name: the one with a panel wins over one
    // showing only a menu, and a same-sized ordinary window over a menu.
    let panel = window(4, 7, "Opie", 3);
    assert_eq!(
        app_pid(&[window(9, 6, "Opie", MENU_LAYER), panel], &name).unwrap(),
        7
    );

    let ordinary = window(5, 6, "Opie", 0);
    assert_eq!(
        owning_window(&[window(9, 6, "Opie", MENU_LAYER), ordinary], 6, &frame),
        Some(5)
    );

    let menubar = window(1, 2, "Window Server", 24);
    assert!(!menubar.is_apps_or_menu(), "the menu bar is not the app's");

    let open = window(9, 6, "Opie", MENU_LAYER);
    let filter = WindowFilter {
        app: Some("opie".into()),
        ..WindowFilter::default()
    };

    assert!(
        !filter.matches(&open),
        "wait and windows never take an open menu for a window"
    );
}

/// An element as the scan reports it, with a frame.
fn shown(index: usize, role: &str, title: Option<&str>, description: Option<&str>) -> Element {
    let mut element = element(role, title, description);
    element.index = index;
    element.frame = Some(Frame {
        x: 10.0,
        y: 10.0,
        width: 20.0,
        height: 20.0,
    });

    element
}

fn named(name: &str) -> ElementFilter {
    ElementFilter {
        role: None,
        name: Some(name.into()),
    }
}

#[test]
fn a_click_chooses_one_visible_element() {
    let whole = shown(0, "AXButton", Some("Delete"), None);
    let row = shown(1, "AXButton", None, Some("Delete, 3 files"));
    let both = [row.clone(), whole.clone()];
    let chosen = choose(&both, &named("delete")).unwrap();
    assert_eq!(chosen.index, 0, "a whole name wins over a first part");
    assert_eq!(
        choose(std::slice::from_ref(&row), &named("delete"))
            .unwrap()
            .index,
        1,
        "a first part alone is enough"
    );

    // A whole match without a frame does not hand the click to a row
    // named only in part.
    let mut hidden = whole.clone();
    hidden.frame = None;
    let error = choose(&[hidden, row.clone()], &named("delete")).unwrap_err();
    assert!(
        error.error.contains("No visible element"),
        "{}",
        error.error
    );

    let other = shown(2, "AXButton", None, Some("Delete, 9 files"));
    let error = choose(&[row.clone(), other], &named("delete")).unwrap_err();
    assert!(
        error.error.starts_with("2 elements match"),
        "{}",
        error.error
    );
    assert_eq!(error.details["candidates"].as_array().unwrap().len(), 2);

    let mut unread = shown(3, "AXButton", None, None);
    unread.unreadable = vec!["AXTitle".into()];
    let error = choose(&[whole.clone(), unread.clone()], &named("delete")).unwrap_err();
    assert!(error.error.contains("could not be read"), "{}", error.error);
    assert!(error.details.is_empty(), "a doubtful match lists none");
}

/// A scan as the probe reports it, complete.
fn scanned(elements: Vec<Element>) -> Scanned {
    Scanned {
        elements,
        incomplete: false,
        reasons: Vec::new(),
        errors: BTreeMap::new(),
    }
}

fn frame(x: f64, y: f64, width: f64, height: f64) -> Frame {
    Frame {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn the_default_listing_names_each_element_by_its_click_point_and_size() {
    let window = frame(10.2, 38.0, 800.0, 600.0);
    let mut bold = shown(3, "AXCheckBox", Some("Bold"), None);
    bold.frame = Some(frame(388.0, 51.0, 24.0, 22.4));
    bold.window = Some(window.clone());
    bold.value = Some("0".into());
    bold.depth = 4;
    bold.identifier = Some("bold".into());
    bold.description = Some("Bold text".into());

    let mut text = shown(17, "AXTextArea", None, None);
    text.frame = Some(frame(20.0, 50.0, 780.0, 560.0));
    text.window = Some(frame(10.0, 38.5, 800.0, 600.0));
    text.value = Some("Hello".into());

    let mut label = shown(18, "AXStaticText", None, None);
    label.value = Some("Status".into());
    label.window = Some(frame(900.0, 0.0, 300.0, 200.0));

    let mut bare = element("AXGroup", None, None);
    bare.index = 19;

    let listed = listing(
        812,
        &scanned(vec![bold, text, label, bare]),
        &ElementFilter::default(),
        50,
        false,
    );
    assert_eq!(
        listed,
        json!({
            "ok": true, "pid": 812, "scanned": 4, "scanIncomplete": false,
            "matching": 4, "more": 0,
            "elements": [
                {"index": 3, "role": "AXCheckBox", "name": "Bold", "value": "0",
                 "at": [400, 62], "size": [24, 22], "window": 0},
                {"index": 17, "role": "AXTextArea", "value": "Hello",
                 "at": [410, 330], "size": [780, 560], "window": 0},
                {"index": 18, "role": "AXStaticText", "name": "Status",
                 "at": [20, 20], "size": [20, 20], "window": 1},
                {"index": 19, "role": "AXGroup"},
            ],
            "windows": [
                {"x": 10, "y": 38, "width": 800, "height": 600},
                {"x": 900, "y": 0, "width": 300, "height": 200},
            ],
        })
    );
}

#[test]
fn flags_are_listed_only_when_they_are_set() {
    let mut off = shown(0, "AXButton", Some("Save"), None);
    off.enabled = Some(false);
    off.focused = Some(false);
    off.selected = Some(false);
    off.value = Some(String::new());

    let mut on = shown(1, "AXMenuItem", Some("Ruler"), None);
    on.enabled = Some(true);
    on.focused = Some(true);
    on.selected = Some(true);
    on.menu = true;
    on.mark = Some("✓".into());
    on.truncated = vec!["AXTitle".into()];
    on.unreadable = vec!["AXDescription".into()];

    let listed = listing(
        1,
        &scanned(vec![off, on]),
        &ElementFilter::default(),
        50,
        false,
    );
    let elements = listed["elements"].as_array().unwrap();
    assert_eq!(elements[0]["disabled"], true);
    for absent in [
        "focused",
        "selected",
        "menu",
        "mark",
        "value",
        "truncated",
        "unreadable",
    ] {
        assert!(elements[0].get(absent).is_none(), "{absent}");
    }

    assert!(elements[1].get("disabled").is_none());
    assert_eq!(elements[1]["focused"], true);
    assert_eq!(elements[1]["selected"], true);
    assert_eq!(elements[1]["menu"], true);
    assert_eq!(elements[1]["mark"], "✓");
    assert_eq!(elements[1]["truncated"], json!(["AXTitle"]));
    assert_eq!(elements[1]["unreadable"], json!(["AXDescription"]));
    for dropped in [
        "depth",
        "subrole",
        "title",
        "description",
        "identifier",
        "frame",
    ] {
        assert!(elements[1].get(dropped).is_none(), "{dropped}");
    }
}

#[test]
fn elements_listed_in_full_are_as_the_scan_read_them() {
    let mut save = shown(0, "AXButton", Some("Save"), None);
    save.window = Some(frame(0.0, 0.0, 600.0, 400.0));
    save.identifier = Some("save".into());
    let scan = scanned(vec![save.clone(), shown(1, "AXButton", Some("Open"), None)]);

    let listed = listing(1, &scan, &named("save"), 50, true);
    assert_eq!(listed["elements"], json!([save]));
    assert!(listed.get("windows").is_none());
    assert_eq!(listed["matching"], 1);
    assert_eq!(listed["more"], 0);
}

#[test]
fn a_short_listing_says_how_many_more_matched() {
    let five: Vec<Element> = (0..5)
        .map(|index| shown(index, "AXButton", Some("Go"), None))
        .collect();
    let scan = scanned(five);
    let all = ElementFilter::default();

    for (max, shown, more) in [(2, 2, 3), (0, 0, 5), (50, 5, 0)] {
        for full in [false, true] {
            let listed = listing(1, &scan, &all, max, full);
            assert_eq!(listed["elements"].as_array().unwrap().len(), shown);
            assert_eq!(listed["matching"], 5);
            assert_eq!(listed["more"], more, "--max {max}");
            assert_eq!(listed["scanIncomplete"], false);
        }
    }
}

#[test]
fn a_scan_says_why_it_stopped_early() {
    let probe = |reasons: Value, errors: Value| {
        let mut reply = json!({"ok": true, "elements": [], "truncated": true, "errors": errors});
        if !reasons.is_null() {
            reply["reasons"] = reasons;
        }
        Scanned::from_probe(reply).unwrap()
    };

    let every = probe(
        json!(["depth", "count", "time", "unread", "withheld", "later"]),
        json!({}),
    );
    assert_eq!(
        every.reasons,
        [
            Stop::Depth,
            Stop::Count,
            Stop::Time,
            Stop::Unread,
            Stop::Withheld
        ]
    );
    assert!(every.incomplete);
    assert_eq!(
        every.diagnostics(),
        Some(json!({"reasons": ["depth", "count", "time", "unread", "withheld"]}))
    );

    let older = probe(Value::Null, json!({}));
    assert!(older.reasons.is_empty() && older.incomplete);
    assert_eq!(older.diagnostics(), None, "an older probe gives none");

    let mut errors = serde_json::Map::new();
    errors.insert("AXTitle:-25212".into(), json!(40));
    errors.insert("AXValue:-25205".into(), json!(40));
    errors.insert(format!("{}:-25200", "A".repeat(70)), json!(50));
    for attribute in 0..10 {
        errors.insert(
            format!("AXChildren{attribute}:-25204"),
            json!(attribute + 1),
        );
    }
    let failed = probe(json!(["unread"]), Value::Object(errors));
    let kept: Vec<(&str, usize)> = failed
        .errors
        .iter()
        .map(|(key, count)| (key.as_str(), *count))
        .collect();
    assert_eq!(kept.len(), 8, "at most eight");
    assert!(
        kept.iter().all(|(key, _)| key.starts_with("AXChildren")),
        "{kept:?}"
    );
    assert_eq!(
        kept.iter().map(|(_, count)| count).min(),
        Some(&3),
        "the most often"
    );
    assert_eq!(
        listing(1, &failed, &ElementFilter::default(), 50, false)["scan"]["reasons"],
        json!(["unread"])
    );

    let quiet = listing(
        1,
        &scanned(Vec::new()),
        &ElementFilter::default(),
        50,
        false,
    );
    assert!(quiet.get("scan").is_none());

    let mut complete = failed;
    complete.incomplete = false;
    let listed = listing(1, &complete, &ElementFilter::default(), 50, false);
    assert!(
        listed.get("scan").is_none(),
        "a complete scan has nothing to say"
    );
}

#[test]
fn a_refused_click_on_an_incomplete_scan_points_to_its_diagnostics() {
    let mut scan = scanned(Vec::new());
    scan.incomplete = true;
    scan.reasons = vec![Stop::Time, Stop::Unread];
    scan.errors = BTreeMap::from([("AXChildren:-25204".to_owned(), 2)]);

    let error = scan.refusal();
    assert_eq!(error.code, "elements");
    assert_eq!(
        error.error,
        "The accessibility scan was incomplete (it ran out of time and part of the app could \
         not be read), so a unique match cannot be established"
    );
    assert_eq!(
        error.details["scan"],
        json!({"reasons": ["time", "unread"], "errors": {"AXChildren:-25204": 2}})
    );
    assert!(
        error.details["next"]
            .as_str()
            .unwrap()
            .starts_with("Try again.")
    );

    scan.reasons.clear();
    scan.errors.clear();
    let error = scan.refusal();
    assert_eq!(
        error.error,
        "The accessibility scan was incomplete, so a unique match cannot be established"
    );
    assert!(error.details.get("scan").is_none());
}

#[test]
fn an_app_that_withheld_its_windows_is_reported_never_listed_as_empty() {
    let probe = json!({
        "ok": true,
        "elements": [{"index": 0, "depth": 0, "role": "AXMenuBarItem", "title": "Edit"}],
        "truncated": true,
        "reasons": ["withheld"],
        "errors": {},
    });
    let scan = Scanned::from_probe(probe).unwrap();

    let listed = listing(1, &scan, &ElementFilter::default(), 50, false);
    assert_eq!(listed["scanIncomplete"], true);
    assert_eq!(listed["scan"]["reasons"], json!(["withheld"]));
    assert_eq!(listed["next"], WITHHELD_NEXT);

    let error = scan.refusal();
    assert!(
        error.error.contains("the app withheld its windows"),
        "{}",
        error.error
    );
    assert_eq!(error.details["next"], WITHHELD_NEXT);

    let mut complete = scanned(Vec::new());
    complete.incomplete = true;
    complete.reasons = vec![Stop::Unread];
    let listed = listing(1, &complete, &ElementFilter::default(), 50, false);
    assert!(listed.get("next").is_none(), "only a withheld scan says so");
}

/// The names a refused click lists, in order.
fn candidate_names(error: &Error) -> Vec<&str> {
    error.details["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|candidate| candidate["name"].as_str())
        .collect()
}

#[test]
fn a_name_that_matches_nothing_lists_the_closest_names() {
    let at = |index, x: f64, role: &str, title: &str| {
        let mut element = shown(index, role, Some(title), None);
        element.frame = Some(frame(x, 0.0, 40.0, 22.0));
        element
    };
    let bar = [
        at(0, 10.0, "AXMenuBarItem", "Edit"),
        at(1, 50.0, "AXMenuBarItem", "Format"),
        at(2, 90.0, "AXMenuBarItem", "Window"),
        at(3, 130.0, "AXMenuBarItem", "Zoom"),
    ];
    let error = choose(&bar, &named("Formt")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Format"],
        "unrelated names are left out"
    );
    assert!(
        error
            .error
            .ends_with(" The closest names are in candidates.")
    );
    assert_eq!(
        error.details["candidates"][0],
        json!({"role": "AXMenuBarItem", "name": "Format", "at": [70, 11]})
    );

    let error = choose(&bar, &named("Nothing like it")).unwrap_err();
    assert!(error.details.get("candidates").is_none());
    assert!(error.error.ends_with("what is there."), "{}", error.error);

    let close = [
        at(0, 10.0, "AXButton", "Sane"),
        at(1, 50.0, "AXButton", "Save As…"),
        at(2, 90.0, "AXButton", "Sav"),
        at(3, 130.0, "AXButton", "Cave"),
        at(4, 170.0, "AXButton", "Sage"),
        at(5, 210.0, "AXButton", "Wave"),
        at(6, 250.0, "AXButton", "Have"),
    ];
    let error = choose(&close, &named("save")).unwrap_err();
    let names = candidate_names(&error);
    assert_eq!(names.len(), 5, "at most five: {names:?}");
    assert_eq!(
        names[..2],
        ["Sav", "Save As…"],
        "a part, nearest first, before a few letters off"
    );

    let error = choose(&[at(0, 10.0, "AXButton", "Sofe")], &named("save")).unwrap_err();
    assert!(
        error.details.get("candidates").is_none(),
        "two letters off is too far for a short name"
    );

    let dialog = [
        at(0, 10.0, "AXButton", "No"),
        at(1, 50.0, "AXButton", "Go"),
        at(2, 90.0, "AXButton", "Okay"),
    ];
    let error = choose(&dialog, &named("OK")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Okay"],
        "a short name is not matched a letter or two off"
    );

    let mut hidden = at(7, 0.0, "AXButton", "Safe");
    hidden.frame = None;
    let error = choose(&[hidden], &named("save")).unwrap_err();
    assert!(
        error.details.get("candidates").is_none(),
        "unframed is skipped"
    );

    let window = at(0, 10.0, "AXButton", "Copied");
    let mut item = at(1, 50.0, "AXMenuItem", "Copy Style");
    item.menu = true;
    let error = choose(&[window, item], &named("Copy")).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Copy Style"],
        "only the open menu"
    );

    let by_role = ElementFilter {
        role: Some("AXButton".into()),
        name: Some("format".into()),
    };
    let mut button = at(4, 170.0, "AXButton", "Formats");
    button.index = 0;
    let error = choose(&[button, bar[1].clone()], &by_role).unwrap_err();
    assert_eq!(
        candidate_names(&error),
        ["Format", "Formats"],
        "the exact name under another role first"
    );

    let listed = listing(1, &scanned(bar.to_vec()), &named("Formt"), 50, false);
    assert_eq!(listed["matching"], 0);
    assert_eq!(listed["candidates"][0]["name"], "Format");
    let listed = listing(1, &scanned(bar.to_vec()), &named("Format"), 50, false);
    assert!(listed.get("candidates").is_none());
}

#[test]
fn a_checkbox_or_radio_button_is_clicked_on_its_box_not_the_middle_of_its_frame() {
    let wide = frame(100.0, 200.0, 237.0, 18.0);
    assert_eq!(click_point(Some("AXCheckBox"), &wide), (109.0, 209.0));
    assert_eq!(click_point(Some("AXRadioButton"), &wide), (109.0, 209.0));

    let tab = frame(100.0, 200.0, 80.0, 50.0);
    assert_eq!(click_point(Some("AXRadioButton"), &tab), (140.0, 225.0));

    let bare = frame(100.0, 200.0, 14.0, 30.0);
    assert_eq!(click_point(Some("AXCheckBox"), &bare), (107.0, 215.0));
}

#[test]
fn an_ordinary_button_is_clicked_in_the_middle_of_its_frame() {
    let wide = frame(100.0, 200.0, 237.0, 18.0);
    assert_eq!(click_point(Some("AXButton"), &wide), (219.0, 209.0));
    assert_eq!(click_point(None, &wide), (219.0, 209.0));
}

#[test]
fn a_click_point_always_lies_inside_its_elements_frame() {
    let frames = [
        frame(100.0, 200.0, 237.0, 16.0),
        frame(485.0, 344.0, 115.0, 24.0),
        frame(100.0, 200.0, 14.0, 30.0),
        frame(10.4, 20.6, 41.0, 20.0),
        frame(-50.6, -20.4, 300.0, 1.0),
        frame(0.0, 0.0, 1.0, 1.0),
    ];

    for role in [
        Some("AXCheckBox"),
        Some("AXRadioButton"),
        Some("AXButton"),
        None,
    ] {
        for frame in &frames {
            let (x, y) = click_point(role, frame);
            assert!(
                (frame.x..=frame.x + frame.width).contains(&x)
                    && (frame.y..=frame.y + frame.height).contains(&y),
                "{role:?} {frame:?} -> {x}, {y}"
            );
        }
    }
}

#[test]
fn listed_and_candidate_points_aim_at_a_checkbox_box() {
    let detectors = |index| {
        let mut element = shown(index, "AXCheckBox", Some("Data detectors"), None);
        element.frame = Some(frame(100.0, 200.0 + 30.0 * index as f64, 237.0, 18.0));
        element
    };

    let listed = listing(
        1,
        &scanned(vec![detectors(0)]),
        &named("Data detectors"),
        50,
        false,
    );
    assert_eq!(listed["elements"][0]["at"], json!([109, 209]));

    let error = choose(&[detectors(0), detectors(1)], &named("Data detectors")).unwrap_err();
    assert_eq!(
        error.details["candidates"],
        json!([
            {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 209]},
            {"role": "AXCheckBox", "name": "Data detectors", "at": [109, 239]},
        ])
    );
}

#[test]
fn a_name_that_matches_several_lists_each_match() {
    let close = |index| {
        let mut element = shown(index, "AXButton", None, Some("close button"));
        element.frame = Some(frame(10.0 + 100.0 * index as f64, 0.0, 14.0, 14.0));
        element
    };

    let error = choose(&[close(0), close(1)], &named("close button")).unwrap_err();
    assert!(
        error.error.starts_with("2 elements match"),
        "{}",
        error.error
    );
    assert_eq!(
        error.details["candidates"],
        json!([
            {"role": "AXButton", "name": "close button", "at": [17, 7]},
            {"role": "AXButton", "name": "close button", "at": [117, 7]},
        ])
    );

    let seven: Vec<Element> = (0..7).map(close).collect();
    let error = choose(&seven, &named("close button")).unwrap_err();
    assert_eq!(error.details["candidates"].as_array().unwrap().len(), 5);
    assert_eq!(error.details["candidates"][4]["at"], json!([417, 7]));
}

/// Names fold case beyond ASCII; an attribute shown only in part is never
/// an exact match but may hide one; a collapsed element is not on
/// screen; a menu bar title gives way to a window element of its name.
#[test]
fn matching_is_honest_about_what_it_cannot_see() {
    let editer = shown(0, "AXButton", Some("Éditer"), None);
    assert_eq!(
        choose(std::slice::from_ref(&editer), &named("ÉDITER"))
            .unwrap()
            .index,
        0
    );

    let mut cut = shown(1, "AXButton", Some("Delete"), None);
    cut.truncated = vec!["AXTitle".into()];
    assert!(!named("delete").matches(&cut), "a cut title proves nothing");
    assert!(named("delete").might_match(&cut), "but it may hide a match");

    let whole = shown(2, "AXButton", Some("Delete"), None);
    let error = choose(&[whole.clone(), cut], &named("delete")).unwrap_err();
    assert!(error.error.contains("could not be read"), "{}", error.error);

    let mut collapsed = shown(3, "AXButton", Some("Delete"), None);
    collapsed.frame.as_mut().unwrap().height = 0.0;
    assert_eq!(
        choose(&[collapsed, whole.clone()], &named("delete"))
            .unwrap()
            .index,
        2,
        "a collapsed element is not on screen"
    );

    // A long message cut short cannot hide a short name; what is shown
    // of a cut name can.
    let mut message = shown(6, "AXStaticText", None, None);
    message.value = Some("This document explains what the button does".into());
    message.truncated = vec!["AXValue".into()];
    let ok = shown(7, "AXButton", Some("OK"), None);
    assert_eq!(
        choose(&[message, ok.clone()], &named("ok")).unwrap().index,
        7
    );

    let mut long = shown(8, "AXButton", Some("Delete every"), None);
    long.truncated = vec!["AXTitle".into()];
    assert!(named("delete everything").might_match(&long));
    assert!(!named("delete").might_match(&long));

    // The whole first part of a cut label is still judged.
    let mut parts = shown(
        9,
        "AXMenuItem",
        Some("QA Eject, Option-Space, 6 Slices"),
        None,
    );
    parts.truncated = vec!["AXTitle".into()];
    assert!(matches!(
        named("qa eject").match_of(&parts),
        Some(NameMatch::Leading)
    ));
    assert!(!named("eject").might_match(&parts));

    // A static text whose value failed to read may be the label, unless
    // it is collapsed.
    let mut unread = shown(10, "AXStaticText", None, None);
    unread.unreadable = vec!["AXValue".into()];
    assert!(named("ok").might_match(&unread));
    assert!(choose(&[unread.clone(), ok.clone()], &named("ok")).is_err());

    unread.frame.as_mut().unwrap().width = 0.0;
    assert_eq!(choose(&[unread, ok], &named("ok")).unwrap().index, 7);

    let help = shown(4, "AXButton", Some("Help"), None);
    let title = shown(5, "AXMenuBarItem", Some("Help"), None);
    assert_eq!(
        choose(&[title.clone(), help], &named("help"))
            .unwrap()
            .index,
        4,
        "the window's Help button, not the menu"
    );

    let by_role = ElementFilter {
        role: Some("AXMenuBarItem".into()),
        name: Some("help".into()),
    };
    assert_eq!(choose(&[title], &by_role).unwrap().index, 5);
}

#[test]
fn with_a_menu_open_only_its_items_are_chosen() {
    let button = shown(0, "AXButton", Some("Copy"), None);
    let mut unread = shown(1, "AXTextArea", None, None);
    unread.unreadable = vec!["AXDescription".into()];
    let mut item = shown(2, "AXMenuItem", Some("Copy"), None);
    item.menu = true;

    let elements = [button.clone(), unread.clone(), item.clone()];
    assert_eq!(
        choose(&elements, &named("copy")).unwrap().index,
        2,
        "the window's button and its unreadable neighbour are out of play"
    );

    let error = choose(&[button, unread, item], &named("paste")).unwrap_err();
    assert!(error.error.contains("open menu"), "{}", error.error);
}

#[test]
fn an_apps_visible_floating_windows_are_listed_and_hidden_ones_are_not() {
    let name = Target::Name("Opie".into());
    let mut hidden = window(4, 6, "Opie", 3);
    hidden.alpha = 0.0;
    assert!(
        app_pid(&[hidden], &name).is_err(),
        "an invisible panel is no window"
    );

    let panel = window(5, 6, "Opie", 3);
    assert_eq!(app_pid(std::slice::from_ref(&panel), &name).unwrap(), 6);

    let frame = Frame {
        x: 10.0,
        y: 20.0,
        width: 300.0,
        height: 200.0,
    };
    let ordinary = window(7, 6, "Opie", 0);
    assert_eq!(
        owning_window(&[panel.clone(), ordinary], 6, &frame),
        Some(7),
        "the ordinary window wins over a same-sized overlay"
    );
    assert_eq!(
        owning_window(std::slice::from_ref(&panel), 6, &frame),
        Some(5)
    );

    let filter = WindowFilter {
        app: Some("opie".into()),
        ..WindowFilter::default()
    };

    assert!(
        filter.matches(&panel),
        "windows --app shows its floating panel"
    );
    assert!(
        WindowFilter::default().matches(&panel),
        "windows shows it too"
    );
}

#[test]
fn a_launch_refused_for_a_finished_job_names_its_app() {
    let said = "The application /System/Applications/Calculator.app cannot be opened for an \
                unexpected reason, error=Error Domain=RBSRequestErrorDomain Code=5 \"Launch \
                failed.\" UserInfo={NSUnderlyingError=0x7482c10990 {Error \
                Domain=OSLaunchdErrorDomain Code=37 \"Operation already in progress\"}}";
    assert_eq!(
        stale_launch(said),
        Some("/System/Applications/Calculator.app")
    );

    let other = said.replace("Code=37", "Code=3");
    assert_eq!(stale_launch(&other), None, "only error 37");
    assert_eq!(
        stale_launch("Unable to find application named 'Nope'"),
        None
    );
}

#[test]
fn apps_resolve_to_exactly_one_process() {
    let name = |name: &str| Target::Name(name.into());
    let windows = [window(1, 7, "Dock", 20), window(2, 8, "Calculator", 0)];
    assert_eq!(app_pid(&windows, &name("calculator")).unwrap(), 8);
    assert!(
        app_pid(&windows, &name("Dock")).is_err(),
        "only document windows"
    );
    assert!(app_pid(&windows, &name("Preview")).is_err());

    let alert = [window(3, 5, "Atlas", 8), window(1, 7, "Dock", 20)];
    assert_eq!(
        app_pid(&alert, &name("Atlas")).unwrap(),
        5,
        "a floating alert"
    );

    let twice = [window(1, 8, "TextEdit", 0), window(2, 9, "TextEdit", 0)];
    let error = app_pid(&twice, &name("TextEdit")).unwrap_err();
    assert!(error.error.contains("--pid"), "{}", error.error);
    assert_eq!(app_pid(&twice, &Target::Pid(9)).unwrap(), 9);
}

#[test]
fn elements_belong_to_the_window_with_their_frame() {
    let windows = [window(4, 8, "TextEdit", 0), {
        let mut other = window(5, 8, "TextEdit", 0);
        other.bounds.x = 400.0;
        other
    }];

    let frame = |x: f64| Frame {
        x,
        y: 20.0,
        width: 300.0,
        height: 200.0,
    };
    assert_eq!(owning_window(&windows, 8, &frame(400.5)), Some(5));
    assert_eq!(owning_window(&windows, 8, &frame(10.0)), Some(4));
    assert_eq!(
        owning_window(&windows, 9, &frame(10.0)),
        None,
        "other process"
    );
    assert_eq!(owning_window(&windows, 8, &frame(200.0)), None);

    let twins = [window(4, 8, "TextEdit", 0), window(5, 8, "TextEdit", 0)];
    assert_eq!(
        owning_window(&twins, 8, &frame(10.0)),
        None,
        "identical geometry is ambiguous"
    );
}

/// TextEdit (812) as the helper lists it.
fn textedit_running() -> Value {
    json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit", "policy": "regular",
        "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit"}]})
}

/// The helper's refusal with `code`.
fn refused(code: &str) -> Value {
    json!({"ok": false, "code": code, "error": "Refused", "uncertain": false})
}

/// A copy root under `directory` holding two unlabelled copies opened
/// with TextEdit, which nothing else uses.
fn two_copies(directory: &Path) -> (std::path::PathBuf, [std::path::PathBuf; 2]) {
    use std::os::unix::fs::DirBuilderExt;

    let root = directory.join("copies");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    let copies = ["a", "b"].map(|name| {
        let folder = root.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&folder)
            .unwrap();
        std::fs::write(folder.join("notes.rtf"), "hello").unwrap();
        std::fs::write(folder.join(".opened-with"), "TextEdit").unwrap();
        folder
    });
    (root, copies)
}

/// Whether `request` is the script that quits apps.
fn quitting(request: &Value) -> bool {
    request["op"] == "run" && request["argv"][2] == QUIT
}

#[tokio::test]
async fn a_close_whose_first_list_is_refused_quits_nothing_and_keeps_every_copy() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());
    let mut signalled = false;

    let error = helper
        .serve(
            close_in(&client, Closing::App("TextEdit"), &root),
            |request| {
                signalled |= quitting(request);
                Some(refused("busy"))
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Busy, false));
    assert!(!signalled, "nothing is quit without a list");
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[tokio::test]
async fn a_close_whose_list_fails_after_the_signal_keeps_the_copies_and_says_what_was_signalled() {
    use crate::test_support::{Helper, UID, completed};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());
    let mut lists = 0;

    let error = helper
        .serve(
            close_in(&client, Closing::App("TextEdit"), &root),
            |request| {
                if quitting(request) {
                    return Some(completed("812\n"));
                }
                lists += 1;
                Some(if lists == 1 {
                    textedit_running()
                } else {
                    refused("apps")
                })
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Apps, false));
    assert_eq!(error.details["signalled"], json!(["TextEdit"]));
    assert!(
        error.error.contains("Their copies were kept"),
        "{}",
        error.error
    );
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[test]
fn an_unconfirmed_quit_always_says_what_it_asked_in_plain_words() {
    let said = |names: &[&str]| {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        unconfirmed(Error::new(Code::Apps, "Refused"), &names)
    };

    let named = said(&["TextEdit"]);
    assert!(
        named.error.starts_with("Atlas asked TextEdit to quit"),
        "{named}"
    );
    for nameless in [said(&[]), said(&[""]), said(&["", ""])] {
        assert!(!nameless.error.contains("  "), "{nameless}");
        assert!(
            nameless.error.contains("the apps it was closing"),
            "{nameless}"
        );
        assert_eq!(nameless.code, "apps");
    }
    assert_eq!(said(&["", "Notes"]).details["signalled"], json!(["Notes"]));
}

#[tokio::test]
async fn a_malformed_list_never_reads_as_not_running() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());

    let error = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": null}))
        })
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Protocol, false));
    assert!(copies.iter().all(|copy| copy.exists()));
}

#[tokio::test]
async fn an_empty_list_still_means_not_running() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let (root, copies) = two_copies(directory.path());

    let reply = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": []}))
        })
        .await
        .unwrap();

    assert_eq!(reply["reason"], "TextEdit is not running");
    assert_eq!(reply["closed"], false);
    assert_eq!(reply["removed"].as_array().map(Vec::len), Some(2));
    assert!(copies.iter().all(|copy| !copy.exists()));
}

#[tokio::test]
async fn removed_copies_are_settled_in_the_task_that_claimed_them() {
    use crate::{
        files::Token,
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::{Helper, UID},
    };
    use std::os::unix::fs::DirBuilderExt;

    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let session = HostSession::new(Host::Codex, "closing").unwrap();
    let task = Task::of(session.key(), Some("probe")).unwrap();
    client.host_session = Some(session.clone());
    client.task = Some(task.clone());
    let output = client.output_dir().unwrap().to_owned();
    let (root, _) = two_copies(directory.path());
    let token = Token::new();
    let folder = root.join(token.as_str());
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .unwrap();
    std::fs::write(folder.join("notes.rtf"), "hello").unwrap();
    std::fs::write(folder.join(".opened-with"), "TextEdit").unwrap();
    std::fs::write(folder.join(".session"), format!("{}\n", session.key())).unwrap();
    let copy = Owned::Copy { id: token };
    Claims::in_store(Store::at(&output), task.clone())
        .claim(&copy, State::Made)
        .unwrap();

    let reply = helper
        .serve(close_in(&client, Closing::App("TextEdit"), &root), |_| {
            Some(json!({"ok": true, "apps": []}))
        })
        .await
        .unwrap();

    assert_eq!(reply["removed"].as_array().map(Vec::len), Some(3));
    let record = Store::at(&output).load(task.key()).unwrap();
    assert_eq!(record.claims().collect::<Vec<_>>(), [(copy, State::Gone)]);
}

#[tokio::test]
async fn an_app_opened_by_name_is_listed_for_the_task_and_nothing_else_is() {
    use crate::{
        host_session::{Host, Key},
        task::{Owned, State, Task, store::Store},
        test_support::{Helper, UID, completed},
    };

    let directory = tempfile::tempdir().unwrap();
    let (mut client, helper) = Helper::client(directory.path(), UID);
    let task = Task::of(&Key::of(Host::Codex, "opening"), Some("probe")).unwrap();
    client.task = Some(task.clone());
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let answer = |request: &Value| {
        Some(if request["op"] == "apps" {
            json!({"ok": true, "apps": []})
        } else {
            completed("")
        })
    };

    for (app, targets) in [
        (None, vec!["https://example.com".to_owned()]),
        (Some("TextEdit"), Vec::new()),
    ] {
        let opened = helper
            .serve(open_in(&client, app, &targets, &[], &copies), answer)
            .await
            .unwrap();
        assert_eq!(opened["opened"], true);
    }

    let output = client.output_dir().unwrap().to_owned();
    let claims: Vec<_> = Store::at(&output)
        .load(task.key())
        .unwrap()
        .claims()
        .collect();
    assert_eq!(
        claims,
        [(
            Owned::Launch {
                app: "TextEdit".to_owned()
            },
            State::Made
        )]
    );
}

#[tokio::test]
async fn an_open_whose_list_is_refused_copies_prunes_and_launches_nothing() {
    use crate::test_support::{Helper, UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let unreadable = directory.path().join("notes.rtf");
    std::fs::write(&unreadable, "hello").unwrap();
    let targets = [unreadable.to_string_lossy().into_owned()];
    let mut ran = Vec::new();

    let error = helper
        .serve(
            open_in(&client, Some("TextEdit"), &targets, &[], directory.path()),
            |request| {
                if request["op"] == "run" {
                    ran.push(request.clone());
                }
                Some(refused("busy"))
            },
        )
        .await
        .unwrap_err();

    assert_eq!((error.code, error.uncertain), (Code::Busy, false));
    assert!(
        ran.is_empty(),
        "nothing was checked, copied or launched: {ran:?}"
    );
}
#[tokio::test]
async fn an_app_launch_services_cannot_find_opens_from_the_tasks_own_install() {
    use crate::{
        files::Token,
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::{Helper, completed, exited},
    };

    let directory = tempfile::tempdir().unwrap();
    let nobody = nix::unistd::User::from_name("nobody").unwrap().unwrap();
    let (mut client, helper) = Helper::staging(directory.path(), nobody.uid.as_raw());
    let session = HostSession::new(Host::Codex, "installed").unwrap();
    let task = Task::of(session.key(), Some("probe")).unwrap();
    client.host_session = Some(session);
    client.task = Some(task.clone());
    let output = client.output_dir().unwrap().to_owned();
    let install = Token::new();
    Claims::in_store(Store::at(&output), task.clone())
        .claim(
            &Owned::Install {
                id: install.clone(),
                app: "Probe".into(),
            },
            State::Made,
        )
        .unwrap();
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let path = "/Users/helper/Applications/Probe.app";
    let mut launched: Vec<Value> = Vec::new();

    let opened = helper
        .serve(
            open_in(&client, Some("Probe"), &[], &[], &copies),
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok": true, "apps": []}));
                }
                if request["argv"][1] == "environment-worker" {
                    let listed = json!({"ok": true, "installs": [{"installId": install,
                            "phase": "installed", "verified": true, "path": path,
                            "root": {"device": 1, "inode": 2}}]});
                    return Some(completed(&listed.to_string()));
                }
                if request["argv"][0] == "/usr/bin/open" {
                    launched.push(request["argv"].clone());
                    return Some(if request["argv"][2] == "Probe" {
                        exited(1, "Unable to find application named 'Probe'\n")
                    } else {
                        completed("")
                    });
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(opened["opened"], true, "{opened}");
    assert_eq!(
        launched,
        [
            json!(["/usr/bin/open", "-a", "Probe"]),
            json!(["/usr/bin/open", "-a", path])
        ]
    );
    let claims: Vec<_> = Store::at(&output)
        .load(task.key())
        .unwrap()
        .claims()
        .collect();
    assert!(
        !claims
            .iter()
            .any(|(owned, _)| matches!(owned, Owned::Launch { .. })),
        "the install's own cleanup quits it: {claims:?}"
    );
}

#[tokio::test]
async fn an_app_no_task_installed_is_not_found_by_name_and_nothing_else_is_tried() {
    use crate::test_support::{Helper, UID, completed, exited};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let copies = directory.path().join("copies");
    std::fs::create_dir(&copies).unwrap();
    let mut launched = 0;

    let opened = helper
        .serve(
            open_in(&client, Some("Probe"), &[], &[], &copies),
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok": true, "apps": []}));
                }
                if request["argv"][0] == "/usr/bin/open" {
                    launched += 1;
                    return Some(exited(1, "Unable to find application named 'Probe'\n"));
                }
                Some(completed(""))
            },
        )
        .await
        .unwrap();

    assert_eq!(
        (&opened["opened"], &opened["code"]),
        (&json!(false), &json!("spawn-failed"))
    );
    assert_eq!(launched, 1);
}

#[tokio::test]
async fn an_open_records_the_launched_apps_identity_without_keeping_its_window_title() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, UID, completed},
    };

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = client.doing(Doing::open(Some("TextEdit"), &[]));
    let mut launched = false;
    let reply = helper
        .serve(
            open_in(
                &client,
                Some("TextEdit"),
                &[],
                &[],
                &directory.path().join("copies"),
            ),
            |request| {
                Some(if request["op"] == "apps" {
                    if launched {
                        json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit",
                    "bundleId": "com.apple.TextEdit", "title": "PRIVATE DOCUMENT TITLE"}]})
                    } else {
                        json!({"ok": true, "apps": []})
                    }
                } else {
                    launched = true;
                    completed("")
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["opened"], true);
    let rows = activity_log::recent(client.output_dir().unwrap(), 10);
    assert_eq!(
        rows.len(),
        1,
        "the completion replaces the lower-level run steps"
    );
    assert_eq!(rows[0].app.as_deref(), Some("TextEdit"));
    assert_eq!(rows[0].bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(rows[0].label, None);
    let log =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(!log.contains("PRIVATE DOCUMENT TITLE"));
}

#[tokio::test]
async fn an_open_stays_successful_when_its_identity_read_fails_or_the_session_changes() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, UID, completed},
    };

    for changed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = if changed {
            helper.moving_after(2)
        } else {
            helper
        };
        let client = client.doing(Doing::open(Some("TextEdit"), &[]));
        let mut lists = 0;
        let reply = helper
            .serve(
                open_in(
                    &client,
                    Some("TextEdit"),
                    &[],
                    &[],
                    &directory.path().join("copies"),
                ),
                |request| {
                    Some(if request["op"] == "apps" {
                        lists += 1;
                        if lists == 1 {
                            json!({"ok": true, "apps": []})
                        } else {
                            refused("apps")
                        }
                    } else {
                        completed("")
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["opened"], true);
        let rows = activity_log::recent(client.output_dir().unwrap(), 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bundle_id, None);
        assert!(rows[0].worked());
        assert_eq!(lists, if changed { 1 } else { 2 });
    }
}

#[tokio::test]
async fn a_close_keeps_the_apps_identity_after_it_has_quit() {
    use crate::{
        activity_log,
        doing::Doing,
        test_support::{Helper, UID, completed},
    };

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = client.doing(Doing::new("close").app(Some("TextEdit")));
    let mut signalled = false;
    let reply = helper
        .serve(
            close_in(
                &client,
                Closing::App("TextEdit"),
                &directory.path().join("copies"),
            ),
            |request| {
                Some(if quitting(request) {
                    signalled = true;
                    completed("812\n")
                } else if signalled {
                    json!({"ok": true, "apps": []})
                } else {
                    let mut apps = textedit_running();
                    apps["apps"][0]["bundleId"] = json!("com.apple.TextEdit");
                    apps["apps"][0]["title"] = json!("PRIVATE DOCUMENT TITLE");
                    apps
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["closed"], true);
    let rows = activity_log::recent(client.output_dir().unwrap(), 10);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bundle_id.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(rows[0].app.as_deref(), Some("TextEdit"));
    assert_eq!(rows[0].label, None);
    let log =
        std::fs::read_to_string(client.output_dir().unwrap().join(activity_log::FILE)).unwrap();
    assert!(!log.contains("PRIVATE DOCUMENT TITLE"));
}

#[test]
fn an_app_name_keeps_its_identity_only_when_its_processes_agree() {
    let apps = |second: Value| {
        json!({"apps": [
            {"name": "TextEdit", "bundleId": "com.apple.TextEdit"}, second
        ]})
    };
    assert!(
        named_app(
            &apps(json!({"name": "textedit", "bundleId": "com.apple.TextEdit"})),
            "TextEdit"
        )
        .is_some()
    );
    for second in [
        json!({"name": "TextEdit", "bundleId": "other.app"}),
        json!({"name": "TextEdit", "bundleId": null}),
    ] {
        assert!(named_app(&apps(second), "TextEdit").is_none());
    }
    assert!(
        named_app(
            &apps(json!({"name": "Other", "bundleId": "other.app"})),
            "Missing"
        )
        .is_none()
    );
}
