//! The frame service as the menu bar's viewer runs it: the private endpoint
//! for this controller and the desktop it shows, one live source and the
//! broker serving it, with a private record clients find it by. It lives as
//! long as the viewer that started it.
use super::{
    auth::Gate,
    broker::{
        self, Privacy, Witness,
        source::{self, Live},
    },
    endpoint::{self, Endpoint},
    owner::Owner,
};
use atlas_client::client::Client;
use atlas_core::{
    error::Result,
    files,
    frame_pipe::{
        budget::{BROKER_PIXELS, Budget},
        model::{Hex32, Hex64, Right, UpdateKind},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{Notify, oneshot};

/// A running frame service.
pub(crate) struct Service {
    descriptor: Value,
    stop: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
    endpoint: Endpoint,
}

impl Service {
    /// What a client needs to pair: the sockets, the service instance and
    /// the observer capability. A bearer secret: never printed or logged.
    pub(crate) fn descriptor(&self) -> &Value {
        &self.descriptor
    }

    /// End every observer, give the helper's stream back and remove the
    /// service's sockets and record.
    pub(crate) async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
        let _ = std::fs::remove_file(self.endpoint.record());
    }
}

/// Start the frame service for `client`'s desktop, reading pictures at most
/// `longest` pixels on their longest side, telling `witness` of each
/// committed picture, showing each observer what `privacy` lets it see, and
/// leaving a still screen's slower pace on each notice on `input`. A VM's
/// endpoint is its own, apart from Second Login's, and its record names it
/// by its record key: its guest's account is none of this Mac's, so it is
/// left out. Its observer capability is kept out of its record: the viewer
/// hands it to the app alone.
///
/// # Errors
///
/// Refuses a VM not yet added or whose keeper gives no live pictures;
/// returns the endpoint's refusals (an unsafe folder, another service for
/// this desktop) and failures to write its record.
pub(crate) async fn start(
    client: Arc<Client>,
    longest: u32,
    witness: Witness,
    privacy: Privacy,
    input: Arc<Notify>,
) -> Result<Service> {
    client.live_window_served().await?;
    let controller = nix::unistd::getuid().as_raw();
    let owner = Owner::of(&client)?;
    let (named, said) = owner.said();
    let (listeners, endpoint) = endpoint::bind(controller, &client).await?;
    let (service, bootstrap) = (Hex32::random(), Hex64::random());
    let mut descriptor = json!({
        "recordVersion": 1,
        "protocolMajor": 1,
        "controllerUID": controller,
        "brokerPID": std::process::id(),
        "serviceInstance": service.as_str(),
        "observerBootstrap": bootstrap.as_str(),
        "controlPath": endpoint.socket("c.sock"),
        "framePath": endpoint.socket("f.sock"),
    });
    descriptor[named] = said;
    // A VM's observer capability goes only to the app, by the viewer:
    // its record keeps no way in for anyone else of this account.
    let mut record = descriptor.clone();
    if owner.app_only()
        && let Some(record) = record.as_object_mut()
    {
        record.remove("observerBootstrap");
    }
    files::write_private(&endpoint.record(), &serde_json::to_vec(&record)?)?;
    let gate = Gate::new(
        service.clone(),
        bootstrap,
        vec![Right::Observe],
        vec![UpdateKind::Full],
    );
    let source = source::spawn(
        Live::new(client, longest, owner.backend()),
        Budget::new(BROKER_PIXELS),
        input,
        privacy.visible.clone(),
    );
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(broker::run(
        listeners,
        controller,
        gate,
        source,
        witness,
        privacy,
        async {
            let _ = stopped.await;
        },
    ));
    Ok(Service {
        descriptor,
        stop,
        task,
        endpoint,
    })
}

/// Held by every test that starts a frame service for the scripted VM,
/// whose endpoint is the same for each.
#[cfg(test)]
pub(crate) static SCRIPTED_VM: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
