//! A sign-in the service was holding when it died. The screen stays hidden
//! from agents and their changes wait, whatever client asks, until the
//! person finishes or cancels it in the app: the app's service takes it
//! over before it serves anything, and keeps that helper's exclusion until
//! then, so no other controller can take control and clear it.
use super::{Viewer, unpoisoned};
use atlas_client::client::Client;
use atlas_control::{Adopted, HoldKind, PrivateHold};
use atlas_core::{
    error::{Code, Error, Result},
    session::Observation,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// A sign-in this service took over, with the helper it is on: it ends
/// there, whichever helper the settings name by then.
pub(super) struct Interrupted {
    client: Arc<Client>,
    adopted: Adopted,
    /// Which taking over this is, so a request that looked at one never
    /// ends a later one.
    id: String,
}

impl Viewer {
    /// Take over the helper's interrupted sign-in, when one is interrupted
    /// and this viewer, the app's, holds none yet. The helper does not
    /// change while one is held (see `follow_settings`).
    ///
    /// # Errors
    ///
    /// Returns the failure to take one over: it then stays interrupted, and
    /// the next status read tries again.
    pub(super) fn adopt_interrupted(&self) -> Result<()> {
        if unpoisoned(&self.app).is_none() {
            return Ok(());
        }
        let mut held = unpoisoned(&self.interrupted);
        if held.is_some() {
            return Ok(());
        }
        *held = self.take_over_for(&self.client())?;
        Ok(())
    }

    /// `client`'s interrupted sign-in, taken over for the app, when its
    /// helper has one and this viewer serves the app.
    ///
    /// # Errors
    ///
    /// Returns the failure to take it over.
    pub(super) fn take_over_for(&self, client: &Arc<Client>) -> Result<Option<Interrupted>> {
        if unpoisoned(&self.app).is_none() {
            return Ok(None);
        }
        Ok(client
            .controller
            .adopt_interrupted()?
            .map(|adopted| Interrupted {
                client: client.clone(),
                adopted,
                id: atlas_core::files::token(),
            }))
    }

    /// The interrupted sign-in, for the person's status: its site, kind and
    /// whether Done can record it, as far as they are known, and, once this
    /// service took it over, the id the app answers it by; null when no
    /// sign-in is interrupted.
    pub(super) fn interrupted_for_status(&self) -> Value {
        let _ = self.adopt_interrupted();
        let held = unpoisoned(&self.interrupted);
        let (hold, id) = match held.as_ref() {
            Some(interrupted) => (
                interrupted.adopted.hold.clone(),
                Some(interrupted.id.clone()),
            ),
            None => match self.client().controller.interrupted_hold() {
                Some(interrupted) => (interrupted.hold, None),
                None => return Value::Null,
            },
        };
        let mut shown = hold.as_ref().map_or_else(|| json!({}), |hold| json!(hold));
        shown["canFinish"] = json!(finishable(hold.as_ref()));
        if let Some(id) = id {
            shown["id"] = json!(id);
        }
        shown
    }

    /// Finish (`done`) or cancel the interrupted sign-in `body` names by its
    /// `id`, as the status gave it, as `body` says:
    /// done records a site the person signed in to among their accounts;
    /// either way, once the helper shows no control and nothing held, the
    /// hold ends, the screen is shown again and the helper's exclusion is
    /// let go. Whatever fails, the hold stays, still held.
    ///
    /// # Errors
    ///
    /// Refuses a body without `done` or `id`, a sign-in other than the one
    /// held now (the person answered another), when none is held, Done
    /// for a hold that cannot be recorded, and while the helper still holds
    /// input or control; returns the failure to record the account or to
    /// end the hold.
    pub(super) async fn sign_in_interrupted(&self, body: &Value) -> Result<Value> {
        let done = body["done"]
            .as_bool()
            .ok_or_else(|| Error::new(Code::Input, "Say done: true or false"))?;
        // The sign-in the person answered, as the status named it to them.
        let id = body["id"]
            .as_str()
            .ok_or_else(|| Error::new(Code::Input, "Name the sign-in by its id"))?
            .to_owned();
        // One at a time, from here to the end: a request that waited finds
        // its sign-in ended. Stop never waits for this.
        let _resolving = self.resolving.lock().await;
        let (client, hold) = {
            let held = unpoisoned(&self.interrupted);
            let interrupted = held
                .as_ref()
                .filter(|interrupted| interrupted.id == id)
                .ok_or_else(none_interrupted)?;
            (interrupted.client.clone(), interrupted.adopted.hold.clone())
        };
        if done && !finishable(hold.as_ref()) {
            return Err(Error::new(
                Code::Input,
                "Atlas can't tell which sign-in was interrupted; cancel it",
            ));
        }
        settled(self, &client).await?;

        // Under one lock, and only for the very sign-in looked at: another
        // request may have ended it, and a later one taken its place.
        let mut held = unpoisoned(&self.interrupted);
        let interrupted = held
            .as_mut()
            .filter(|interrupted| interrupted.id == id)
            .ok_or_else(none_interrupted)?;
        let added = match (&hold, done) {
            (Some(hold), true) => match (hold.kind, hold.browser.as_deref()) {
                (HoldKind::SignIn, Some(browser)) => Some(super::accounts::add_signed_in(
                    &self.accounts_home(),
                    &hold.site,
                    browser,
                )?),
                _ => None,
            },
            _ => None,
        };
        interrupted.adopted.end()?;
        held.take();
        drop(held);
        let mut reply = json!({"ok": true, "done": done});
        if let Some(added) = added {
            reply["added"] = json!(added);
        }
        Ok(reply)
    }
}

/// A test's hold on the helper's recovery under way: each recovery says it
/// began, then waits to be let go on, while one is set.
#[cfg(test)]
#[derive(Default)]
pub(super) struct Pause {
    set: std::sync::Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Semaphore>)>>,
}

