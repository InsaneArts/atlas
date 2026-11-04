use super::{elements::*, expect::*, input::*, launch::*, quit::*, target::*, windows::*};
use crate::app_selection::{Closing, Process};
use atlas_control::interrupt::Interrupt;
use atlas_core::{
    actions::{Batch, Button, Expect},
    error::{Code, Error, Holder, Result},
    screen::select::{ElementFilter, NameMatch},
    screen::{
        elements::{Element, Look, Stop, click_point, element_check},
        windows::{Bounds, MENU_BAR_LAYER, MENU_LAYER, Window, owning_window},
    },
    session::{Display, FOCUS_APP, LOOKS},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Duration};

#[test]
fn helper_identity_resolves_even_when_the_process_is_unavailable_to_the_controller() {
    let pid = i64::from(i32::MAX);
    let start = [123, 456];
    let starts = BTreeMap::from([(pid, start)]);
    let expect = snapshot_expect(pid, &starts).unwrap();
    assert_eq!(
        serde_json::to_value(&expect).unwrap(),
        json!({"pid":pid,"start":start})
    );
    assert_eq!(expect.app_only().instance(), expect.instance());
}

#[test]
fn a_current_snapshot_without_the_process_identity_refuses_instead_of_adopting_it() {
    for starts in [
        BTreeMap::new(),
        BTreeMap::from([(812, [0, 1])]),
        BTreeMap::from([(812, [123, 1_000_000])]),
    ] {
        let error = snapshot_expect(812, &starts).unwrap_err();
        assert_eq!(error.code, Code::Elements);
        assert!(!error.uncertain);
    }
}

#[test]
fn window_snapshot_identities_are_read_but_never_appear_in_the_public_window_rows() {
    let reply = json!({
        "ok":true,"starts":{"812":[123,456]},"windows":[window(42,812,"Editor",0)]
    });
    let snapshot = atlas_core::screen::windows::from_reply(&reply).unwrap();
    let expect = snapshot_expect(snapshot.windows[0].pid, &snapshot.starts).unwrap();
    assert_eq!(
        serde_json::to_value(expect).unwrap(),
        json!({"pid":812,"start":[123,456]})
    );
    let public = json!({"ok":true,"windows":snapshot.windows});
    assert!(public.get("starts").is_none());
    assert!(public["windows"][0].get("start").is_none());
    assert!(public["windows"][0].get("starts").is_none());
}

/// The stand-in helper's answer to a read: TextEdit (812) with one
/// window and no elements.
fn textedit_reads(request: &Value) -> Option<Value> {
    use crate::test_support::completed;

    if request["op"] == "elements" {
        assert_eq!(request["expect"], json!({"pid": 812, "start": [123, 456]}));
        return Some(json!({"ok": true, "look": "8123abc", "elements": [], "truncated": false}));
    }
    if request["op"] == "apps" {
        return Some(
            json!({"ok": true, "apps": [{"pid": 812, "name": "TextEdit"}],
            "starts": {"812": [123, 456]}}),
        );
    }
    if request["op"] == "windows" {
        return Some(json!({"ok": true, "starts": {"812": [123, 456]},
            "windows": [{"number": 5, "pid": 812,
            "owner": "TextEdit", "title": null, "layer": 0, "alpha": 1.0,
            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}}]}));
    }
    let output = match request["argv"][1].as_str() {
        Some("elements-info") => json!({"ok": true, "elements": [], "truncated": false}),
        other => panic!("nothing else is read: {other:?}"),
    };
    Some(completed(&output.to_string()))
}

#[tokio::test]
async fn atlas_windows_reads_once_and_remembers_that_reading() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for named in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        if named {
            client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        }
        let mut reads = 0;
        let reply = helper
            .serve(list(&client, &WindowFilter::default()), |request| {
                assert_eq!(request["op"], "windows", "no window probe runs");
                assert!(request.get("argv").is_none());
                assert!(request["session"].is_object());
                assert_eq!(request["dock"], named);
                reads += 1;
                Some(
                    json!({"ok": true, "windows": [window(42, 812, "Editor", 0)],
                "starts": {"812": [123,456]}}),
                )
            })
            .await
            .unwrap();
        assert_eq!(reads, 1);
        assert_eq!(json!(reply), json!([window(42, 812, "Editor", 0)]));
        if named {
            let memory = client.remembered_windows().unwrap();
            assert_eq!(memory.windows.as_ref().unwrap().len(), 1);
            assert_eq!(memory.windows.unwrap()[0].number, 42);
        }
    }
}

/// An app found by name or process ID is looked for in the helper the
/// command verified moments before: the lookup and the scan after it
/// verify the helper once between them, not once for each list read.
#[tokio::test]
async fn finding_an_app_verifies_the_helper_once() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    for target in [Target::Name("TextEdit".into()), Target::Pid(812)] {
        let before = helper.proofs();
        let listed = helper
            .serve(
                elements(&client, &target, &ElementFilter::default(), None, 10, false),
                textedit_reads,
            )
            .await
            .unwrap();
        assert_eq!(listed["pid"], 812, "{target:?}");
        assert_eq!(listed["look"], "8123abc", "{target:?}");
        assert_eq!(helper.proofs() - before, 1, "{target:?}");
    }
}

/// A look is asked of the input service in the session just verified, for
/// the very process found; one that cannot keep looks is asked nothing.
#[tokio::test]
async fn a_look_is_asked_of_an_input_service_that_keeps_looks() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[FOCUS_APP]);
    let error = helper
        .serve(
            elements(
                &client,
                &Target::Pid(812),
                &ElementFilter::default(),
                None,
                10,
                false,
            ),
            |request| {
                assert_ne!(request["op"], "elements", "no look is asked for");
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Startup);
    assert!(error.error.contains("atlas start-input"), "{error}");

    for (reply, code) in [
        (
            json!({"ok": true, "elements": [], "truncated": false}),
            "elements",
        ),
        (
            json!({"ok": true, "look": "8123ABC", "elements": [], "truncated": false}),
            "elements",
        ),
        (
            json!({"ok": false, "code": "session-changed", "uncertain": true,
                "error": "The helper changed since Atlas checked it; look again"}),
            "session-changed",
        ),
        (
            json!({"ok": false, "code": "permission", "uncertain": false,
                "error": "The app's accessibility tree is unavailable; check Accessibility for the helper"}),
            "elements",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.serving_input(&client, &[LOOKS]);
        let error = helper
            .serve(
                elements(
                    &client,
                    &Target::Pid(812),
                    &ElementFilter::default(),
                    None,
                    10,
                    false,
                ),
                |request| {
                    if request["op"] == "elements" {
                        assert_eq!(request["epoch"]["processId"], 456);
                        return Some(reply.clone());
                    }
                    textedit_reads(request)
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, code, "{reply}");
        assert!(!error.uncertain, "a read changes nothing");
        if let Some(said) = reply["error"].as_str() {
            assert_eq!(error.error, said, "the read's own words");
        }
    }
}

/// A daemon that cannot be told which session a read is for is not
/// trusted to be the one verified: nothing is read from it.
#[tokio::test]
async fn a_daemon_that_cannot_pin_a_read_is_refused() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.older();
    let target = Target::Name("TextEdit".into());
    let error = helper
        .serve(
            elements(&client, &target, &ElementFilter::default(), None, 10, false),
            |request| panic!("nothing is read: {request}"),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Daemon);
    assert!(error.error.contains("run atlas restart"), "{error}");
}

/// An app found in one helper session is never captured in another:
/// its process ID means nothing there, so no capture is asked for.
#[tokio::test]
async fn an_app_is_captured_only_in_the_session_it_was_found_in() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.moving_after(1);
    let mut captures = 0;
    let error = helper
        .serve(
            screenshot(
                &client,
                Some(&Target::Name("TextEdit".into())),
                None,
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
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
                elements(&client, &target, &filter, None, 10, false),
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
    let row = app_row(json!({
        "name": "Éditeur de texte",
        "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    }));
    assert!(app_named(&row, "éditeur de texte"));
    assert!(app_named(&row, "TextEdit"));
    assert!(
        !app_named(&row, "Contents"),
        "a folder on the way is not a name"
    );
    assert!(!app_named(&app_row(json!({"name": "Finder"})), "TextEdit"));

    let nested = app_row(json!({
        "name": "Simulator",
        "executable": "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator",
    }));

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
        system: None,
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
        if request["op"] == "apps" {
            return Some(
                json!({"ok": true, "starts": {"812":[123,456],"900":[124,0]}, "apps": [
                {"pid": 812, "name": "TextEdit"}, {"pid": 900, "name": "Calculator"}]}),
            );
        }
        assert_eq!(request["op"], "windows", "nothing else is read");
        lists.set(lists.get() + 1);
        let window = |number, pid, owner| {
            json!({"number": number, "pid": pid,
            "owner": owner, "title": null, "layer": 0, "alpha": 1.0,
            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}})
        };
        let listed = json!({"ok": true, "starts": {"812":[123,456],"900":[124,0]}, "windows": [
            window(4120, 812, "TextEdit"), window(339, 900, "Calculator")]});
        Some(listed)
    }
}

/// What `expected` asks the helper to check, found in a helper showing
/// [`two_apps`], with how many window lists it read.
async fn resolved(expected: Option<Expected>) -> (Result<Expect>, usize) {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
        json!({"pid": 812, "start": [123, 456], "window": 4120, "field": "Email"}),
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
        json!({"pid": 812, "start": [123, 456], "field": "Email"})
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
        Batch::decode(&actions).unwrap().check(&display()).unwrap();
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
        Batch::decode(&json!([{ "type": "type", "text": batch }]))
            .unwrap()
            .check(&display())
            .unwrap();
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
        Batch::decode(&Value::Array(parts))
            .unwrap()
            .check(&display())
            .unwrap();
    }
}

/// The report of typing `batches` batches of 64 characters with no command
/// under way, the first `confirmed` of which the helper confirms before
/// the next ends with `failure`.
async fn stopped_at(batches: usize, confirmed: usize, failure: Result<Value>) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"z".repeat(64 * batches)).unwrap();
    let mut failure = Some(failure);
    let mut sent = 0;
    type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        sent += 1;
        if sent > confirmed {
            return failure.take().unwrap();
        }
        Ok(json!({"ok": true, "performed": actions.as_array().unwrap().len()}))
    })
    .await
}

