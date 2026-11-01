//! Where an agent's input must land, and sending a batch there: an app, or
//! a window and so its app, and the field with keyboard focus, found once
//! control is held, brought forward and checked by the helper before input.
use super::{
    target::{Target, snapshot_expect, target_among, target_expect},
    windows::{DockRead, window_snapshot},
};
use crate::{
    actions::Expect,
    client::{Check, Client, Verified},
    error::{Code, Error, Result},
    last_capture::{Observation, recorded_epoch},
    native::windows::{DOCK_LAYER, HitWindow, Window},
};
use serde_json::Value;

/// Send `actions` as one batch. With an expectation, the app is found once
/// control is held. The helper brings its target forward, then refuses input
/// unless it lands where expected: text and keys while the app, and the
/// window and field when named, have keyboard focus, pointer actions where
/// the app's own window, or the named one, is the top one.
pub async fn act(client: &Client, expected: Option<&Expected>, actions: &Value) -> Result<Value> {
    let remembered = Expected::remembered(client, expected, actions)?;
    if expected.is_none() && remembered.is_none() {
        return client.input(actions, None, None).await;
    }
    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify().await?;
    let expect = if let Some((window, epoch)) = remembered {
        if epoch != recorded_epoch(&verified.session().epoch) {
            return Err(Error::new(
                Code::SessionChanged,
                "Helper changed; look again",
            ));
        }
        let expect = Expected::new(None, Some(window.number), None)?
            .unwrap()
            .resolve(client, &verified)
            .await
            .map_err(|mut error| {
                if matches!(error.code, Code::Elements | Code::Usage) {
                    error.code = Code::Windows;
                }
                error
            })?;
        if expect.instance().is_none_or(|instance| {
            i64::from(instance.pid) != window.pid || Some(instance.start) != window.start
        }) {
            return Err(Error::new(Code::Windows, "Window changed; look again"));
        }
        expect
    } else {
        expected.unwrap().resolve(client, &verified).await?
    };
    client
        .input_owned(
            &ownership,
            actions,
            &verified.session().epoch,
            Some(Check::Required(expect)),
        )
        .await
}

/// Where an agent's input must land: an app, by name or process, or a
/// window and so the app it belongs to, and within it, when named, the
/// field with keyboard focus.
#[derive(Debug, Clone)]
pub struct Expected {
    place: Place,
    field: Option<String>,
}

/// The app, or the window, an agent's input must reach.
#[derive(Debug, Clone)]
enum Place {
    /// Any window of the app.
    App(Target),
    /// One window, by its number, which must be `app`'s when one is named.
    Window { number: i64, app: Option<Target> },
}

impl Expected {
    fn remembered(
        client: &Client,
        expected: Option<&Self>,
        actions: &Value,
    ) -> Result<Option<(HitWindow, Value)>> {
        if expected.is_some() {
            return Ok(None);
        }
        let point = || {
            let [action] = actions.as_array()?.as_slice() else {
                return None;
            };
            match action["type"].as_str()? {
                "click" | "scroll" | "move" => Some((action["x"].as_f64()?, action["y"].as_f64()?)),
                "drag" => Some((action["fromX"].as_f64()?, action["fromY"].as_f64()?)),
                _ => None,
            }
        };
        let Some((x, y)) = point() else {
            return Ok(None);
        };
        let Some(session) = client.host_session.as_ref() else {
            return Ok(None);
        };
        let Some(Observation { windows, epoch }) =
            crate::last_capture::remembered_windows(&client.config.output_dir, session.key())?
        else {
            return Ok(None);
        };
        let windows = windows.ok_or_else(|| {
            Error::new(
                Code::Windows,
                "The windows changed while you looked; look again",
            )
        })?;
        Ok(windows
            .into_iter()
            .find(|window| window.bounds.contains(x, y))
            .filter(|window| (0..DOCK_LAYER).contains(&window.layer))
            .map(|window| (window, epoch)))
    }

    /// The expectation of `app`, `window` and `field`, as many as are
    /// named; None when none is. A field needs the app or window it is in.
    pub fn new(
        app: Option<Target>,
        window: Option<i64>,
        field: Option<String>,
    ) -> Result<Option<Self>> {
        let place = match (app, window) {
            (app, Some(number)) => Place::Window { number, app },
            (Some(app), None) => Place::App(app),
            (None, None) if field.is_some() => {
                return Err(Error::new(
                    Code::Usage,
                    "--field needs the app it is in: add --app NAME or --window NUMBER",
                ));
            }
            (None, None) => return Ok(None),
        };

        Ok(Some(Self { place, field }))
    }

    /// What the helper is asked to check, found in the helper `verified`
    /// describes: the app's process, or the named window's owner, refused
    /// when the window is not the named app's.
    pub(super) async fn resolve(&self, client: &Client, verified: &Verified) -> Result<Expect> {
        let mut expect = match &self.place {
            Place::App(app) => target_expect(client, verified, app).await,
            Place::Window { number, app } => {
                let listed = window_snapshot(client, verified, DockRead::Skip).await?;
                let owner = window_owner(&listed.windows, *number)?;
                if let Some(app) = app
                    && target_among(client, verified, app, Some(&listed))
                        .await?
                        .pid()
                        != owner.pid
                {
                    return Err(other_app(owner, app));
                }
                snapshot_expect(owner.pid, listed.starts.as_ref())
            }
        }?;
        if let Place::Window { number, .. } = &self.place {
            expect = expect.in_window(*number)?;
        }

        if let Some(field) = &self.field {
            expect = expect.in_field(field)?;
        }

        Ok(expect)
    }
}

