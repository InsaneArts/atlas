use super::*;
use crate::{
    agent::{click, drag, keys, move_to, scroll},
    host_session::{Host, HostSession},
};
use atlas_core::{
    actions::Button,
    screen::windows::{Bounds, Dock, hit_windows},
};
use serde_json::json;

fn session(client: &Client, name: &str) -> Client {
    let mut client = client.clone();
    client.host_session = Some(HostSession::new(Host::Named, name).unwrap());
    client
}

fn display() -> atlas_core::session::Display {
    atlas_core::session::Display {
        width: 1000.0,
        height: 800.0,
        scale: 1.0,
    }
}

fn window(number: i64, x: f64) -> Window {
    Window {
        number,
        pid: 812,
        owner: Some("Editor".into()),
        title: None,
        layer: 0,
        alpha: 1.0,
        bounds: Bounds {
            x,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        },
    }
}

fn epoch() -> atlas_core::session::Epoch {
    atlas_core::session::Epoch {
        uid: atlas_core::test_support::UID,
        capture_pid: 123,
        input_pid: 0,
        display: display(),
        security_session_id: 42,
        display_id: 7,
    }
}

fn remember(client: &Client, windows: &[Window]) {
    client.remember_windows(
        &hit_windows(
            windows,
            &Bounds::display(display().width, display().height),
            &Dock::Unread,
            &std::collections::BTreeMap::from([(812, [123, 456])]),
        ),
        &epoch(),
    );
}

fn remembered(
    client: &Client,
    expected: Option<&Expected>,
    actions: &Value,
) -> Result<Option<(HitWindow, Value)>> {
    let point = actions.as_array().and_then(|list| match list.as_slice() {
        [action] => actions::Kind::deserialize(action)
            .ok()?
            .remembered_point(action),
        _ => None,
    });
    Expected::remembered(client, expected, point)
}

fn number(client: &Client, expected: Option<&Expected>, actions: &Value) -> Option<i64> {
    remembered(client, expected, actions)
        .unwrap()
        .map(|(window, _)| window.number)
        .or_else(|| match &expected?.place {
            Place::Window { number, .. } => Some(*number),
            Place::App(_) => None,
        })
}

#[test]
fn pointer_phases_do_not_acquire_new_remembered_targets() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    for phase in ["down", "move", "up"] {
        assert_eq!(
            number(
                &client,
                None,
                &json!([{"type":"pointer","phase":phase,"x":30,"y":40}])
            ),
            None
        );
    }
    assert_eq!(
        number(&client, None, &json!([{"type":"wait","ms":0}])),
        None
    );
    assert_eq!(
        number(
            &client,
            None,
            &json!([{"type":"move","x":30,"y":40},{"type":"move","x":30,"y":40}])
        ),
        None
    );
    assert_eq!(
        number(
            &client,
            None,
            &drag((30.0, 40.0), (500.0, 500.0), Button::Left)
        ),
        Some(42)
    );
    assert_eq!(
        number(
            &client,
            None,
            &drag((500.0, 500.0), (30.0, 40.0), Button::Left)
        ),
        None
    );
}

#[tokio::test]
async fn malformed_point_fields_keep_the_unreadable_window_refusal_first() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    let path = client.config.output_dir.join("last-windows").join(format!(
        "{}.json",
        client.host_session.as_ref().unwrap().key()
    ));
    std::fs::write(path, "unreadable json").unwrap();
    let actions = json!([{"type":"click","x":30,"y":40,"count":"two"}]);
    let expected = remembered(&client, None, &actions).unwrap_err();
    let error = act(&client, None, &actions).await.unwrap_err();
    assert_eq!(json!(error), json!(expected));
    assert_eq!(error.code, Code::Windows);
    assert!(!error.uncertain);
    assert!(client.controller.pending().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_actions_keep_controller_refusals_when_window_memory_is_not_used() {
    let directory = tempfile::tempdir().unwrap();
    let anonymous = crate::test_support::client(directory.path());
    let named = session(&anonymous, "one");
    let explicit = Expected::new(None, Some(42), None).unwrap();
    let claim = anonymous.controller.quarantine().unwrap();
    let pending = anonymous.controller.pending().unwrap();
    for actions in [
        json!([{"type":"click","x":30,"y":40,"count":"two"}]),
        json!([{"type":"unknown"}]),
        json!([{"type":"key","key":false}]),
    ] {
        for (client, target) in [
            (&anonymous, None),
            (&named, None),
            (&named, explicit.as_ref()),
        ] {
            let expected = client.controller.acquire_beside_commands().unwrap_err();
            let error = act(client, target, &actions).await.unwrap_err();
            assert_eq!(json!(error), json!(expected));
            assert_eq!(client.controller.pending().unwrap(), pending);
        }
    }
    drop(claim);
}

#[tokio::test]
async fn a_named_malformed_batch_keeps_another_controllers_busy_refusal() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let client = session(&crate::test_support::client(directory.path()), "one");
    let _ownership = client.controller.acquire_beside_commands()?;
    for actions in [
        json!([{"type":"unknown"}]),
        json!([{"type":"click","x":30,"y":40,"count":"two"}]),
    ] {
        let Err(expected) = client.controller.acquire_beside_commands() else {
            panic!("the other controller must keep exclusion");
        };
        let Err(error) = act(&client, None, &actions).await else {
            panic!("a malformed batch cannot execute under another controller");
        };
        assert_eq!(error.code, Code::Busy);
        assert_eq!(json!(error), json!(expected));
        assert!(client.controller.pending()?.is_empty());
    }
    Ok(())
}