#[tokio::test]
async fn a_stopped_type_counts_the_parts_the_helper_performed() {
    let refused = json!({"ok": false, "code": "focus-changed", "reason": "app",
        "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
        "holder": "person", "unread": "slow",
        "performed": 2, "partial": false, "uncertain": false, "cleanupError": "kept out",
        "secret": "unrelated-refusal-secret-4b72", "id": "job", "task": "task", "state": "running"});

    let report = stopped_at(3, 1, Ok(refused.clone())).await;
    assert_eq!(report["typed"], 64 + 32);
    assert_eq!(report["batches"], 3);
    assert_eq!(report["confirmedBatches"], 1);
    assert_eq!(report["partial"], true);
    assert_eq!(report["uncertain"], false);
    assert_eq!(report["code"], "focus-changed");
    assert_eq!(report["error"], "Calculator has keyboard focus");
    assert_eq!(report["reason"], "app");
    assert_eq!(report["found"], refused["found"]);
    assert_eq!(
        report,
        json!({"ok": false, "code": "focus-changed", "reason": "app",
            "found": {"app": "Calculator", "pid": 900}, "error": "Calculator has keyboard focus",
            "holder": "person", "unread": "slow", "uncertain": false,
            "typed": 96, "batches": 3, "confirmedBatches": 1, "partial": true})
    );
    assert!(
        report.get("cleanupError").is_none(),
        "only what agents act on"
    );

    let first = stopped_at(1, 0, Ok(json!({"performed": 0, "code": "focus-changed"}))).await;
    assert_eq!(
        (first["typed"].clone(), first["partial"].clone()),
        (json!(0), json!(false))
    );
    let broke = json!({"performed": 0, "partial": true, "uncertain": true});
    assert_eq!(stopped_at(1, 0, Ok(broke)).await["partial"], true);

    let left = json!({"ok": false, "code": "focus-changed", "reason": "field",
        "found": {"app": "TextEdit", "pid": 812, "window": 4120,
            "field": {"role": "AXTextField", "name": "Second"}},
        "error": "The field with keyboard focus is not the one named First",
        "performed": 0, "partial": false, "uncertain": false});
    let report = stopped_at(4, 2, Ok(left.clone())).await;
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
    let report = stopped_at(2, 1, Err(lost)).await;
    assert_eq!(report["typed"], 64, "no part of a failed batch counts");
    assert_eq!(report["code"], "transport");
    assert_eq!(report["uncertain"], true);
    assert!(report.get("reason").is_none() && report.get("found").is_none());

    let busy = Error::new(Code::Busy, "held").held_by(Holder::Agent);
    assert_eq!(stopped_at(1, 0, Err(busy)).await["holder"], "agent");
    let busy = json!({"code": "busy", "holder": "person"});
    assert_eq!(stopped_at(1, 0, Ok(busy)).await["holder"], "person");

    let unknown = stopped_at(1, 0, Ok(json!({"code": "made-up"}))).await;
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
        let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
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
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
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
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
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
        interrupt.ask_for_test(Signal::SIGTERM);
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

    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let batches = type_batches(&"x".repeat(200)).unwrap();
    assert_eq!(batches.len(), 4);
    let mut sent = 0;

    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
        sent += 1;
        interrupt.ask_for_test(Signal::SIGTERM);
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

    let interrupt = Interrupt::new_for_test(Duration::from_secs(10));
    let mut first = true;
    let report = type_recorded(&client, &batches, &interrupt, async |actions: &Value| {
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

    let frame = Bounds {
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
    element.frame = Some(Bounds {
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

#[test]
fn disabled_controls_refuse_without_guessing_native_panel_identity() {
    for name in ["Save", "Open", "Export"] {
        let mut element = shown(0, "AXButton", Some(name), None);
        element.enabled = Some(false);
        let error = choose(&[element], &named(name)).unwrap_err();
        assert_eq!(
            json!(error),
            json!({
                "ok":false,"code":"input","uncertain":false,
                "error":"The selected control is disabled; no click was sent",
                "next":"Choose an enabled control. If this is Save/Open, cancel with Escape and use atlas run to create files or atlas open to open them; do not retry an inert confirmation."
            })
        );
    }
}

#[test]
fn enabled_and_unknown_controls_including_save_and_sheets_remain_selectable() {
    for role in ["AXButton", "AXSheet", "AXMenuItem"] {
        for enabled in [Some(true), None] {
            let mut element = shown(0, role, Some("Save"), None);
            element.enabled = enabled;
            assert_eq!(
                choose(std::slice::from_ref(&element), &named("Save")).unwrap(),
                &element
            );
        }
    }
}

#[test]
fn a_disabled_match_does_not_hide_ambiguity_or_unreadable_names() {
    let mut disabled = shown(0, "AXButton", Some("Save"), None);
    disabled.enabled = Some(false);
    let other = shown(1, "AXButton", Some("Save"), None);
    let error = choose(&[disabled.clone(), other], &named("Save")).unwrap_err();
    assert_eq!(error.code, Code::Elements);
    let mut unread = shown(1, "AXButton", None, None);
    unread.unreadable.push("AXTitle".into());
    let error = choose(&[disabled, unread], &named("Save")).unwrap_err();
    assert_eq!(error.code, Code::Elements);
}

#[tokio::test]
async fn a_disabled_named_control_refuses_before_any_input_or_native_preparation() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut reads = 0;
    let error = helper
        .serve(
            click_element(
                &client,
                &Target::Pid(i64::from(std::process::id())),
                &named("Save"),
                Button::Left,
                1,
                &[],
            ),
            |request| {
                if request["op"] == "apps" {
                    assert_eq!(reads, 0, "no app lookup after scanning");
                    return Some(json!({"ok": true, "apps": [{"pid": std::process::id(), "name": "TextEdit"}],
                        "starts": {std::process::id().to_string(): [123, 456]}}));
                }
                assert_eq!(
                    request["op"], "run",
                    "no input or other mutation: {request}"
                );
                assert_eq!(request["argv"][1], "elements-info", "no native preparation");
                reads += 1;
                let mut element = shown(0, "AXButton", Some("Save"), None);
                element.enabled = Some(false);
                Some(completed(
                    &json!({"ok":true,"elements":[element],"truncated":false}).to_string(),
                ))
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        json!(error),
        json!({
            "ok":false,"code":"input","uncertain":false,
            "error":"The selected control is disabled; no click was sent",
            "next":"Choose an enabled control. If this is Save/Open, cancel with Escape and use atlas run to create files or atlas open to open them; do not retry an inert confirmation."
        })
    );
    assert_eq!(reads, 1);
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire_beside_commands().is_ok());
}

#[tokio::test]
async fn a_named_click_keeps_an_unknown_mutation_pending_instead_of_scanning_or_posting() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let abandoned = client.controller.quarantine().unwrap();
    let generation = abandoned.to_string();
    drop(abandoned);
    let error = click_element(
        &client,
        &Target::Pid(812),
        &named("Save"),
        Button::Left,
        1,
        &[],
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Uncertain);
    assert_eq!(client.controller.pending().unwrap(), vec![generation]);
}

/// A scan as the probe reports it, complete.
fn scanned(elements: Vec<Element>) -> Scanned {
    Scanned {
        look: None,
        elements,
        incomplete: false,
        coverage_told: true,
        reasons: Vec::new(),
        errors: BTreeMap::new(),
    }
}

fn frame(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
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

    let mut bare = element("AXButton", None, None);
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
                {"index": 19, "role": "AXButton"},
            ],
            "windows": [
                {"x": 10, "y": 38, "width": 800, "height": 600},
                {"x": 900, "y": 0, "width": 300, "height": 200},
            ],
        })
    );
}

