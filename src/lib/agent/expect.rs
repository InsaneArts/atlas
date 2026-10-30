//! Where an agent's input must land, and sending a batch there: an app, or
//! a window and so its app, and the field with keyboard focus, found once
//! control is held, brought forward and checked by the helper before input.
use super::{
    target::{Target, resolved_instance, target_pid, target_pid_among},
    windows::verified_window_list,
};
use crate::{
    actions::Expect,
    client::{Check, Client, Verified},
    error::{Code, Error, Result},
    native::windows::Window,
};
use serde_json::Value;

/// Send `actions` as one batch. With an expectation, the app is found once
/// control is held. The helper brings its target forward, then refuses input
/// unless it lands where expected: text and keys while the app, and the
/// window and field when named, have keyboard focus, pointer actions where
/// the app's own window, or the named one, is the top one.
pub async fn act(client: &Client, expected: Option<&Expected>, actions: &Value) -> Result<Value> {
    let Some(expected) = expected else {
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
        let instance = resolved_instance(client, async {
            match &self.place {
                Place::App(app) => target_pid(client, verified, app).await,
                Place::Window { number, app } => {
                    let listed = verified_window_list(client, verified).await?;
                    let owner = window_owner(&listed, *number)?;
                    if let Some(app) = app
                        && target_pid_among(client, verified, app, &listed).await? != owner.pid
                    {
                        return Err(other_app(owner, app));
                    }
                    Ok(owner.pid)
                }
            }
        })
        .await?;
        let mut expect = Expect::from_instance(instance)?;
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