#[test]
fn remembered_points_target_the_frontmost_window_for_each_pointer_action() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0), window(43, 0.0)]);
    let actions = [
        click(30.0, 40.0, Button::Left, 1, &[]),
        scroll((30.0, 40.0), (0, 1)),
        drag((30.0, 40.0), (500.0, 500.0), Button::Left),
        move_to(30.0, 40.0),
    ];
    for action in actions {
        assert_eq!(number(&client, None, &action), Some(42));
    }
    assert_eq!(number(&client, None, &move_to(500.0, 500.0)), None);
}

#[tokio::test]
async fn ineligible_actions_do_no_extra_reads() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;

    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    let client = session(&client, "one");
    remember(&client, &[window(42, 10.0)]);
    let targets = [
        None,
        Expected::new(Some(Target::Pid(812)), None, None).unwrap(),
        Expected::new(None, Some(99), None).unwrap(),
        Expected::new(Some(Target::Pid(812)), None, Some("Search".into())).unwrap(),
    ];
    for expected in &targets {
        let actions = if expected.is_some() {
            vec![move_to(30.0, 40.0)]
        } else {
            vec![
                keys(&["a".into()]).unwrap(),
                json!([{"type":"type","text":"a"}]),
                json!([{"type":"click","x":30,"y":40}, {"type":"click","x":50,"y":40}]),
                move_to(500.0, 500.0),
            ]
        };
        for actions in actions {
            let mut original = None;
            for direct in [true, false] {
                let before = helper.proofs();
                let mut reads = Vec::new();
                let error = helper.serve(async {
                    if !direct {
                        act(&client, expected.as_ref(), &actions).await
                    } else if let Some(expected) = expected {
                        let ownership = client.controller.acquire_beside_commands()?;
                        let verified = client.verify().await?;
                        let expect = expected.resolve(&client, &verified).await?;
                        client.input_owned(&ownership, actions::Batch::decode(&actions), Some((&verified.session().epoch, std::time::Instant::now())), Some(expect)).await
                    } else {
                        client.input(&actions, None, None).await
                    }
                }, |request| {
                    reads.push((request["op"].clone(), request["argv"].clone()));
                    Some(if request["op"] == "apps" {
                        json!({"ok":true,"apps":[{"pid":812,"name":"Editor"}],"starts":{"812":[123,456]}})
                    } else {
                        assert_eq!(request["op"], "windows");
                        json!({"ok":true,"windows":[window(99,10.0)],"starts":{"812":[123,456]}})
                    })
                }).await.unwrap_err();
                assert_eq!(error.code, Code::Startup);
                let observed = (helper.proofs() - before, reads, json!(error));
                if direct {
                    original = Some(observed);
                } else {
                    assert_eq!(Some(observed), original);
                }
            }
        }
    }
}