/// A kept look adds its name to the listing and nothing to its elements:
/// a reference is that name and an element's index.
#[test]
fn a_kept_look_adds_only_its_name_to_the_listing() {
    let elements = vec![
        shown(3, "AXButton", Some("Save"), None),
        shown(12, "AXButton", Some("Open"), None),
    ];
    let mut scan = scanned(elements.clone());
    scan.look = Look::parse("8123abc");
    for full in [false, true] {
        let mut kept = listing(812, &scan, &ElementFilter::default(), 50, full);
        let plain = scanned(elements.clone());
        let unkept = listing(812, &plain, &ElementFilter::default(), 50, full);
        assert_eq!(kept["look"], "8123abc");
        assert_eq!(kept["elements"][1]["index"], 12);
        kept.as_object_mut().unwrap().remove("look");
        assert_eq!(kept, unkept, "full: {full}");
    }
}

/// A reference that is not one a listing gives is refused before control
/// is taken or the helper is asked anything.
#[tokio::test]
async fn a_malformed_reference_is_refused_before_anything_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    for reference in ["", "8123abc", "8123abc.012", "8123ABC.1", "save"] {
        let error = click_reference(&client, None, reference, Button::Left, 1, &[])
            .await
            .unwrap_err();
        assert_eq!(
            json!(error),
            json!({"ok": false, "code": "elements", "uncertain": false,
                "error": "That is not a ref from atlas elements",
                "next": "Run atlas elements again.", "performed": 0}),
            "{reference:?}"
        );
    }
    assert!(client.controller.pending().unwrap().is_empty());
    assert!(client.controller.acquire_beside_commands().is_ok());
}

