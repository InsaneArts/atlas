//! Where an agent's input must land, and sending a batch there: an app, or
//! a window and so its app, and the field with keyboard focus, found once
//! control is held, brought forward and checked by the helper before input.
use super::target::{Target, snapshot_expect, target_among, target_expect};
use crate::{
    client::{Client, Verified, WINDOW_LOOKUP},
    last_capture::Observation,
};
use atlas_core::{
    actions::{self, Expect, Target as ActionTarget},
    error::{Code, Error, Result},
    screen::windows::{DOCK_LAYER, HitWindow, Window},
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Send `actions` as one batch. With an expectation, the app is found once
/// control is held. The helper brings its target forward, then refuses input
/// unless it lands where expected: text and keys while the app, and the
/// window and field when named, have keyboard focus, pointer actions where
/// the app's own window, or the named one, is the top one.
///
/// # Errors
///
/// Returns admission, identity, target, batch-validation or execution failures. An
/// uncertain outcome may include delivered input and must be recovered rather than
/// replayed.
pub async fn act(client: &Client, expected: Option<&Expected>, actions: &Value) -> Result<Value> {
    let decoded = actions::Batch::decode(actions);
    let point = lone_point(&decoded, actions);
    let remembered = Expected::remembered(client, expected, point)?;
    let inferred;
    let expected = match (&remembered, expected) {
        (None, None) => {
            let ownership = client.controller.acquire_beside_commands()?;
            return client.input_owned(&ownership, decoded, None, None).await;
        }
        (Some((window, _)), _) => {
            inferred = Expected {
                place: Place::Window {
                    number: window.number,
                    app: None,
                },
                field: None,
            };
            &inferred
        }
        (None, Some(expected)) => expected,
    };
    let ownership = client.controller.acquire_beside_commands()?;
    let verified = client.verify().await?;
    if let Some((_, epoch)) = &remembered
        && *epoch != json!(verified.session().epoch)
    {
        return Err(Error::session_changed());
    }
    let expect = expected
        .resolve(client, &verified)
        .await
        .map_err(|mut error| {
            if remembered.is_some() && matches!(error.code, Code::Elements | Code::Usage) {
                error.code = Code::Windows;
            }
            error
        })?;
    if let Some((window, _)) = remembered
        && expect.instance().is_none_or(|instance| {
            i64::from(instance.pid) != window.pid || Some(instance.start) != window.start
        })
    {
        return Err(Error::new(Code::Windows, "Window changed; look again"));
    }
    client
        .input_owned(
            &ownership,
            decoded,
            Some((&verified.session().epoch, std::time::Instant::now())),
            Some(expect),
        )
        .await
}

/// The point a batch of one pointer action lands on. A batch that did not
/// decode still names one, so remembered windows refuse before its own error.
fn lone_point(decoded: &Result<actions::Batch<'_>>, actions: &Value) -> Option<(f64, f64)> {
    match decoded {
        Ok(batch) => match batch.actions() {
            [action] => match action.target() {
                ActionTarget::Point(x, y) => Some((x, y)),
                ActionTarget::Drag { from, .. } => Some(from),
                ActionTarget::Keys | ActionTarget::Nothing | ActionTarget::Pointer(..) => None,
            },
            _ => None,
        },
        Err(_) => actions.as_array().and_then(|list| match list.as_slice() {
            [action] => actions::Kind::deserialize(action)
                .ok()?
                .remembered_point(action),
            _ => None,
        }),
    }
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
        point: Option<(f64, f64)>,
    ) -> Result<Option<(HitWindow, Value)>> {
        if expected.is_some() {
            return Ok(None);
        }
        let Some((x, y)) = point else {
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
    ///
    /// # Errors
    ///
    /// Returns a usage error when a field is named without an app or window. Identity,
    /// window and field validation happens during target resolution, not construction.
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
        let expect = match &self.place {
            Place::App(app) => target_expect(client, verified, app).await?,
            Place::Window { number, app } => {
                let listed = client
                    .windows_in(Some(verified), false, WINDOW_LOOKUP)
                    .await?;
                let owner = window_owner(&listed.windows, *number)?;
                if let Some(app) = app
                    && target_among(client, verified, app, Some(&listed))
                        .await?
                        .pid()
                        != owner.pid
                {
                    return Err(other_app(owner, app));
                }
                snapshot_expect(owner.pid, &listed.starts)?.in_window(*number)?
            }
        };

        match &self.field {
            Some(field) => expect.in_field(field),
            None => Ok(expect),
        }
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
mod tests;