#[tokio::test]
async fn point_actions_infer_only_the_observed_owner_in_the_observed_epoch() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    for change in 0..14 {
        let accepts = change == 0;
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let client = session(&client, "one");
        let listener = UnixListener::bind(&client.config.input_socket).unwrap();
        let sent = std::cell::RefCell::new(Vec::new());
        let native = async {
            loop {
                let (peer, _) = listener.accept().await.unwrap();
                let mut peer = BufReader::new(peer);
                let mut line = String::new();
                peer.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let reply = if request["op"] == "hello" {
                    json!({"ok":true,"pid":456,"protocolVersion":1,"ready":true,"accessibility":true,
                            "capabilities":[atlas_core::session::FOCUS_APP,atlas_core::session::FOCUS_DOCK,atlas_core::session::FOCUS_WINDOW],
                            "session":{"ok":true,"uid":UID,"onConsole":false,"hasGraphicAccess":true,
                                "securitySessionID":42,"displayID":7,"width":100,"height":100}})
                } else {
                    if request["op"] == "input" {
                        sent.borrow_mut().push(request["expect"].clone());
                    }
                    json!({"ok":true,"performed":1,"expect":request["expect"]})
                };
                peer.get_mut()
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        };
        let operation = helper.serve(
            async {
                let verified = client.verify().await.unwrap();
                let mut epoch = verified.session().epoch.clone();
                match change {
                    3 => epoch.uid += 1,
                    4 => epoch.capture_pid += 1,
                    5 => epoch.input_pid += 1,
                    6 => epoch.security_session_id += 1,
                    7 => epoch.display_id += 1,
                    8 => epoch.display.width += 1.0,
                    9 => epoch.display.height += 1.0,
                    10 => epoch.display.scale += 1.0,
                    _ => {}
                }
                client.remember_windows(
                    &hit_windows(
                        &[window(42, 10.0)],
                        &Bounds::display(100.0, 100.0),
                        &Dock::Unread,
                        &if change == 13 {
                            std::collections::BTreeMap::new()
                        } else {
                            std::collections::BTreeMap::from([(812, [123, 456])])
                        },
                    ),
                    &epoch,
                );
                let result = act(&client, None, &move_to(30.0, 40.0)).await;
                if accepts {
                    result.unwrap();
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(
                        error.code,
                        if (3..=10).contains(&change) {
                            Code::SessionChanged
                        } else {
                            Code::Windows
                        }
                    );
                    assert_eq!(
                        error.error,
                        match change {
                            1 | 12 =>
                                "The app's process changed while it was being resolved; look again",
                            2 =>
                                "No window 42 is on screen; run atlas windows to see their numbers",
                            3..=10 => "The helper changed since Atlas checked it; look again",
                            _ => "Window changed; look again",
                        }
                    );
                    assert!(!error.uncertain);
                }
            },
            |request| {
                if request["op"] == "apps" {
                    return Some(json!({"ok":true,"apps":[{"pid":812,"name":"Editor"}],
                        "starts":{"812":[123,456]}}));
                }
                assert_eq!(request["op"], "windows");
                assert!(
                    client.controller.acquire(false).is_err(),
                    "validation holds ownership"
                );
                let mut current = window(42, 10.0);
                if change == 1 {
                    current.pid = 900;
                }
                let windows = if change == 2 { vec![] } else { vec![current] };
                let starts = match change {
                    11 => json!({"812":[124,456]}),
                    12 => json!({}),
                    _ => json!({"812":[123,456]}),
                };
                Some(json!({"ok":true,"windows":windows,"starts":starts}))
            },
        );
        tokio::select! {
            () = operation => {},
            () = native => unreachable!(),
            () = tokio::time::sleep(std::time::Duration::from_secs(5)) => panic!("window validation timed out"),
        }
        let expected = if accepts {
            vec![json!({"pid":812,"start":[123,456],"window":42})]
        } else {
            vec![]
        };
        assert_eq!(*sent.borrow(), expected, "change {change}");
    }
}

#[test]
fn point_admission_keeps_a_window_expectation_during_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            start.wait();
            for turn in 0..256 {
                remember(&client, &[window(42 + turn % 2, 10.0)]);
            }
        });
        start.wait();
        let actions = move_to(30.0, 40.0);
        for _ in 0..4096 {
            assert!(matches!(number(&client, None, &actions), Some(42 | 43)));
        }
        writer.join().unwrap();
    });
}

#[test]
fn a_failed_replacement_keeps_the_previous_point_expectation() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    let partial = client.config.output_dir.join("last-windows").join(format!(
        "{}.json.partial",
        client.host_session.as_ref().unwrap().key()
    ));
    std::fs::create_dir(partial).unwrap();
    assert!(
        crate::last_capture::remember_windows(
            &client.config.output_dir,
            client.host_session.as_ref().unwrap().key(),
            Some(&[]),
            &epoch(),
        )
        .is_err()
    );
    assert_eq!(number(&client, None, &move_to(30.0, 40.0)), Some(42));
}