/// A referenced click is sent as one referenced action for the input
/// service to find again, with the app's expectation only when one is named.
#[tokio::test]
async fn a_referenced_click_asks_the_input_service_to_find_its_element_again() {
    use crate::test_support::Helper;
    use atlas_core::{
        session::{FOCUS_DOCK, FOCUS_FIELD, FOCUS_WINDOW},
        test_support::UID,
    };

    for target in [None, Some(Target::Pid(812))] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.serving_input(
            &client,
            &[LOOKS, FOCUS_APP, FOCUS_DOCK, FOCUS_WINDOW, FOCUS_FIELD],
        );
        let mut sent = Vec::new();
        let reply = helper
            .serve(
                click_reference(&client, target.as_ref(), "8123abc.12", Button::Left, 2, &[]),
                |request| match request["op"].as_str() {
                    Some("acquire" | "release") => Some(json!({"ok": true})),
                    Some("input") => {
                        sent.push(request.clone());
                        let refused = json!({"ok": false, "code": "elements",
                            "error": "That look is no longer kept", "uncertain": false,
                            "next": "Run atlas elements again.", "performed": 0,
                            "partial": false});
                        Some(refused)
                    }
                    _ => textedit_reads(request),
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["code"], "elements");
        assert_eq!(reply["next"], "Run atlas elements again.");
        assert_eq!(reply["performed"], 0);
        let [request] = sent.as_slice() else {
            panic!("one input request: {sent:?}");
        };
        assert_eq!(
            request["actions"],
            json!([{"type": "reference", "ref": "8123abc.12", "button": "left", "count": 2,
                "modifiers": []}])
        );
        let expected = target
            .as_ref()
            .map_or(Value::Null, |_| json!({"pid": 812, "start": [123, 456]}));
        assert_eq!(
            request.get("expect").cloned().unwrap_or(Value::Null),
            expected
        );
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

/// An action on an element goes to the input service as the one action
/// it is, and its reply says only how many were performed, how, and how
/// it reads now. A service that cannot act on elements is never sent one.
#[tokio::test]
async fn an_element_action_is_sent_whole_and_answered_briefly() {
    use crate::test_support::Helper;
    use atlas_core::{session::ELEMENT_ACTIONS, test_support::UID};

    let action = json!({"type": "set-value", "ref": "8123abc.12", "value": "draft"});
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS, ELEMENT_ACTIONS]);
    let mut sent = Vec::new();
    let reply = helper
        .serve(
            super::element(&client, None, &action),
            |request| match request["op"].as_str() {
                Some("acquire" | "release") => Some(json!({"ok": true})),
                Some("input") => {
                    sent.push(request.clone());
                    Some(json!({"ok": true, "performed": 1, "app": "TextEdit",
                        "bundleId": "com.apple.TextEdit", "mechanism": "accessibility",
                        "verification": "mismatch"}))
                }
                _ => textedit_reads(request),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        reply,
        json!({"ok": true, "performed": 1, "mechanism": "accessibility",
            "verification": "mismatch"})
    );
    let [request] = sent.as_slice() else {
        panic!("one input request: {sent:?}");
    };
    assert_eq!(request["actions"], json!([action]));
    assert!(client.controller.pending().unwrap().is_empty());

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    let error = helper
        .serve(super::element(&client, None, &action), |request| {
            assert_ne!(request["op"], "input", "nothing is sent");
            match request["op"].as_str() {
                Some("acquire" | "release") => Some(json!({"ok": true})),
                _ => textedit_reads(request),
            }
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Startup);
    assert!(!error.uncertain);
}

/// A pick's option must be a ref too, and an action that sends events is
/// no action on an element: both are refused before anything is asked.
#[tokio::test]
async fn a_malformed_option_or_a_physical_action_is_refused_before_anything_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let action = json!({"type": "pick", "ref": "8123abc.12", "option": "Large"});
    let error = super::element(&client, None, &action).await.unwrap_err();
    assert_eq!(error.code, Code::Elements);
    assert_eq!(error.details["performed"], 0);

    for physical in [
        json!({"type": "key", "key": "cmd+q"}),
        json!({"type": "reference", "ref": "8123abc.12"}),
    ] {
        let error = super::element(&client, None, &physical).await.unwrap_err();
        assert_eq!(error.code, Code::Input, "{physical}");
    }
    assert!(client.controller.pending().unwrap().is_empty());
}

#[test]
fn the_default_listing_omits_only_groups_without_information() {
    let bare = shown(0, "AXGroup", None, None);
    let mut groups = vec![bare.clone()];
    let mut empty = bare.clone();
    empty.index = 1;
    empty.title = Some(String::new());
    empty.value = Some(String::new());
    groups.push(empty);
    for index in 2..13 {
        let mut group = bare.clone();
        group.index = index;
        match index {
            2 => group.title = Some("Section".into()),
            3 => group.description = Some("Settings".into()),
            4 => group.identifier = Some("panel".into()),
            5 => group.value = Some("0".into()),
            6 => group.focused = Some(true),
            7 => group.selected = Some(true),
            8 => group.unreadable = vec!["AXTitle".into()],
            9 => group.truncated = vec!["AXDescription".into()],
            10 => group.enabled = Some(false),
            11 => group.menu = true,
            _ => group.mark = Some("✓".into()),
        }
        groups.push(group);
    }
    groups.push(shown(13, "AXButton", None, None));
    let scan = scanned(groups);
    let all = ElementFilter::default();
    let listed = listing(1, &scan, &all, 50, false);
    let indices: Vec<_> = listed["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|element| element["index"].as_u64().unwrap())
        .collect();
    assert_eq!(indices, (2..14).collect::<Vec<_>>());
    assert_eq!(listed["scanned"], 14);
    assert_eq!(listed["matching"], 12);
    let short = listing(1, &scan, &all, 2, false);
    assert_eq!(short["more"], 10);
    assert_eq!(
        short["elements"],
        json!(&listed["elements"].as_array().unwrap()[..2])
    );
    assert_eq!(
        listing(1, &scan, &all, 50, true)["elements"],
        json!(scan.elements)
    );
    assert_eq!(
        listing(
            1,
            &scan,
            &ElementFilter {
                role: Some("AXGroup".into()),
                name: None
            },
            50,
            false
        )["elements"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert_eq!(
        listing(1, &scan, &named("panel"), 50, false)["elements"][0]["index"],
        4
    );
}

#[test]
fn empty_groups_do_not_use_the_limit_or_hide_later_controls() {
    let scan = scanned(vec![
        shown(0, "AXGroup", None, None),
        shown(1, "AXButton", Some("Save"), None),
        shown(2, "AXGroup", None, None),
        shown(3, "AXButton", Some("Cancel"), None),
        shown(4, "AXGroup", None, None),
    ]);
    for (max, indices, more) in [(0, vec![], 2), (1, vec![1], 1), (2, vec![1, 3], 0)] {
        let listed = listing(1, &scan, &ElementFilter::default(), max, false);
        assert_eq!(listed["matching"], 2);
        assert_eq!(listed["more"], more);
        assert_eq!(
            listed["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|element| element["index"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            indices
        );
    }
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
        Scanned::from_reply(reply).unwrap()
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
    let scan = Scanned::from_reply(probe).unwrap();

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

    let frame = Bounds {
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

    let frame = |x: f64| Bounds {
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
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

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
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::Helper,
    };
    use atlas_core::{files::Token, test_support::UID};
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
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

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
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

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
        host_session::{Host, HostSession},
        task::{Claims, Owned, State, Task, store::Store},
        test_support::{Helper, completed, exited},
    };
    use atlas_core::files::Token;

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
    use crate::test_support::{Helper, completed, exited};
    use atlas_core::test_support::UID;

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
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

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
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

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
        test_support::{Helper, completed},
    };
    use atlas_core::test_support::UID;

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
    let apps = |mut second: Value| {
        second["pid"] = json!(2);
        crate::client::AppList::decode(json!({"ok": true, "apps": [
            {"pid": 1, "name": "TextEdit", "bundleId": "com.apple.TextEdit"}, second
        ]}))
        .unwrap()
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

#[test]
fn a_covered_named_element_requires_its_exact_window_at_the_native_boundary() {
    let mut target = shown(0, "AXTextField", None, Some("Email"));
    target.window = Some(frame(10.0, 20.0, 300.0, 200.0));
    let own = window(42, 812, "Editor", 0);
    let cover = window(43, 900, "Other", 0);
    let check = element_check(
        &target,
        &[cover, own],
        Expect::new(812).unwrap(),
        (20.0, 40.0),
    )
    .unwrap();
    let expect = check;
    assert_eq!(expect.pid(), 812);
    assert_eq!(expect.window(), Some(42));
}

#[test]
fn missing_or_ambiguous_owning_windows_refuse_named_clicks() {
    let mut target = shown(0, "AXButton", Some("Save"), None);
    target.window = Some(frame(10.0, 20.0, 300.0, 200.0));
    for windows in [
        vec![],
        vec![window(42, 812, "Editor", 0), window(43, 812, "Editor", 0)],
    ] {
        let error =
            element_check(&target, &windows, Expect::new(812).unwrap(), (20.0, 40.0)).unwrap_err();
        assert_eq!(error.code, Code::Elements);
    }
}

#[test]
fn menu_titles_and_open_items_require_the_app_without_raising_a_menu_window() {
    let mut title = shown(0, "AXMenuBarItem", Some("File"), None);
    let bar = window(2, 1, "Window Server", MENU_BAR_LAYER);
    title.window = Some(frame(10.0, 20.0, 300.0, 200.0));
    let check = element_check(&title, &[bar], Expect::new(812).unwrap(), (20.0, 21.0)).unwrap();
    assert!(check.window().is_none() && check.pid() == 812);
    let mut item = shown(0, "AXMenuItem", Some("Save"), None);
    item.menu = true;
    item.window = title.window;
    let check = element_check(
        &item,
        &[window(3, 812, "Editor", MENU_LAYER)],
        Expect::new(812).unwrap(),
        (20.0, 21.0),
    )
    .unwrap();
    assert!(check.window().is_none());
}

#[tokio::test]
async fn incomplete_or_ambiguous_element_scans_never_dispatch_input() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;
    for incomplete in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut reads = 0;
        let error = helper
            .serve(
                click_element(
                    &client,
                    &Target::Pid(i64::from(std::process::id())),
                    &named("Save"),
                    Button::Left,
                    1,
                    &[],
                ),
                |request| {
                    if request["argv"][1] == "elements-info" {
                        reads += 1;
                        let elements = vec![
                            shown(0, "AXButton", Some("Save"), None),
                            shown(1, "AXButton", Some("Save"), None),
                        ];
                        Some(completed(
                            &json!({"ok":true,"elements":elements,"truncated":incomplete})
                                .to_string(),
                        ))
                    } else if request["op"] == "apps" {
                        Some(json!({"ok": true, "apps": [{"pid": std::process::id(), "name": "TextEdit"}],
                            "starts": {std::process::id().to_string(): [123, 456]}}))
                    } else {
                        textedit_reads(request)
                    }
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Elements);
        assert_eq!(reads, 1);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}

#[test]
fn a_named_sheet_button_keeps_the_exact_sheet_or_parent_number_from_the_scan() {
    let mut button = shown(0, "AXButton", Some("OK"), None);
    button.frame = Some(frame(100.0, 120.0, 60.0, 24.0));
    let parent = window(42, 812, "TextEdit", 0);
    let sheet = Window {
        number: 43,
        bounds: Bounds {
            x: 0.0,
            y: 40.0,
            width: 500.0,
            height: 180.0,
        },
        ..parent.clone()
    };
    for (owning, listed, number) in [
        (
            frame(0.0, 40.0, 500.0, 180.0),
            vec![sheet, parent.clone()],
            43,
        ),
        (frame(10.0, 20.0, 300.0, 200.0), vec![parent], 42),
    ] {
        button.window = Some(owning);
        let check =
            element_check(&button, &listed, Expect::new(812).unwrap(), (130.0, 132.0)).unwrap();
        assert!(check.pid() == 812 && check.window() == Some(number));
    }
}

#[tokio::test]
async fn pictures_compare_two_readings_and_window_lists_remember_their_own_reading() {
    use crate::{
        client::Size,
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::{protocol::Crop, test_support::UID};
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::cell::RefCell;

    for named in [false, true] {
        for command in 0..4 {
            for change in 0..10 {
                let directory = tempfile::tempdir().unwrap();
                let (mut client, helper) = Helper::client(directory.path(), UID);
                if named {
                    client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
                }
                let helper = if command == 1 {
                    helper.serving_input(&client, &[LOOKS])
                } else {
                    helper
                };
                let reads = RefCell::new(Vec::new());
                let reply = helper.serve(async {
                    match command {
                        0 => client.capture(Size::Full, Some(&Crop::App { pid: 812 }), false).await,
                        1 => elements(&client, &Target::Pid(812), &ElementFilter::default(), None, 10, false).await,
                        2 => list(&client, &WindowFilter::default()).await.map(|windows| json!({"ok": true, "windows": windows})),
                        _ => client.capture(Size::Full, None, false).await,
                    }
                }, |request| {
                    if request["op"] == "screenshot" {
                        let mut reply = json!({
                            "ok": true, "png": STANDARD.encode(atlas_native::native::capture::sample_png(10, 10)),
                            "width": 10, "height": 10,
                        });
                        if request["pid"] == 812 {
                            reply["region"] = json!({"x": 0, "y": 0, "width": 10, "height": 10});
                            reply["app"] = json!({"pid": 812, "name": "Editor"});
                            reply["overlapping"] = json!([]);
                        }
                        return Some(reply);
                    }
                    if request["op"] == "apps" {
                        return Some(json!({"ok": true, "apps": [{"pid": 812, "name": "Editor"}], "starts": {"812": [123,456]}}));
                    }
                    if request["op"] == "elements" {
                        return Some(json!({"ok": true, "look": "8123abc", "elements": [], "truncated": false}));
                    }
                    assert_eq!(request["op"], "windows");
                    let dock = request["dock"].as_bool().unwrap();
                    reads.borrow_mut().push(dock);
                    if (dock && change == 6) || (!dock && change == 3 && named) {
                        return Some(json!({"ok": false, "code": "windows", "error": "The window list is unavailable"}));
                    }
                    let mut windows = vec![window(42, 812, "Editor", 0), window(43, 812, "Editor", 0)];
                    if !dock && named {
                        match change {
                            1 => windows.reverse(),
                            2 => windows[0].bounds.x += 1.0,
                            4 => windows[0].title = Some("A new title".into()),
                            5 => windows[0].alpha = 0.0,
                            7 => windows[0].alpha = 0.5,
                            8 => windows.insert(0, window(99, 900, "SystemUIServer", 25)),
                            9 => windows.insert(0, window(100, 812, "Editor", atlas_core::screen::windows::MENU_LAYER)),
                            _ => {}
                        }
                    }
                    Some(json!({"ok": true, "windows": windows,
                        "starts": {"812": [123,456]}, "dock": "unread"}))
                }).await;
                if named && (change == 6 || (change == 3 && command != 2)) {
                    assert!(reply.is_err());
                    assert_eq!(
                        *reads.borrow(),
                        if change == 6 {
                            vec![true]
                        } else {
                            vec![true, false]
                        }
                    );
                    if command == 2 {
                        assert_eq!(helper.proofs(), 1);
                    }
                    assert!(client.remembered_windows().is_none());
                    continue;
                }
                let reply = reply.unwrap();
                assert_eq!(reply["ok"], true, "command {command}, change {change}");
                let expected = if command == 2 {
                    vec![named]
                } else if named {
                    vec![true, false]
                } else {
                    vec![]
                };
                assert_eq!(*reads.borrow(), expected);
                if command == 2 {
                    assert_eq!(helper.proofs(), 1);
                    assert_eq!(
                        reply["windows"],
                        json!([window(42, 812, "Editor", 0), window(43, 812, "Editor", 0)])
                    );
                }
                if named {
                    let memory = client.remembered_windows().unwrap();
                    assert_eq!(
                        memory.windows.is_none(),
                        command != 2 && !matches!(change, 0 | 4 | 7)
                    );
                } else {
                    assert!(client.remembered_windows().is_none());
                }
            }
        }
    }
}

#[tokio::test]
async fn a_changed_look_refuses_point_actions_before_control_is_acquired() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for change in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        client.controller = client.controller.waiting(Duration::ZERO);
        helper
            .serve(
                async {
                    let verified = client.verify().await.unwrap();
                    client.look(&verified, async { Ok(()) }).await.unwrap();
                    let _ownership = client.controller.acquire(false).unwrap();
                    for actions in [
                        click(30.0, 40.0, Button::Left, 1, &[]),
                        scroll((30.0, 40.0), (0, 1)),
                        drag((30.0, 40.0), (50.0, 50.0), Button::Left),
                        move_to(30.0, 40.0),
                    ] {
                        let error = act(&client, None, &actions).await.unwrap_err();
                        assert_eq!(error.code, Code::Windows);
                        assert_eq!(error.error, "The windows changed while you looked; look again");
                    }
                },
                |request| {
                    assert_eq!(request["op"], "windows");
                    let mut windows = vec![window(42, 812, "Editor", 0), window(43, 812, "Editor", 0)];
                    if request["dock"] == false {
                        match change {
                            0 => windows[0].bounds.x += 1.0,
                            1 => windows.reverse(),
                            _ => windows.insert(0, window(99, 812, "Editor", 0)),
                        }
                    }
                    Some(json!({"ok":true,"windows":windows,"starts":{"812":[123,456]},"dock":"unread"}))
                },
            )
            .await;
    }
}

#[tokio::test]
async fn a_failed_window_read_preserves_memory_and_stops_the_look() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::{screen::windows::HitWindow, test_support::UID};
    use std::cell::Cell;

    for failed_read in [1, 2] {
        for refused in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let (mut client, helper) = Helper::client(directory.path(), UID);
            client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
            let verified = helper
                .serve(client.verify(), |_| {
                    panic!("only identity reads are needed")
                })
                .await
                .unwrap();
            client.remember_windows(
                &[HitWindow {
                    number: 42,
                    pid: 812,
                    start: Some([123, 456]),
                    layer: 0,
                    bounds: Bounds::display(100.0, 100.0),
                }],
                &verified.session().epoch,
            );
            let looked = Cell::new(false);
            let mut reads = 0;
            let result = helper
                .serve(
                    client.look(&verified, async {
                        looked.set(true);
                        Ok(())
                    }),
                    |request| {
                        assert_eq!(request["op"], "windows");
                        reads += 1;
                        if reads == failed_read {
                            return refused.then(
                                || json!({"ok": false, "code": "windows", "error": "Unavailable"}),
                            );
                        }
                        Some(json!({"ok": true, "windows": [], "starts": {}, "dock": "unread"}))
                    },
                )
                .await;
            assert!(result.is_err());
            assert_eq!(reads, failed_read);
            assert_eq!(looked.get(), failed_read == 2);
            assert_eq!(
                client.remembered_windows().unwrap().windows.unwrap()[0].number,
                42
            );
        }
    }
}

#[tokio::test]
async fn looks_and_window_lists_fail_when_window_memory_cannot_be_replaced() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;
    use std::os::unix::fs::PermissionsExt;

    for listing in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let verified = helper
            .serve(client.verify(), |_| {
                panic!("only identity reads are needed")
            })
            .await
            .unwrap();
        let output = client.output_dir().unwrap();
        client.remember_windows(&[], &verified.session().epoch);
        let observations = output.join("last-windows");
        std::fs::set_permissions(&observations, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = helper
            .serve(
                async {
                    if listing {
                        list(&client, &WindowFilter::default()).await.map(|_| ())
                    } else {
                        client.look(&verified, async { Ok(()) }).await
                    }
                },
                |request| {
                    assert_eq!(request["op"], "windows");
                    Some(json!({"ok": true, "windows": [], "starts": {}, "dock": "unread"}))
                },
            )
            .await;
        std::fs::set_permissions(&observations, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err(), "listing: {listing}");
    }
}

#[test]
fn element_reply_cost_preserves_listing_and_refusal_words() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/elements.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let scanned = Scanned::from_reply(fixture["input"].clone()).unwrap();
        let listed = listing(
            812,
            &scanned,
            &ElementFilter::default(),
            fixture["max"].as_u64().unwrap() as usize,
            fixture["full"] == true,
        );
        atlas_core::test_support::reply_cost(&listed, &fixture);
    }
    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/choose.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let elements: Vec<Element> = serde_json::from_value(fixture["input"].clone()).unwrap();
        let error = choose(&elements, &named(fixture["name"].as_str().unwrap())).unwrap_err();
        atlas_core::test_support::reply_cost(&serde_json::to_value(error).unwrap(), &fixture);
    }
}

