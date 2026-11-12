//! Signing the helper in to an account, or deleting a site's data or the
//! browser's in its own settings, with agents paused and the screen hidden
//! from them. Start takes the person's lease beside nothing an agent runs,
//! refuses while an agent's background job may still run, hides the screen
//! and opens the page in the helper's browser under that lease; each step
//! that fails undoes those before it. Done and Cancel end the hold and the
//! lease together, and so does anything else that ends the lease: the
//! viewer's release, its lease running out, the menu bar's emergency stop or
//! the viewer itself ending. The person's Stop is never touched.
use super::{LEASE_SECONDS, Viewer};
use crate::menubar::Line;
use atlas_client::{agent, client::Client, jobs};
use atlas_control::{ActionPermit, Beside, HoldKind, PrivateHold};
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Holder, Result},
};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

/// What the person hears when an agent's command keeps them from signing in.
pub(super) const COMMAND_RUNNING: &str =
    "An agent's command is still running. Try again when it ends.";

/// Where the browsers the helper may sign in with are installed.
const APPLICATIONS: &str = "/Applications";

/// The helper's browser a sign-in opens: Google Chrome, else Firefox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Browser {
    Chrome,
    Firefox,
}

impl Browser {
    /// The app's name, as `open -a` takes it.
    fn app(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Firefox => "Firefox",
        }
    }

    /// Its bundle identifier, as accounts record the browser of a site.
    pub(super) fn bundle(self) -> &'static str {
        match self {
            Self::Chrome => "com.google.Chrome",
            Self::Firefox => "org.mozilla.firefox",
        }
    }

    /// The browser of bundle identifier `bundle`, when Atlas opens it.
    fn of(bundle: &str) -> Option<Self> {
        [Self::Chrome, Self::Firefox]
            .into_iter()
            .find(|browser| browser.bundle() == bundle)
    }

    /// The first of Chrome and Firefox installed in `applications`.
    fn installed_in(applications: &Path) -> Option<Self> {
        [Self::Chrome, Self::Firefox]
            .into_iter()
            .find(|browser| applications.join(format!("{}.app", browser.app())).is_dir())
    }

    /// Whether a page opens in a window of its own: Chrome opens one for it,
    /// its own settings pages included.
    fn new_window(self) -> bool {
        match self {
            Self::Chrome => true,
            Self::Firefox => false,
        }
    }

    /// The page `kind` opens for `address`, listed as `site`: the site
    /// itself to sign in, else the browser's own settings for its data.
    fn page(self, kind: HoldKind, address: &str, site: &str) -> String {
        match (kind, self) {
            (HoldKind::SignIn, _) => format!("https://{address}"),
            (HoldKind::Forget, Self::Chrome) => {
                format!("chrome://settings/content/all?searchSubpage={site}")
            }
            (HoldKind::Reset, Self::Chrome) => "chrome://settings/clearBrowserData".to_owned(),
            (HoldKind::Forget | HoldKind::Reset, Self::Firefox) => {
                "about:preferences#privacy".to_owned()
            }
        }
    }
}

/// A sign-in under way: the lease it holds, what it hides the screen for and
/// the browser it opened.
#[derive(Clone, Debug)]
pub(super) struct SignIn {
    lease: String,
    hold: PrivateHold,
    browser: Browser,
}