/// The listed window numbered `number`, one of an app's own, as `atlas
/// windows` lists them.
pub(super) fn window_owner(listed: &[Window], number: i64) -> Result<&Window> {
    listed
        .iter()
        .find(|window| window.number == number && window.is_apps())
        .ok_or_else(|| {
            Error::new(
                Code::Windows,
                format!("No window {number} is on screen; run atlas windows to see their numbers"),
            )
        })
}

/// The usage error for a window and an app that are not each other's.
pub(super) fn other_app(window: &Window, app: &Target) -> Error {
    let owner = window.owner.as_deref().unwrap_or("another app");
    let named = match app {
        Target::Name(name) => name.clone(),
        Target::Pid(pid) => format!("process {pid}"),
    };
    Error::new(
        Code::Usage,
        format!(
            "Window {} is {owner}'s, not {named}'s: name one app, or leave out --app",
            window.number
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::Button,
        agent::{click, drag, keys, move_to, scroll},
        host_session::{Host, HostSession},
        native::windows::{Bounds, Dock, hit_windows},
    };
    use serde_json::json;

    fn session(client: &Client, name: &str) -> Client {
        let mut client = client.clone();
        client.host_session = Some(HostSession::new(Host::Named, name).unwrap());
        client
    }

    fn display() -> crate::session::Display {
        crate::session::Display {
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

    fn epoch() -> crate::session::Epoch {
        crate::session::Epoch {
            uid: crate::test_support::UID,
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
                Some(&std::collections::BTreeMap::from([(812, [123, 456])])),
            ),
            &epoch(),
        );
    }

    fn number(client: &Client, expected: Option<&Expected>, actions: &Value) -> Option<i64> {
        Expected::remembered(client, expected, actions)
            .unwrap()
            .map(|(window, _)| window.number)
            .or_else(|| match &expected?.place {
                Place::Window { number, .. } => Some(*number),
                Place::App(_) => None,
            })
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
        use crate::test_support::{Helper, UID};

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
                            client.input_owned(&ownership, &actions, &verified.session().epoch, Some(Check::Required(expect))).await
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
                    let observed = (helper.proofs() - before, reads);
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
        use crate::test_support::{Helper, UID};
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
                                "capabilities":[crate::session::FOCUS_APP,crate::session::FOCUS_DOCK,crate::session::FOCUS_WINDOW],
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
                            (change != 13)
                                .then_some(&std::collections::BTreeMap::from([(812, [123, 456])])),
                        ),
                        &epoch,
                    );
                    let result = act(&client, None, &move_to(30.0, 40.0)).await;
                    if accepts {
                        result.unwrap();
                    } else {
                        assert_eq!(
                            result.unwrap_err().code,
                            if (3..=10).contains(&change) {
                                Code::SessionChanged
                            } else {
                                Code::Windows
                            }
                        );
                    }
                },
                |request| {
                    if request["op"] == "apps" {
                        return Some(json!({"ok":true,"apps":[{"pid":812,"name":"Editor"}]}));
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
                        12 => Value::Null,
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
            Expected::remembered(&client, app.as_ref(), &move_to(30.0, 40.0))
                .unwrap()
                .is_none()
        );
        let field = Expected::new(Some(Target::Pid(812)), None, Some("Search".into())).unwrap();
        assert!(
            Expected::remembered(&client, field.as_ref(), &move_to(30.0, 40.0))
                .unwrap()
                .is_none()
        );
        for action in [
            keys(&["a".into()]).unwrap(),
            json!([{"type":"type","text":"a"}]),
        ] {
            assert!(
                Expected::remembered(&client, None, &action)
                    .unwrap()
                    .is_none()
            );
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
            None,
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
                    Expected::remembered(&client, expected.as_ref(), &actions)
                        .unwrap()
                        .is_none()
                );
            }
        }
        assert!(
            Expected::remembered(&client, None, &keys(&["a".into()]).unwrap())
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
                4 => {
                    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o0)).unwrap()
                }
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
                assert!(Expected::remembered(&client, None, &actions).is_err());
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
                    Expected::remembered(&client, explicit.as_ref(), &actions)
                        .unwrap()
                        .is_none()
                );
            }
            assert!(
                Expected::remembered(&client, None, &keys(&["a".into()]).unwrap())
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
            Expected::remembered(&client, None, &move_to(30.0, 40.0))
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
                    Expected::remembered(&client, expected, &move_to(30.0, 40.0))
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
                Expected::remembered(&client, app.as_ref(), &actions)
                    .unwrap()
                    .is_none()
            );
            assert!(
                Expected::remembered(&client, None, &actions)
                    .unwrap()
                    .is_none()
            );
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
                    Some(&std::collections::BTreeMap::from([(812, [123, 456])])),
                ),
                &epoch(),
            );
            let screen = crate::native::windows::Screen::new(display.clone(), dock);
            let top = crate::native::windows::top_at(&windows, 30.0, 40.0, &screen).unwrap();
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
}