#[tokio::test]
async fn representative_element_reads_keep_their_helper_request_counts() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let fixtures: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/reply-cost/elements.json"
    ))
    .unwrap();
    for fixture in fixtures {
        let dir = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(dir.path(), UID);
        let helper = helper.serving_input(&client, &[LOOKS]);
        let mut operations = Vec::new();
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Name("TextEdit".into()),
                    &ElementFilter::default(),
                    None,
                    fixture["max"].as_u64().unwrap() as usize,
                    fixture["full"] == true,
                ),
                |request| {
                    operations.push(request["op"].as_str().unwrap().to_owned());
                    if request["op"] == "elements" {
                        Some(fixture["input"].clone())
                    } else {
                        textedit_reads(request)
                    }
                },
            )
            .await
            .unwrap();
        atlas_core::test_support::reply_cost(&listed, &fixture);
        assert_eq!(json!(operations), fixture["helperOperations"]);
        assert_eq!(json!(helper.proofs()), fixture["helperProofs"]);
        assert_eq!(json!(helper.requests()), fixture["helperRequests"]);
    }
}

fn app_row(mut fields: Value) -> crate::client::AppRow {
    fields["pid"] = json!(1);
    crate::client::AppList::decode(json!({"ok": true, "apps": [fields]}))
        .unwrap()
        .rows()[0]
        .clone()
}

