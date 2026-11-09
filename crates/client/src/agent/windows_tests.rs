use super::*;
use crate::agent::tests::{app_pid, save, textedit_reads, window};
use atlas_core::{
    screen::windows::{MENU_LAYER, owning_window},
    session::{FOCUS_APP, LOOKS},
};

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
