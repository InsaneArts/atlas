//! The frame service as the menu bar's viewer runs it: the private endpoint
//! for this controller and helper, one live source and the broker serving
//! it, with a private record clients find it by. It lives as long as the
//! viewer that started it.
use super::{
    auth::Gate,
    broker::{
        self, Witness,
        source::{self, Live},
    },
    endpoint::{self, Endpoint},
};
use atlas_client::client::Client;
use atlas_core::{
    error::Result,
    files,
    frame_pipe::{
        budget::{BROKER_PIXELS, Budget},
        model::{Hex32, Hex64, Right, UpdateKind},
    },
    protocol::CaptureBackend,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::oneshot;

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

/// Start the frame service for `client`'s helper, reading pictures at most
/// `longest` pixels on their longest side, and telling `witness` of each
/// committed picture.
///
/// # Errors
///
/// Returns the endpoint's refusals (an unsafe folder, another service for
/// this helper) and failures to write its record.
pub(crate) async fn start(client: Arc<Client>, longest: u32, witness: Witness) -> Result<Service> {
    let controller = nix::unistd::getuid().as_raw();
    let helper = client.transport.peer_uid;
    let (listeners, endpoint) = endpoint::bind(controller, helper).await?;
    let (service, bootstrap) = (Hex32::random(), Hex64::random());
    let descriptor = json!({
        "recordVersion": 1,
        "protocolMajor": 1,
        "controllerUID": controller,
        "helperUID": helper,
        "brokerPID": std::process::id(),
        "serviceInstance": service.as_str(),
        "observerBootstrap": bootstrap.as_str(),
        "controlPath": endpoint.socket("c.sock"),
        "framePath": endpoint.socket("f.sock"),
    });
    files::write_private(&endpoint.record(), &serde_json::to_vec(&descriptor)?)?;
    let gate = Gate::new(
        service.clone(),
        bootstrap,
        vec![Right::Observe],
        vec![UpdateKind::Full],
    );
    let source = source::spawn(
        Live::new(client, longest, CaptureBackend::CoreGraphics),
        Budget::new(BROKER_PIXELS),
    );
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(broker::run(
        listeners,
        controller,
        gate,
        service,
        source,
        witness,
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