#[test]
fn target_selection_pins_the_listed_process_start() {
    let apps = crate::client::AppList::decode(json!({"ok": true,
        "apps": [{"pid": 812, "name": "Editor", "active": true}],
        "starts": {"812": [123, 456]}}))
    .unwrap();
    let app = named_app(&apps, "editor").unwrap();
    let expect = snapshot_expect(app.pid() as i64, apps.starts().unwrap()).unwrap();
    assert_eq!(expect.pid(), 812);
    assert_eq!(
        expect.instance(),
        Some(atlas_core::process::Instance::new(812, [123, 456]))
    );
}

#[tokio::test]
async fn app_targets_keep_valid_process_identities_and_refuse_absent_or_malformed_ones() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    for target in [Target::Pid(812), Target::Name("Editor".into())] {
        for starts in [
            None,
            Some(json!({"812": [123, 456]})),
            Some(Value::Null),
            Some(json!({"812": [123]})),
            Some(json!({})),
            Some(json!({"812": [0, 0]})),
        ] {
            let mut reply = json!({"ok": true, "apps": [
                {"pid": 812, "name": "Editor", "active": true}
            ]});
            if let Some(starts) = &starts {
                reply["starts"] = starts.clone();
            }
            let result = helper
                .serve(
                    async {
                        let verified = client.verify().await?;
                        target_among(&client, &verified, &target, None).await
                    },
                    |_| Some(reply.clone()),
                )
                .await;
            match starts {
                Some(value) if value == json!({"812": [123, 456]}) => {
                    assert_eq!(
                        result.unwrap().instance(),
                        Some(atlas_core::process::Instance::new(812, [123, 456]))
                    );
                }
                Some(value) if value == json!({}) || value == json!({"812": [0, 0]}) => {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, Code::Elements);
                    assert_eq!(
                        error.error,
                        "The app's process changed while it was being resolved; look again"
                    );
                }
                None | Some(_) => {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, Code::Protocol);
                    assert_eq!(
                        error.error,
                        "The helper's process identities were malformed"
                    );
                }
            }
        }
    }
}

/// A Save button as a scan reports it: enabled unless `enabled` says not.
fn save_button(enabled: bool) -> Value {
    json!({"index": 7, "depth": 2, "role": "AXButton", "title": "Save", "enabled": enabled,
        "window": {"x": 0, "y": 0, "width": 100, "height": 100},
        "frame": {"x": 10, "y": 10, "width": 60, "height": 20}})
}

/// TextEdit as `textedit_reads` stands in for it, its probe scans showing
/// `probed` and its kept look `kept`; the looks asked for are counted.
fn waiting_reads(
    probed: Value,
    kept: Value,
    looks: &mut usize,
) -> impl FnMut(&Value) -> Option<Value> {
    use crate::test_support::completed;
    move |request: &Value| {
        if request["op"] == "elements" {
            *looks += 1;
            return Some(
                json!({"ok": true, "look": "8123abc", "elements": kept.clone(),
                "truncated": false}),
            );
        }
        if request["argv"][1] == "elements-info" {
            return Some(completed(
                &json!({"ok": true, "elements": probed.clone(), "truncated": false}).to_string(),
            ));
        }
        textedit_reads(request)
    }
}

fn save() -> ElementFilter {
    ElementFilter {
        role: Some("AXButton".into()),
        name: Some("Save".into()),
    }
}

#[tokio::test]
async fn a_met_wait_names_the_element_by_the_look_the_input_service_kept() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    let mut looks = 0;
    let reply = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Name("TextEdit".into()),
                &save(),
                &State::Enabled,
                Duration::ZERO,
            ),
            waiting_reads(
                json!([save_button(true)]),
                json!([save_button(true)]),
                &mut looks,
            ),
        )
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["state"], "enabled");
    assert_eq!(reply["matched"], true);
    assert_eq!(reply["look"], "8123abc");
    assert_eq!(reply["element"]["index"], 7);
    assert_eq!(reply["element"]["name"], "Save");
    assert!(reply["element"].get("window").is_none());
    assert_eq!(looks, 1, "only the confirming look is kept");
}

#[tokio::test]
async fn a_wait_that_times_out_says_what_its_last_look_saw() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    for (probed, kept, why) in [
        (json!([save_button(false)]), json!([]), "disabled"),
        (json!([]), json!([]), "none"),
        (
            json!([save_button(true)]),
            json!([save_button(false)]),
            "disabled",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.serving_input(&client, &[LOOKS]);
        let mut looks = 0;
        let reply = helper
            .serve(
                wait_for_element(
                    &client,
                    &Target::Pid(812),
                    &save(),
                    &State::Enabled,
                    Duration::ZERO,
                ),
                waiting_reads(probed.clone(), kept.clone(), &mut looks),
            )
            .await
            .unwrap();
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(reply["code"], "wait-failed");
        assert_eq!(reply["timedOut"], true);
        assert_eq!(reply["state"], "enabled");
        assert_eq!(reply["last"]["why"], why, "{probed} then {kept}");
        assert_eq!(
            reply["error"],
            format!(
                "The element was not enabled in time: {}",
                reply["last"]["says"].as_str().unwrap()
            )
        );
        assert_eq!(
            looks,
            usize::from(kept != json!([])),
            "a look is kept only to confirm a state the probe saw"
        );
    }
}

#[tokio::test]
async fn a_gone_wait_keeps_no_look_and_holds_once_the_app_shows_no_window() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    let mut looks = 0;
    let reply = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Gone,
                Duration::ZERO,
            ),
            waiting_reads(json!([]), json!([]), &mut looks),
        )
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(reply.get("look").is_none() && reply.get("element").is_none());
    assert_eq!(looks, 0);

    for (state, ok) in [(State::Gone, true), (State::Exists, false)] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let reply = helper
            .serve(
                wait_for_element(
                    &client,
                    &Target::Name("Calculator".into()),
                    &save(),
                    &state,
                    Duration::ZERO,
                ),
                textedit_reads,
            )
            .await
            .unwrap();
        assert_eq!(reply["ok"], ok, "{state:?}: {reply}");
    }
}

#[tokio::test]
async fn an_element_wait_needs_a_name_or_a_role() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let error = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &ElementFilter::default(),
                &State::Exists,
                Duration::ZERO,
            ),
            |_| panic!("nothing is read"),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Usage);
}

#[tokio::test]
async fn only_an_app_that_shows_nothing_counts_as_gone_and_other_target_failures_end_the_wait() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let error = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Name("TextEdit".into()),
                &save(),
                &State::Gone,
                Duration::ZERO,
            ),
            |request| {
                if request["op"] == "windows" {
                    let window = |number: i64, pid: i64| {
                        json!({"number": number, "pid": pid, "owner": "TextEdit", "title": null,
                            "layer": 0, "alpha": 1.0,
                            "bounds": {"x": 0, "y": 0, "width": 100, "height": 100}})
                    };
                    return Some(json!({"ok": true,
                        "starts": {"812": [123, 456], "813": [124, 456]},
                        "windows": [window(5, 812), window(6, 813)]}));
                }
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert!(
        error.error.contains("Several TextEdit processes"),
        "{error}"
    );
}