#[cfg(test)]
impl Pause {
    async fn pause(&self) {
        let set = unpoisoned(&self.set).clone();
        if let Some((began, go_on)) = set {
            began.notify_one();
            go_on.acquire().await.unwrap().forget();
        }
    }
}

fn none_interrupted() -> Error {
    Error::new(Code::State, "No sign-in was interrupted")
}

/// Whether Done can record `hold`: a sign-in needs the browser it was in;
/// deleting a site's data or the browser's records nothing.
fn finishable(hold: Option<&PrivateHold>) -> bool {
    hold.is_some_and(|hold| match hold.kind {
        HoldKind::SignIn => hold.browser.is_some(),
        HoldKind::Forget | HoldKind::Reset => true,
    })
}

/// Make sure `client`'s helper holds nothing the dead service left: no
/// control, no input held and its record written, as its input service
/// answers. When it does not answer so, its input is started again (reset,
/// or started anew), under the exclusion this service holds for the hold,
/// and looked at once more. An unknown outcome is never settled here: it
/// stays pending, and the next control waits for its recovery.
///
/// # Errors
///
/// Refuses while the helper still does not answer settled, and returns the
/// failure to start its input.
async fn settled(viewer: &Viewer, client: &Client) -> Result<()> {
    if answers_settled(client).await {
        return Ok(());
    }
    #[cfg(test)]
    viewer.recovery_pause.pause().await;
    #[cfg(not(test))]
    let _ = viewer;
    client.start_input_held().await?;
    if answers_settled(client).await {
        return Ok(());
    }
    Err(Error::new(
        Code::Busy,
        "The helper is still letting go of the sign-in's control; try again in a moment",
    ))
}

/// Whether `client`'s input service answers that it holds no control, no
/// input and nothing left to record. One of an earlier build never says
/// what is left to record: no control and no input held is all it can
/// answer, and all an earlier build's own Cancel ever waited for.
async fn answers_settled(client: &Client) -> bool {
    let observation = Observation::decode(&client.status().await);
    let native = &observation.native;
    let recorded = match native.unsettled {
        Some(unsettled) => !unsettled,
        None => native.ok && !observation.input_build_current(),
    };
    native.ok
        && native.control_active == Some(false)
        && native.held_input == Some(false)
        && recorded
}

#[cfg(test)]
#[path = "interrupted_tests.rs"]
mod tests;
