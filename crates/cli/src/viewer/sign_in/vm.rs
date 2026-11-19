//! Signing a VM in to an account, as on Second Login, through its keeper:
//! the browser its guest has, the person's own control of it taken beside
//! nothing an agent runs, nothing an agent started left running there, the
//! screen hidden from agents, and the page opened in the guest's browser by
//! a change of the person's own under the owner their lease holds there,
//! which no Stop refuses or ends. The screen stays hidden from agents past
//! the lease, a lost reply and a restart of this service, until the guest
//! let go of the person's input for certain or the keeper's recovery proved
//! it.
use super::{
    super::{LEASE_SECONDS, Viewer},
    Browser, COMMAND_RUNNING, SignIn, command_running, sign_in_ended,
};
use crate::menubar::Line;
use atlas_client::{app_selection, client::Client};
use atlas_control::{ActionPermit, HoldKind, PrivateHold};
use atlas_core::error::{Code, Error, Holder, Result};
use serde_json::{Value, json};
use std::sync::Arc;

impl Viewer {
    /// [`Viewer::sign_in_start`] on `client`'s VM, for the site typed as
    /// `address`, listed as `site`: the person's lease, which the app
    /// renews, ends and gives input with, and what it signs in to.
    pub(super) async fn sign_in_start_on_vm(
        self: &Arc<Self>,
        client: Arc<Client>,
        kind: HoldKind,
        (address, site): (String, String),
        body: &Value,
    ) -> Result<Value> {
        let browser = match body["browser"].as_str() {
            Some(bundle) => Browser::of(bundle).ok_or_else(|| {
                Error::new(Code::Input, "Atlas opens Google Chrome or Firefox for this")
            })?,
            None => {
                let apps = [Browser::Chrome.app(), Browser::Firefox.app()];
                let installed = app_selection::installed(&client, &apps).await?;
                Browser::first_installed(|app| installed.get(app).copied().unwrap_or(false))
                    .ok_or_else(|| {
                        Error::new(
                            Code::Apps,
                            format!(
                                "Install Google Chrome or Firefox in {} to sign it in to a site",
                                client.selected.name
                            ),
                        )
                    })?
            }
        };
        self.follow_settings().await;
        if self.sign_in.lock().await.is_some() {
            return Err(
                Error::new(Code::Busy, "A sign-in is under way already").held_by(Holder::Person)
            );
        }

        let proof = client.person_proof_on_vm().await?;
        let lease = self
            .take_vm_control_proved(&client, &proof)
            .await
            .map_err(command_running)?;
        // One interrupted earlier is the person's to finish or cancel
        // first: a new one would take its place and leave it unresolved.
        if client.controller.interrupted_hold().is_some() {
            self.abandon_start(&client, &lease).await;
            return Err(Error::new(
                Code::Busy,
                "Finish or cancel the sign-in that was interrupted first",
            )
            .held_by(Holder::Person));
        }
        let signing = SignIn {
            lease: lease.clone(),
            hold: PrivateHold {
                site,
                kind,
                browser: Some(browser.bundle().to_owned()),
            },
            browser,
        };
        let page = browser.page(kind, &address, &signing.hold.site);
        let started = async {
            self.publish(&signing).await?;
            self.held_on_vm(&client, &signing, &page).await
        };
        if let Err(error) = started.await {
            self.abandon_start(&client, &lease).await;
            return Err(error);
        }
        // Only an app shows the viewer; without one the sign-in still holds.
        let _ = self.send(Line::Viewer);

        Ok(json!({
            "ok": true,
            "lease": lease,
            "ttlSeconds": LEASE_SECONDS,
            "kind": kind,
            "site": signing.hold.site,
            "browser": browser.bundle(),
        }))
    }

    /// The steps of a sign-in on a VM under its lease, in order: nothing an
    /// agent started left running there, as its guest says; the screen
    /// hidden; then `page` opened in the guest's browser under the owner the
    /// lease holds there, sent only while the lease still holds, and said
    /// opened only while it still does.
    async fn held_on_vm(&self, client: &Client, signing: &SignIn, page: &str) -> Result<()> {
        if client.leftovers().await?.may_run() {
            return Err(Error::new(Code::Busy, COMMAND_RUNNING)
                .held_by(Holder::Agent)
                .with("running", json!("command")));
        }
        self.lease
            .lock()
            .await
            .hide_screen(&client.controller, &signing.lease, &signing.hold)?;
        let _opening = ActionPermit::begin(self.lease.clone(), &signing.lease).await?;
        let generation = self.guest_generation()?;
        let capability = self.persons_capability(client)?;
        client
            .person_open_on_vm(
                (&self.lease, &signing.lease),
                generation,
                (signing.browser.app(), page),
                &capability,
            )
            .await
            .map_err(|error| match error.code {
                Code::LeaseLost => sign_in_ended(),
                _ => error,
            })?;
        // A lease that ran out meanwhile ended the sign-in, opened or not.
        self.lease
            .lock()
            .await
            .require(&signing.lease)
            .map_err(|_| sign_in_ended())
    }
}

#[cfg(test)]
#[path = "vm_tests.rs"]
mod tests;