#[tokio::test]
async fn a_met_state_is_never_replied_without_a_kept_look_to_confirm_it() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[FOCUS_APP]);
    let mut looks = 0;
    let error = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Enabled,
                Duration::ZERO,
            ),
            waiting_reads(json!([save_button(true)]), json!([]), &mut looks),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        Code::Startup,
        "an input service that keeps no looks"
    );
    assert_eq!(looks, 0);

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    let probed = json!({"ok": true, "elements": [save_button(true)], "truncated": false});
    let error = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Enabled,
                Duration::ZERO,
            ),
            |request| {
                if request["op"] == "elements" {
                    return Some(json!({"ok": true, "elements": [], "truncated": false}));
                }
                if request["argv"][1] == "elements-info" {
                    return Some(crate::test_support::completed(&probed.to_string()));
                }
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Elements);
    assert!(error.error.contains("named no look"), "{error}");
}

#[tokio::test]
async fn a_scan_that_does_not_say_whether_it_was_complete_never_proves_an_element_gone() {
    use crate::test_support::{Helper, completed};
    use atlas_core::{screen::wait::State, test_support::UID};

    for truncated in [json!("true"), json!(null), json!(0)] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let probed = json!({"ok": true, "elements": [], "truncated": truncated});
        let reply = helper
            .serve(
                wait_for_element(
                    &client,
                    &Target::Pid(812),
                    &save(),
                    &State::Gone,
                    Duration::ZERO,
                ),
                |request| {
                    if request["argv"][1] == "elements-info" {
                        return Some(completed(&probed.to_string()));
                    }
                    textedit_reads(request)
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["ok"], false, "{truncated}");
        assert_eq!(reply["last"]["why"], "incomplete", "{truncated}");
    }
}

#[tokio::test]
async fn a_wait_looks_again_until_its_state_holds_in_a_kept_look() {
    use crate::test_support::{Helper, completed};
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let helper = helper.serving_input(&client, &[LOOKS]);
    // The probe sees nothing, then the button; the first kept look sees it
    // disabled, the second enabled.
    let mut probes = 0;
    let mut looks = 0;
    let reply = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Enabled,
                Duration::from_secs(10),
            ),
            |request| {
                if request["op"] == "elements" {
                    looks += 1;
                    let kept = if looks == 1 {
                        save_button(false)
                    } else {
                        save_button(true)
                    };
                    return Some(json!({"ok": true, "look": "8123abc", "elements": [kept],
                        "truncated": false}));
                }
                if request["argv"][1] == "elements-info" {
                    probes += 1;
                    let seen = if probes == 1 {
                        json!([])
                    } else {
                        json!([save_button(true)])
                    };
                    return Some(completed(
                        &json!({"ok": true, "elements": seen, "truncated": false}).to_string(),
                    ));
                }
                textedit_reads(request)
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!((probes, looks), (3, 2));
    assert_eq!(reply["element"]["index"], 7);
}

#[tokio::test]
async fn each_scan_of_a_wait_is_stopped_by_what_is_left_of_its_deadline() {
    use crate::test_support::{Helper, completed};
    use atlas_core::{screen::wait::State, test_support::UID};

    for (timeout, longest) in [
        (Duration::ZERO, 5_000),
        (Duration::from_millis(800), 800),
        (Duration::from_secs(2), 2_000),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut asked = Vec::new();
        let reply = helper
            .serve(
                wait_for_element(&client, &Target::Pid(812), &save(), &State::Gone, timeout),
                |request| {
                    if request["argv"][1] == "elements-info" {
                        asked.push(request["timeoutMs"].as_u64().unwrap());
                        return Some(completed(
                            &json!({"ok": true, "elements": [save_button(true)],
                                "truncated": false})
                            .to_string(),
                        ));
                    }
                    textedit_reads(request)
                },
            )
            .await
            .unwrap();
        assert_eq!(reply["timedOut"], true);
        assert!(!asked.is_empty());
        assert!(
            asked.iter().all(|ms| (500..=longest).contains(ms)),
            "{timeout:?}: {asked:?}"
        );
    }
}

#[tokio::test]
async fn a_scan_its_budget_stopped_keeps_the_wait_going_as_an_incomplete_look() {
    use crate::test_support::Helper;
    use atlas_core::{screen::wait::State, test_support::UID};

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let reply = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Gone,
                Duration::ZERO,
            ),
            |request| {
                if request["argv"][1] == "elements-info" {
                    return Some(json!({"ok": true, "exitCode": null, "timedOut": true,
                        "signal": "SIGKILL", "events": []}));
                }
                textedit_reads(request)
            },
        )
        .await
        .unwrap();
    assert_eq!(reply["code"], "wait-failed", "{reply}");
    assert_eq!(reply["timedOut"], true);
    assert_eq!(reply["last"]["why"], "incomplete");

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let error = helper
        .serve(
            wait_for_element(
                &client,
                &Target::Pid(812),
                &save(),
                &State::Gone,
                Duration::ZERO,
            ),
            |request| {
                if request["argv"][1] == "elements-info" {
                    return Some(crate::test_support::completed("not json"));
                }
                textedit_reads(request)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        Code::Elements,
        "a scan that failed still ends the wait"
    );
}

/// The screenshot TextEdit's stand-in captures: the request it was asked
/// for, refused so that nothing more is needed of the stand-in.
fn captured_asking(asked: &mut Vec<Value>) -> impl FnMut(&Value) -> Option<Value> {
    move |request: &Value| {
        if request["op"] == "screenshot" {
            asked.push(request.clone());
            return Some(json!({"ok": false, "code": "capture", "error": "not in this test"}));
        }
        textedit_reads(request)
    }
}

#[tokio::test]
async fn an_apps_part_of_a_region_is_asked_of_the_helper_and_one_showing_none_is_refused() {
    use crate::test_support::Helper;
    use atlas_core::{protocol::Region, test_support::UID};

    let asked = Region::from_points(50.0, 50.0, 200.0, 200.0).unwrap();
    for shows in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut requests = Vec::new();
        let outcome = helper
            .serve(
                screenshot(
                    &client,
                    Some(&Target::Name("TextEdit".into())),
                    Some(asked),
                    None,
                    crate::client::Size::Standard,
                    false,
                ),
                |request| {
                    if request["op"] == "screenshot" {
                        requests.push(request.clone());
                        return Some(if shows {
                            json!({"ok": false, "code": "capture", "error": "not in this test"})
                        } else {
                            json!({"ok": false, "code": "no-window",
                                "error": "Process 812 has no window on screen"})
                        });
                    }
                    textedit_reads(request)
                },
            )
            .await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["pid"], 812);
        assert_eq!(
            requests[0]["within"],
            json!(asked),
            "the helper cuts it as it captures"
        );
        assert!(requests[0].get("region").is_none());
        let error = outcome.unwrap_err();
        if !shows {
            assert_eq!(error.code, Code::NoWindow);
            assert!(
                error.error.contains("shows none of TextEdit's windows"),
                "{error}"
            );
        }
    }
}

#[tokio::test]
async fn one_window_is_asked_of_the_helper_by_its_number_and_its_owner() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    for target in [Some(Target::Name("TextEdit".into())), None] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let mut requests = Vec::new();
        let outcome = helper
            .serve(
                screenshot(
                    &client,
                    target.as_ref(),
                    None,
                    Some(5),
                    crate::client::Size::Standard,
                    false,
                ),
                captured_asking(&mut requests),
            )
            .await;
        assert!(outcome.is_err(), "the stand-in refuses the capture itself");
        assert_eq!(requests.len(), 1, "{target:?}");
        assert_eq!(
            (requests[0]["pid"].clone(), requests[0]["window"].clone()),
            (json!(812), json!(5))
        );
    }

    // A window no app shows is refused before any capture.
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let mut requests = Vec::new();
    let error = helper
        .serve(
            screenshot(
                &client,
                None,
                None,
                Some(9),
                crate::client::Size::Standard,
                false,
            ),
            captured_asking(&mut requests),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::NoWindow);
    assert!(error.error.contains("atlas windows"), "{error}");
    assert!(requests.is_empty());
}

/// TextEdit's stand-in with two buttons in its one window.
fn two_buttons(request: &Value) -> Option<Value> {
    if request["op"] == "elements" {
        let button = |index: usize, name: &str| {
            json!({"index": index, "depth": 2, "role": "AXButton", "title": name,
                "window": {"x": 0, "y": 0, "width": 100, "height": 100},
                "frame": {"x": 10, "y": 10, "width": 30, "height": 20}})
        };
        return Some(json!({"ok": true, "look": "8123abc",
            "elements": [button(1, "Save"), button(2, "Cancel")], "truncated": false}));
    }
    textedit_reads(request)
}

