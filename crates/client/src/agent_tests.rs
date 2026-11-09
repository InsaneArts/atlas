use super::{elements::*, expect::*, input::*, quit::*, target::*, windows::*};
use atlas_core::{
    actions::{Button, Expect},
    error::{Code, Error, Result},
    screen::select::ElementFilter,
    screen::{
        elements::{Element, click_point, element_check},
        windows::{Bounds, MENU_BAR_LAYER, MENU_LAYER, Window, owning_window},
    },
    session::{Display, LOOKS},
};
use serde_json::{Value, json};
use std::time::Duration;

/// The stand-in helper's answer to a read: TextEdit (812) with one
/// window and no elements.
pub(super) fn textedit_reads(request: &Value) -> Option<Value> {
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
                elements(&client, &target, &asked_for(&filter)),
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
pub(super) fn app_pid(windows: &[Window], target: &Target) -> Result<i64> {
    match target {
        Target::Pid(pid) => Ok(*pid),
        Target::Name(name) => one_pid(name, &app_pids(windows, name)),
    }
}

pub(super) fn display() -> Display {
    Display {
        width: 2560.0,
        height: 1440.0,
        scale: 1.0,
    }
}

pub(super) fn window(number: i64, pid: i64, owner: &str, layer: i64) -> Window {
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

pub(super) fn element(role: &str, title: Option<&str>, description: Option<&str>) -> Element {
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

/// An element as the scan reports it, with a frame.
pub(super) fn shown(
    index: usize,
    role: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> Element {
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

/// A listing request for the elements `filter` is for.
pub(super) fn asked_for(filter: &ElementFilter) -> Asked {
    Asked {
        role: filter.role.clone(),
        names: filter.name.iter().cloned().collect(),
        ..Asked::every(10)
    }
}

pub(super) fn named(name: &str) -> ElementFilter {
    ElementFilter {
        role: None,
        name: Some(name.into()),
    }
}

pub(super) fn frame(x: f64, y: f64, width: f64, height: f64) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
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

/// The helper's refusal with `code`.
pub(super) fn refused(code: &str) -> Value {
    json!({"ok": false, "code": code, "error": "Refused", "uncertain": false})
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

    // Made once, before any operation is timed: a process's first PNG
    // starts macOS's image framework, which can take seconds.
    let png = STANDARD.encode(atlas_native::native::capture::sample_png(10, 10));
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
                        1 => elements(&client, &Target::Pid(812), &Asked::every(10)).await,
                        2 => list(&client, &WindowFilter::default()).await.map(|windows| json!({"ok": true, "windows": windows})),
                        _ => client.capture(Size::Full, None, false).await,
                    }
                }, |request| {
                    if request["op"] == "screenshot" {
                        let mut reply = json!({
                            "ok": true, "png": png,
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

pub(super) fn save() -> ElementFilter {
    ElementFilter {
        role: Some("AXButton".into()),
        name: Some("Save".into()),
    }
}
