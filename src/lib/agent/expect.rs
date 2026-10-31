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
    native::windows::{Bounds, Dock, Screen, Window, top_at},
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
            }) => Some(app.clone()),
            Some(_) => return None,
            None => None,
        };
        let action = &actions[0];
        let (x, y) = match action["type"].as_str()? {
            "click" | "scroll" | "move" => (action["x"].as_f64()?, action["y"].as_f64()?),
            "drag" => (action["fromX"].as_f64()?, action["fromY"].as_f64()?),
            _ => return None,
        };
        let (windows, display) = client.remembered_windows()?;
        let display = Bounds::display(display.width, display.height);
        let screen = Screen::new(display, Dock::Icons(vec![]));
        let window = top_at(&windows, x, y, &screen)?;
        Self::new(app, Some(window.number), None).ok().flatten()
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
                let listed = verified_window_snapshot(client, verified).await?;
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
        test_support::{Helper, UID, completed},
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
        client
            .remember_windows(&[window(42, 10.0), window(43, 0.0)], &display())
            .unwrap();
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

    #[test]
    fn remembered_points_leave_explicit_windows_fields_and_keys_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let client = session(&crate::test_support::client(directory.path()), "one");
        client
            .remember_windows(&[window(42, 10.0)], &display())
            .unwrap();
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
        anonymous
            .remember_windows(&[window(42, 10.0)], &display())
            .unwrap();
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
        first
            .remember_windows(&[window(42, 10.0)], &display())
            .unwrap();
        assert_eq!(number(&second, None, &move_to(30.0, 40.0)), None);
        second
            .remember_windows(&[window(99, 10.0)], &display())
            .unwrap();
        assert_eq!(number(&first, None, &move_to(30.0, 40.0)), Some(42));
        assert_eq!(number(&second, None, &move_to(30.0, 40.0)), Some(99));
        first.remember_windows(&[], &display()).unwrap();
        assert_eq!(number(&first, None, &move_to(30.0, 40.0)), None);
    }

    #[tokio::test]
    async fn remembered_points_follow_each_look_without_extra_probes_and_use_crop_coordinates() {
        use crate::{agent, client::Size, daemon::protocol::Region};
        use base64::{Engine, engine::general_purpose::STANDARD};

        for look in ["screenshot", "elements", "windows"] {
            let directory = tempfile::tempdir().unwrap();
            let (client, helper) = Helper::client(directory.path(), UID);
            let client = session(&client, "one");
            let mut reads = 0;
            let saved = helper
                .serve(
                    async {
                        match look {
                            "screenshot" => {
                                agent::screenshot(
                                    &client,
                                    None,
                                    Some(Region {
                                        x: 10,
                                        y: 20,
                                        width: 80,
                                        height: 80,
                                    }),
                                    Size::Longest(40),
                                    false,
                                )
                                .await
                            }
                            "elements" => {
                                agent::elements(
                                    &client,
                                    &Target::Pid(812),
                                    &agent::ElementFilter::default(),
                                    10,
                                    false,
                                )
                                .await
                            }
                            _ => {
                                agent::list(
                                    &client,
                                    &agent::WindowFilter {
                                        app: Some("Other".into()),
                                        ..Default::default()
                                    },
                                )
                                .await
                            }
                        }
                    },
                    |request| {
                        reads += 1;
                        let windows = [window(42, 10.0), window(43, 0.0)];
                        match request["op"].as_str() {
                            Some("screenshot") => return Some(json!({
                                "ok": true, "width": 40, "height": 40, "windows": windows,
                                "png": STANDARD.encode(crate::native::capture::sample_png(40, 40)),
                                "region": {"x": 10, "y": 20, "width": 80, "height": 80},
                            })),
                            Some("apps") => return Some(json!({
                                "ok": true, "apps": [{"pid": 812, "name": "Editor"}],
                            })),
                            _ => {}
                        }
                        assert!(matches!(
                            request["argv"][1].as_str(),
                            Some("elements-info" | "windows-info")
                        ));
                        Some(completed(
                            &json!({"ok":true,"windows":windows,"elements":[],"truncated":false})
                                .to_string(),
                        ))
                    },
                )
                .await
                .unwrap();
            assert_eq!(reads, if look == "elements" { 2 } else { 1 });
            let (x, y) = if look == "screenshot" {
                assert!(saved.get("windows").is_none());
                let scale = saved["pointsPerPixel"].as_f64().unwrap();
                (
                    saved["origin"]["x"].as_f64().unwrap() + 10.0 * scale,
                    saved["origin"]["y"].as_f64().unwrap() + 10.0 * scale,
                )
            } else {
                assert_eq!(saved["windows"], json!([]));
                (30.0, 40.0)
            };
            assert_eq!((x, y), (30.0, 40.0));
            assert_eq!(number(&client, None, &move_to(x, y)), Some(42));
        }
    }

    #[tokio::test]
    async fn remembered_points_refuse_a_gone_window_through_the_existing_window_path() {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let client = session(&client, "one");
        client
            .remember_windows(&[window(42, 10.0)], &display())
            .unwrap();
        let error = helper
            .serve(act(&client, None, &move_to(30.0, 40.0)), |request| {
                assert_eq!(request["argv"][1], "windows-info");
                Some(completed(
                    &json!({"ok":true,"windows":[window(99,10.0)]}).to_string(),
                ))
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Windows);
        assert_eq!(error.error, window_owner(&[], 42).unwrap_err().error);
    }
}