#[tokio::test]
async fn a_listing_names_its_windows_by_number_and_lists_only_the_window_asked_for() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for window in [None, Some(5), Some(9)] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Name("TextEdit".into()),
                    &ElementFilter::default(),
                    window,
                    50,
                    false,
                ),
                two_buttons,
            )
            .await;
        match window {
            Some(9) => assert_eq!(listed.unwrap_err().code, Code::NoWindow),
            _ => {
                let listed = listed.unwrap();
                assert_eq!(listed["windows"][0]["number"], 5, "{listed}");
                assert_eq!(listed["matching"], 2);
            }
        }
    }
}

/// TextEdit with windows 5 and 6 at these bounds and a button in each.
fn two_windows(first: (f64, f64), second: (f64, f64)) -> impl Fn(&Value) -> Option<Value> {
    move |request: &Value| {
        let frame = |(x, y): (f64, f64)| json!({"x": x, "y": y, "width": 100, "height": 100});
        if request["op"] == "windows" {
            let window = |number: i64, at: (f64, f64)| {
                json!({"number": number, "pid": 812, "owner": "TextEdit", "title": null,
                    "layer": 0, "alpha": 1.0, "bounds": frame(at)})
            };
            return Some(json!({"ok": true, "starts": {"812": [123, 456]},
                "windows": [window(5, first), window(6, second)]}));
        }
        if request["op"] == "elements" {
            let button = |index: usize, at: (f64, f64)| {
                json!({"index": index, "depth": 2, "role": "AXButton", "title": "OK",
                    "window": frame(at),
                    "frame": {"x": at.0 + 10.0, "y": at.1 + 10.0, "width": 30, "height": 20}})
            };
            return Some(json!({"ok": true, "look": "8123abc",
                "elements": [button(1, first), button(2, second)], "truncated": false}));
        }
        textedit_reads(request)
    }
}

#[tokio::test]
async fn a_window_lists_only_its_own_elements_and_one_sharing_its_frame_is_refused() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    for (second, apart) in [((300.0, 0.0), true), ((0.0, 0.0), false)] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Pid(812),
                    &ElementFilter::default(),
                    Some(6),
                    50,
                    false,
                ),
                two_windows((0.0, 0.0), second),
            )
            .await;
        if apart {
            let listed = listed.unwrap();
            assert_eq!(listed["matching"], 1, "{listed}");
            assert_eq!(listed["elements"][0]["index"], 2);
            assert_eq!(listed["windows"][0]["number"], 6);
        } else {
            let error = listed.unwrap_err();
            assert_eq!(error.code, Code::Elements);
            assert!(error.error.contains("cannot be told apart"), "{error}");
        }
    }
}

#[tokio::test]
async fn windows_that_move_during_a_scan_or_share_a_frame_on_another_layer_are_not_told_apart() {
    use crate::{
        host_session::{Host, HostSession},
        test_support::Helper,
    };
    use atlas_core::test_support::UID;

    // The two windows swap places between the reads either side of the
    // scan; or a panel of the app's sits on window 5's frame.
    for case in ["swap", "panel"] {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        client.host_session = Some(HostSession::new(Host::Named, "one").unwrap());
        let helper = helper.serving_input(&client, &[LOOKS]);
        let listed = helper
            .serve(
                elements(
                    &client,
                    &Target::Pid(812),
                    &ElementFilter::default(),
                    Some(5),
                    50,
                    false,
                ),
                |request| {
                    if request["op"] == "windows" {
                        let window = |number: i64, x: f64, layer: i64| {
                            json!({"number": number, "pid": 812, "owner": "TextEdit",
                                "title": null, "layer": layer, "alpha": 1.0,
                                "bounds": {"x": x, "y": 0, "width": 100, "height": 100}})
                        };
                        let before = request["dock"] == true;
                        let windows = match (case, before) {
                            ("swap", true) => vec![window(5, 0.0, 0), window(6, 300.0, 0)],
                            ("swap", false) => vec![window(5, 300.0, 0), window(6, 0.0, 0)],
                            _ => vec![window(6, 0.0, 3), window(5, 0.0, 0)],
                        };
                        return Some(json!({"ok": true, "starts": {"812": [123, 456]},
                            "windows": windows}));
                    }
                    two_windows((0.0, 0.0), (300.0, 0.0))(request)
                },
            )
            .await;
        let error = listed.unwrap_err();
        assert_eq!(error.code, Code::Elements, "{case}");
        let said = if case == "swap" {
            "moved while it was read"
        } else {
            "cannot be told apart"
        };
        assert!(error.error.contains(said), "{case}: {error}");
    }
}

#[tokio::test]
async fn an_older_helper_is_never_asked_for_a_part_it_cannot_crop() {
    use crate::test_support::Helper;
    use atlas_core::{protocol::Region, test_support::UID};

    for (region, window) in [
        (Region::from_points(0.0, 0.0, 50.0, 50.0), None),
        (None, Some(5)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let helper = helper.without_parts();
        let mut requests = Vec::new();
        let error = helper
            .serve(
                screenshot(
                    &client,
                    Some(&Target::Name("TextEdit".into())),
                    region,
                    window,
                    crate::client::Size::Standard,
                    false,
                ),
                captured_asking(&mut requests),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Daemon, "{error}");
        assert!(error.error.contains("cannot capture one window"), "{error}");
        assert!(requests.is_empty(), "nothing is captured");
    }
}

#[test]
fn a_listed_window_is_numbered_by_the_very_frame_the_scan_gave_it() {
    let frame = |x: f64| Bounds {
        x,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    let shown = [5, 6].map(|number| Window {
        bounds: frame(if number == 5 { 99.4 } else { 101.4 }),
        ..window(number, 812, "Editor", 0)
    });
    // Both frames round to 100: only the second, of window 6, is listed.
    let button = |index: usize, name: &str, x: f64| Element {
        index,
        role: Some("AXButton".into()),
        title: Some(name.into()),
        window: Some(frame(x)),
        frame: Some(frame(x + 10.0)),
        ..Element::default()
    };
    let scan = scanned(vec![button(1, "Open", 99.51), button(2, "Save", 100.49)]);
    let listed = listing_in(812, &scan, &named("Save"), 50, false, Some(&shown));
    assert_eq!(listed["windows"][0]["x"], 100, "shown rounded");
    assert_eq!(listed["windows"][0]["number"], 6);
    let both = listing_in(
        812,
        &scan,
        &ElementFilter::default(),
        50,
        false,
        Some(&shown),
    );
    assert_eq!(
        both["windows"].as_array().unwrap().len(),
        1,
        "listed as one window"
    );
    assert!(
        both["windows"][0].get("number").is_none(),
        "its elements are in windows 5 and 6, so it names neither"
    );
    let unnumbered = listing(812, &scan, &named("Save"), 50, false);
    assert!(unnumbered["windows"][0].get("number").is_none());
}

#[test]
fn a_window_filter_refuses_an_element_whose_frame_could_be_another_window() {
    let frame = |x: f64| Bounds {
        x,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };
    for panel_layer in [3, 0] {
        let mut panel = window(6, 812, "Editor", panel_layer);
        panel.bounds = frame(101.5);
        let mut ordinary = window(5, 812, "Editor", 0);
        ordinary.bounds = frame(100.0);
        let shown = [ordinary, panel];
        let element = |index: usize, x: f64| Element {
            index,
            role: Some("AXButton".into()),
            title: Some("OK".into()),
            window: Some(frame(x)),
            frame: Some(frame(x + 10.0)),
            ..Element::default()
        };
        // 100.75 is within a point of both windows.
        let Err(error) =
            scanned(vec![element(1, 100.0), element(2, 100.75)]).in_window(&shown, 812, 5)
        else {
            panic!("an element that could be window 6 is refused");
        };
        assert_eq!(error.code, Code::Elements, "panel at layer {panel_layer}");
        let kept = scanned(vec![element(1, 100.0), element(2, 102.5)])
            .in_window(&shown, 812, 5)
            .unwrap();
        assert_eq!(kept.elements.len(), 1, "only window 5's own element");
    }
}