impl Viewer {
    /// Begin `kind` for the site `body` names (`site`, with `browser` its
    /// bundle identifier when the person's account names one): the person's
    /// lease, beside no agent's command; no agent's background job left
    /// running; the screen hidden from agents; the page open in the
    /// browser. The lease's token, which the app renews, ends and gives
    /// input with.
    pub(super) async fn sign_in_start(
        self: &Arc<Self>,
        kind: HoldKind,
        body: &Value,
    ) -> Result<Value> {
        let (address, site) = site_of(&body["site"])?;
        let browser = match body["browser"].as_str() {
            Some(bundle) => Browser::of(bundle).ok_or_else(|| {
                Error::new(Code::Input, "Atlas opens Google Chrome or Firefox for this")
            })?,
            // A second login shares this Mac's Applications folder.
            None => match self.client().desktop().kind() {
                Kind::SecondLogin(_) => Browser::installed_in(Path::new(APPLICATIONS)),
            }
            .ok_or_else(|| {
                Error::new(
                    Code::Apps,
                    "Install Google Chrome or Firefox to sign the helper in to a site",
                )
            })?,
        };
        self.follow_settings().await;
        if self.sign_in.lock().await.is_some() {
            return Err(
                Error::new(Code::Busy, "A sign-in is under way already").held_by(Holder::Person)
            );
        }

        let client = self.client();
        let current = client.identity().await?;
        if !current.input {
            return Err(Error::new(
                Code::Permission,
                "Accessibility permission is unavailable",
            ));
        }
        let lease = self
            .take_control_beside(&client, &current.epoch, Beside::Nothing)
            .await
            .map_err(command_running)?;
        let signing = SignIn {
            lease: lease.clone(),
            hold: PrivateHold { site, kind },
            browser,
        };
        let page = browser.page(kind, &address, &signing.hold.site);
        let started = async {
            self.publish(&signing).await?;
            self.held(&client, &signing, &page).await
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

    /// Note `signing` as under way while its lease still holds, checked
    /// under the lease's lock: a release from here on ends it, and one that
    /// came first leaves nothing noted.
    pub(super) async fn publish(&self, signing: &SignIn) -> Result<()> {
        let mut lease = self.lease.lock().await;
        lease.require(&signing.lease).map_err(|_| sign_in_ended())?;
        *self.sign_in.lock().await = Some(signing.clone());
        drop(lease);
        Ok(())
    }

    /// Undo a start under `lease` that failed: the lease, and with it the
    /// hold, ends, and the sign-in it noted goes even when the lease had
    /// already ended some other way. A release's own failure to clean up
    /// stays pending for recovery.
    pub(super) async fn abandon_start(&self, client: &Client, lease: &str) {
        let _ = self.release(Some(lease)).await;
        self.end_sign_in(client, lease).await;
    }

    /// The steps of a sign-in under its lease, in order: nothing an agent's
    /// finished command or background job left running, the screen hidden, then `page` opened under the
    /// person's control.
    async fn held(&self, client: &Client, signing: &SignIn, page: &str) -> Result<()> {
        client
            .controller
            .require_no_run_left()
            .map_err(command_running)?;
        jobs::require_none(client).await?;
        self.lease
            .lock()
            .await
            .hide_screen(&client.controller, &signing.lease, &signing.hold)?;
        let _opening = ActionPermit::begin(self.lease.clone(), &signing.lease).await?;
        let ownership = self.lease.lock().await.ownership(&signing.lease)?;
        // Each launch, and its success, only while the sign-in's lease
        // holds: one that ran out meanwhile has ended the sign-in.
        agent::open_for_person(
            client,
            &ownership,
            (&self.lease, &signing.lease),
            signing.browser.app(),
            page,
            signing.browser.new_window(),
        )
        .await
        .map_err(|error| match error.code {
            Code::LeaseLost => sign_in_ended(),
            _ => error,
        })?;
        Ok(())
    }

    /// Renew the sign-in `owner` holds, with no frame: the viewer's window
    /// may be covered or off screen while the person signs in elsewhere.
    pub(super) async fn sign_in_renew(&self, owner: &str) -> Result<Value> {
        self.signing(owner).await?;
        let client = {
            let mut lease = self.lease.lock().await;
            lease.renew(owner)?;
            self.holder()
        };
        client.native_control("renew", owner).await?;
        Ok(json!({"ok": true}))
    }

    /// The person is done: a site they signed in to joins their accounts,
    /// and the hold and the lease end. For a site's data or the browser's,
    /// nothing is removed here: the app asks whether to remove the account.
    pub(super) async fn sign_in_done(&self, owner: &str) -> Result<Value> {
        let signing = self.signing(owner).await?;
        let added = match signing.hold.kind {
            HoldKind::SignIn => Some(super::accounts::add_signed_in(
                &self.accounts_home(),
                &signing.hold.site,
                signing.browser.bundle(),
            )),
            HoldKind::Forget | HoldKind::Reset => None,
        };
        let released = self.release(Some(owner)).await;
        let mut reply = json!({
            "ok": true,
            "kind": signing.hold.kind,
            "site": signing.hold.site,
            "browser": signing.browser.bundle(),
        });
        if let Some(added) = added {
            reply["added"] = json!(added?);
        }
        released?;
        Ok(reply)
    }

    /// The person gave up: the hold and the lease end, and nothing is
    /// added. One already ended is fine.
    pub(super) async fn sign_in_cancel(&self, owner: &str) -> Result<Value> {
        if self.signing(owner).await.is_err() {
            return Ok(json!({"ok": true, "ended": false}));
        }
        self.release(Some(owner)).await?;
        Ok(json!({"ok": true, "ended": true}))
    }

    /// The sign-in `owner`'s lease holds.
    async fn signing(&self, owner: &str) -> Result<SignIn> {
        self.sign_in
            .lock()
            .await
            .clone()
            .filter(|signing| signing.lease == owner)
            .ok_or_else(sign_in_ended)
    }

    /// End the sign-in under the lease `released`, which is ending: the
    /// screen is shown to agents again. Whether one was under way.
    pub(super) async fn end_sign_in(&self, client: &Client, released: &str) -> bool {
        let mut signing = self.sign_in.lock().await;
        if signing
            .as_ref()
            .is_none_or(|signing| signing.lease != released)
        {
            return false;
        }
        *signing = None;
        // A hold that can't be ended is honoured no longer than the lease
        // it names, which ends now.
        let _ = client.controller.end_private();
        true
    }
}

/// What the person hears of a sign-in whose control ended.
fn sign_in_ended() -> Error {
    Error::new(
        Code::LeaseLost,
        "The sign-in ended because control ended. Start it again.",
    )
}

/// An agent's command still running refuses a sign-in in the person's
/// words; every other refusal keeps its own.
fn command_running(error: Error) -> Error {
    if error.code == Code::Busy && error.details.get("running") == Some(&json!("command")) {
        return Error::new(Code::Busy, COMMAND_RUNNING)
            .held_by(Holder::Agent)
            .with("running", json!("command"));
    }
    error
}

/// The address the person typed, without its scheme or path, and the site
/// it is listed by: its registrable domain.
fn site_of(given: &Value) -> Result<(String, String)> {
    let refused = || Error::new(Code::Input, "Type a site's address, like github.com");
    let typed = given
        .as_str()
        .map(str::trim)
        .filter(|typed| !typed.is_empty() && typed.len() <= 300)
        .ok_or_else(refused)?;
    let rest = ["https://", "http://"]
        .iter()
        .find_map(|scheme| {
            typed
                .get(..scheme.len())
                .filter(|start| start.eq_ignore_ascii_case(scheme))
                .map(|_| &typed[scheme.len()..])
        })
        .unwrap_or(typed);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };
    let host = atlas_core::name::web_host(host);
    let labels_fit = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    if host.is_empty() || host.len() > 253 || !labels_fit {
        return Err(refused());
    }
    let site = atlas_client::sign_ins::site_key(&host);
    let address = match port {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };
    Ok((address, site))
}

#[cfg(test)]
mod tests;
