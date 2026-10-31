//! Where an agent's input must land, and sending a batch there: an app, or
//! a window and so its app, and the field with keyboard focus, found once
//! control is held, brought forward and checked by the helper before input.
use super::{
    target::{Target, snapshot_expect, target_among, target_expect},
    windows::verified_window_snapshot,
};
use crate::{
    actions::Expect,
    client::{Check, Client, Verified},
    error::{Code, Error, Result},
    last_capture::{Observation, recorded_epoch},
    native::windows::{DOCK_LAYER, HitWindow, Window},
    process::Instance,
};
use serde_json::Value;

/// Send `actions` as one batch. With an expectation, the app is found once
/// control is held. The helper brings its target forward, then refuses input
/// unless it lands where expected: text and keys while the app, and the
/// window and field when named, have keyboard focus, pointer actions where
/// the app's own window, or the named one, is the top one.
pub async fn act(client: &Client, expected: Option<&Expected>, actions: &Value) -> Result<Value> {
    let remembered = Expected::remembered(client, expected, actions);
    let Some(expected) = remembered.as_ref().or(expected) else {
        return client.input(actions, None, None).await;
    };

    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify().await?;
    let expect = expected.resolve(client, &verified).await?;
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
    observation: Option<(HitWindow, Value)>,
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
    fn remembered(client: &Client, expected: Option<&Self>, actions: &Value) -> Option<Self> {
        let app = match expected {
            Some(Self {
                place: Place::App(app),
                field: None,
                ..
            }) => Some(app.clone()),
            Some(_) => return None,
            None => None,
        };
        let [action] = actions.as_array()?.as_slice() else {
            return None;
        };
        let (x, y) = match action["type"].as_str()? {
            "click" | "scroll" | "move" => (action["x"].as_f64()?, action["y"].as_f64()?),
            "drag" => (action["fromX"].as_f64()?, action["fromY"].as_f64()?),
            _ => return None,
        };
        let Observation { windows, epoch } = client.remembered_windows()?;
        let window = windows.iter().find(|window| window.bounds.contains(x, y))?;
        if !(0..DOCK_LAYER).contains(&window.layer) {
            return None;
        }
        let mut expected = Self::new(app, Some(window.number), None).ok().flatten()?;
        expected.observation = Some((window.clone(), epoch));
        Some(expected)
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

        Ok(Some(Self {
            place,
            field,
            observation: None,
        }))
    }

    /// What the helper is asked to check, found in the helper `verified`
    /// describes: the app's process, or the named window's owner, refused
    /// when the window is not the named app's or its observation is stale.
    pub(super) async fn resolve(&self, client: &Client, verified: &Verified) -> Result<Expect> {
        if self
            .observation
            .as_ref()
            .is_some_and(|(_, epoch)| *epoch != recorded_epoch(&verified.session().epoch))
        {
            return Err(Error::new(
                Code::SessionChanged,
                "The helper changed since the window was observed; look again",
            ));
        }
        let mut expect = match &self.place {
            Place::App(app) => target_expect(client, verified, app).await,
            Place::Window { number, app } => {
                let listed = verified_window_snapshot(client, verified).await?;
                let owner = window_owner(&listed.windows, *number)?;
                let observed = if let Some((window, _)) = &self.observation {
                    let start = window
                        .start
                        .filter(|start| {
                            start[0] > 0
                                && start[1] < 1_000_000
                                && owner.pid == window.pid
                                && listed.starts.as_ref().and_then(|starts| starts.get(&owner.pid))
                                    == Some(start)
                        })
                        .ok_or_else(|| {
                            Error::new(
                                Code::Windows,
                                format!(
                                    "Window {number}'s process changed since it was observed; look again"
                                ),
                            )
                        })?;
                    Expect::from_instance(Instance::new(owner.pid as i32, start))
                } else {
                    snapshot_expect(owner.pid, listed.starts.as_ref())
                };
                if let Some(app) = app
                    && target_among(client, verified, app, Some(&listed))
                        .await?
                        .pid()
                        != owner.pid
                {
                    return Err(other_app(owner, app));
                }
                observed
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
        let remembered = Expected::remembered(client, expected, actions);
        match &remembered.as_ref().or(expected)?.place {
            Place::Window { number, .. } => Some(*number),
            Place::App(_) => None,
        }
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
    async fn point_actions_infer_only_the_observed_owner_in_the_observed_epoch() {
        use crate::test_support::{Helper, UID, completed};
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixListener,
        };

        for change in 0..14 {
            for app in [None, Some(Target::Pid(812))] {
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
                                (change != 13).then_some(&std::collections::BTreeMap::from([(
                                    812,
                                    [123, 456],
                                )])),
                            ),
                            &epoch,
                        );
                        let expected = Expected::new(app.clone(), None, None).unwrap();
                        let result = act(&client, expected.as_ref(), &move_to(30.0, 40.0)).await;
                        if change == 0 {
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
                        assert_eq!(request["argv"][1], "windows-info");
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
                        Some(completed(
                            &json!({"ok":true,"windows":windows,"starts":starts}).to_string(),
                        ))
                    },
                );
                tokio::select! {
                    () = operation => {},
                    () = native => unreachable!(),
                    () = tokio::time::sleep(std::time::Duration::from_secs(5)) => panic!("window validation timed out"),
                }
                let expected = if change == 0 {
                    vec![json!({"pid":812,"start":[123,456],"window":42})]
                } else {
                    vec![]
                };
                assert_eq!(*sent.borrow(), expected, "change {change}, app {app:?}");
            }
        }
    }

    #[test]
    fn remembered_points_leave_explicit_windows_fields_and_keys_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let client = session(&crate::test_support::client(directory.path()), "one");
        remember(&client, &[window(42, 10.0)]);
        let explicit = Expected::new(None, Some(99), None).unwrap();
        assert_eq!(
            number(&client, explicit.as_ref(), &move_to(30.0, 40.0)),
            Some(99)
        );
        let app = Expected::new(Some(Target::Pid(812)), None, None).unwrap();
        let remembered = Expected::remembered(&client, app.as_ref(), &move_to(30.0, 40.0)).unwrap();
        assert!(matches!(
            remembered.place,
            Place::Window {
                number: 42,
                app: Some(Target::Pid(812)),
            }
        ));
        let field = Expected::new(Some(Target::Pid(812)), None, Some("Search".into())).unwrap();
        assert!(Expected::remembered(&client, field.as_ref(), &move_to(30.0, 40.0)).is_none());
        for action in [
            keys(&["a".into()]).unwrap(),
            json!([{"type":"type","text":"a"}]),
        ] {
            assert!(Expected::remembered(&client, None, &action).is_none());
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
        assert_eq!(number(&client, None, &move_to(30.0, 40.0)), None);
    }

    #[test]
    fn remembered_points_are_isolated_between_sessions_and_replaced_by_each_look() {
        let directory = tempfile::tempdir().unwrap();
        let first = session(&crate::test_support::client(directory.path()), "one");
        let second = session(&first, "two");
        remember(&first, &[window(42, 10.0)]);
        assert_eq!(number(&second, None, &move_to(30.0, 40.0)), None);
        remember(&second, &[window(99, 10.0)]);
        assert_eq!(number(&first, None, &move_to(30.0, 40.0)), Some(42));
        assert_eq!(number(&second, None, &move_to(30.0, 40.0)), Some(99));
        remember(&first, &[]);
        assert_eq!(number(&first, None, &move_to(30.0, 40.0)), None);
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
                assert!(Expected::remembered(&client, expected, &move_to(30.0, 40.0)).is_none());
            }
        }
        remember(&client, &[window(42, 10.0)]);
        for actions in [
            json!([]),
            json!([{"type":"click","x":30,"y":40}, {"type":"click","x":500,"y":40}]),
            json!([{"type":"wait","ms":1}, {"type":"click","x":30,"y":40}]),
        ] {
            assert!(Expected::remembered(&client, app.as_ref(), &actions).is_none());
            assert!(Expected::remembered(&client, None, &actions).is_none());
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
            client.remember_windows(&hit_windows(&windows, &display, &dock, None), &epoch());
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
        let bytes = std::fs::read(client.config.output_dir.join("last-captures.json")).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(!text.contains("private document") && !text.contains("Editor"));
        let record: Value = serde_json::from_slice(&bytes).unwrap();
        let windows = &record["targets"]
            [format!("session:{}", client.host_session.as_ref().unwrap().key())]["windows"];
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
        std::fs::write(
            client.config.output_dir.join("last-captures.json"),
            "unreadable json",
        )
        .unwrap();
        assert_eq!(number(&client, None, &move_to(30.0, 40.0)), None);
    }
}