#[test]
fn remembered_points_leave_explicit_targets_and_keys_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    let explicit = Expected::new(None, Some(99), None).unwrap();
    assert_eq!(
        number(&client, explicit.as_ref(), &move_to(30.0, 40.0)),
        Some(99)
    );
    let app = Expected::new(Some(Target::Pid(812)), None, None).unwrap();
    assert!(
        remembered(&client, app.as_ref(), &move_to(30.0, 40.0))
            .unwrap()
            .is_none()
    );
    let field = Expected::new(Some(Target::Pid(812)), None, Some("Search".into())).unwrap();
    assert!(
        remembered(&client, field.as_ref(), &move_to(30.0, 40.0))
            .unwrap()
            .is_none()
    );
    for action in [
        keys(&["a".into()]).unwrap(),
        json!([{"type":"type","text":"a"}]),
    ] {
        assert!(remembered(&client, None, &action).unwrap().is_none());
    }
}

#[tokio::test]
async fn oversized_window_memory_refuses_point_actions_before_control_is_acquired() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    remember(&client, &[window(42, 10.0)]);
    let windows = hit_windows(
        &vec![window(42, 10.0); 1024],
        &Bounds::display(display().width, display().height),
        &Dock::Unread,
        &std::collections::BTreeMap::new(),
    );
    let error = crate::last_capture::remember_windows(
        &client.config.output_dir,
        client.host_session.as_ref().unwrap().key(),
        Some(&windows),
        &epoch(),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Io);
    assert_eq!(
        error.error,
        "Too many window regions to remember; look again"
    );
    let _ownership = client.controller.acquire(false).unwrap();
    for actions in [
        click(30.0, 40.0, Button::Left, 1, &[]),
        scroll((30.0, 40.0), (0, 1)),
        drag((30.0, 40.0), (500.0, 500.0), Button::Left),
        move_to(30.0, 40.0),
    ] {
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            act(&client, None, &actions),
        )
        .await
        .expect("unknown windows are refused without waiting for control")
        .unwrap_err();
        assert_eq!(error.code, Code::Windows);
        assert_eq!(
            error.error,
            "The windows changed while you looked; look again"
        );
        for expected in [
            Expected::new(Some(Target::Pid(812)), None, None).unwrap(),
            Expected::new(None, Some(99), None).unwrap(),
            Expected::new(Some(Target::Pid(812)), None, Some("Search".into())).unwrap(),
        ] {
            assert!(
                remembered(&client, expected.as_ref(), &actions)
                    .unwrap()
                    .is_none()
            );
        }
    }
    assert!(
        remembered(&client, None, &keys(&["a".into()]).unwrap())
            .unwrap()
            .is_none()
    );
    remember(&client, &[window(99, 10.0)]);
    assert_eq!(number(&client, None, &move_to(30.0, 40.0)), Some(99));
}

#[tokio::test]
async fn unreadable_window_memory_refuses_point_actions_before_control_is_acquired() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    for damage in 0..7 {
        let directory = tempfile::tempdir().unwrap();
        let client = session(&crate::test_support::client(directory.path()), "one");
        remember(&client, &[window(42, 10.0)]);
        let folder = client.config.output_dir.join("last-windows");
        let path = folder.join(format!(
            "{}.json",
            client.host_session.as_ref().unwrap().key()
        ));
        match damage {
            0 => std::fs::write(&path, "unreadable json").unwrap(),
            1 => std::fs::write(&path, r#"{"windows":false,"epoch":{}}"#).unwrap(),
            2 => std::fs::write(&path, vec![b' '; 65537]).unwrap(),
            3 => std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap(),
            4 => std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o0)).unwrap(),
            5 => {
                std::fs::remove_file(&path).unwrap();
                std::fs::create_dir(&path).unwrap();
            }
            _ => {
                std::fs::remove_file(&path).unwrap();
                symlink(directory.path().join("missing"), &path).unwrap();
            }
        }
        for actions in [
            click(30.0, 40.0, Button::Left, 1, &[]),
            scroll((30.0, 40.0), (0, 1)),
            drag((30.0, 40.0), (500.0, 500.0), Button::Left),
            move_to(30.0, 40.0),
        ] {
            assert!(remembered(&client, None, &actions).is_err());
            let error = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                act(&client, None, &actions),
            )
            .await
            .expect("unreadable memory is refused without waiting for control")
            .unwrap_err();
            assert!(matches!(error.code, Code::Windows | Code::Io));
            let explicit = Expected::new(None, Some(99), None).unwrap();
            assert!(
                remembered(&client, explicit.as_ref(), &actions)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            remembered(&client, None, &keys(&["a".into()]).unwrap())
                .unwrap()
                .is_none()
        );
        if damage == 4 {
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
}

#[test]
fn unobserved_and_anonymous_sessions_have_no_remembered_target() {
    let directory = tempfile::tempdir().unwrap();
    let anonymous = crate::test_support::client(directory.path());
    remember(&anonymous, &[window(42, 10.0)]);
    assert!(anonymous.remembered_windows().is_none());
    assert!(!anonymous.config.output_dir.exists());
    let client = session(&anonymous, "one");
    assert!(client.remembered_windows().is_none());
    assert!(
        remembered(&client, None, &move_to(30.0, 40.0))
            .unwrap()
            .is_none()
    );
}

#[test]
fn remembered_points_are_isolated_between_sessions_and_replaced_by_each_look() {
    let directory = tempfile::tempdir().unwrap();
    let first = session(&crate::test_support::client(directory.path()), "one");
    let second = session(&first, "two");
    remember(&first, &[window(42, 10.0)]);
    assert!(second.remembered_windows().is_none());
    remember(&second, &[window(99, 10.0)]);
    assert_eq!(
        first.remembered_windows().unwrap().windows.unwrap()[0].number,
        42
    );
    assert_eq!(
        second.remembered_windows().unwrap().windows.unwrap()[0].number,
        99
    );
    remember(&first, &[]);
    assert!(
        first
            .remembered_windows()
            .unwrap()
            .windows
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remembered_points_leave_menus_system_windows_and_batches_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    let app = Expected::new(Some(Target::Pid(812)), None, None).unwrap();
    for layer in [-1, 20, 24, 101] {
        let mut front = window(99, 10.0);
        front.layer = layer;
        remember(&client, &[front, window(42, 10.0)]);
        for expected in [None, app.as_ref()] {
            assert!(
                remembered(&client, expected, &move_to(30.0, 40.0))
                    .unwrap()
                    .is_none()
            );
        }
    }
    remember(&client, &[window(42, 10.0)]);
    for actions in [
        json!([]),
        json!([{"type":"click","x":30,"y":40}, {"type":"click","x":500,"y":40}]),
        json!([{"type":"wait","ms":1}, {"type":"click","x":30,"y":40}]),
    ] {
        assert!(
            remembered(&client, app.as_ref(), &actions)
                .unwrap()
                .is_none()
        );
        assert!(remembered(&client, None, &actions).unwrap().is_none());
    }
}

#[test]
fn remembered_points_use_the_same_dock_regions_as_the_point_check() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    let display = Bounds::display(1000.0, 800.0);
    let mut dock_window = window(99, 0.0);
    dock_window.layer = 20;
    dock_window.owner = Some("Dock".into());
    dock_window.bounds = display.clone();
    let windows = [dock_window, window(42, 10.0)];
    for dock in [
        Dock::Icons(vec![windows[1].bounds.clone()]),
        Dock::Other,
        Dock::Unread,
        Dock::Icons(vec![]),
    ] {
        client.remember_windows(
            &hit_windows(
                &windows,
                &display,
                &dock,
                &std::collections::BTreeMap::from([(812, [123, 456])]),
            ),
            &epoch(),
        );
        let screen = atlas_core::screen::windows::Screen::new(display.clone(), dock);
        let top = atlas_core::screen::windows::top_at(&windows, 30.0, 40.0, &screen).unwrap();
        assert_eq!(
            number(&client, None, &move_to(30.0, 40.0)),
            top.is_apps().then_some(top.number)
        );
    }
}

#[test]
fn remembering_windows_saves_no_names_and_leaves_session_records_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let client = session(&crate::test_support::client(directory.path()), "one");
    let sessions = client.output_dir().unwrap().join("sessions");
    std::fs::create_dir(&sessions).unwrap();
    std::fs::write(sessions.join("record.json"), "unchanged").unwrap();
    let mut observed = window(42, 10.0);
    observed.title = Some("private document".into());
    remember(&client, &[observed]);
    let path = client.config.output_dir.join("last-windows").join(format!(
        "{}.json",
        client.host_session.as_ref().unwrap().key()
    ));
    let bytes = std::fs::read(&path).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(!text.contains("private document") && !text.contains("Editor"));
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    let windows = &record["windows"];
    let fields: Vec<_> = windows[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(fields, ["bounds", "layer", "number", "pid", "start"]);
    assert_eq!(std::fs::read_dir(&sessions).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(sessions.join("record.json")).unwrap(),
        "unchanged"
    );
    std::fs::write(path, "unreadable json").unwrap();
    assert!(
        crate::last_capture::remembered_windows(
            &client.config.output_dir,
            client.host_session.as_ref().unwrap().key(),
        )
        .is_err()
    );
}
