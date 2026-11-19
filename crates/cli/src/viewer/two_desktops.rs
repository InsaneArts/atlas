//! Two desktops served at once, for the tests of every part of the service
//! that serves them: Second Login and a VM, each stood in for.
use super::{
    Viewer,
    http::{APP_SECRET_HEADER, bind, respond},
    unpoisoned,
};
use atlas_client::{
    client::{Actor, Client},
    test_support::{ScriptedKeeper, VM_ID, vm_through_keeper},
};
use atlas_core::{config::desktops::Settings, test_support::CONFIGURED_UID};
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::Method,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Settings listing the Second Login `main`, the default, with the whole
/// Mac's output in `output`, and, with `work`, the VM desktop
/// [`vm_through_keeper`] stands in for.
pub(super) fn listing(output: &Path, work: bool) -> String {
    let mut text = format!(
        "default = \"main\"\noutput_dir = \"{}\"\n\n[[desktops]]\nname = \"main\"\nkind = \
         \"user\"\nhelper_uid = {CONFIGURED_UID}\nsocket_path = \"c.sock\"\nidentity_probe = \
         \"probe\"\ninput_socket = \"i.sock\"\n",
        output.display()
    );
    if work {
        text.push_str(&format!(
            "\n[[desktops]]\nname = \"work\"\nkind = \"vm\"\nid = \"{VM_ID}\"\n"
        ));
    }
    text
}

/// A service following settings that list Second Login and a VM, Second
/// Login viewed, each stood in for: the VM's keeper and Atlas VM under a
/// folder of their own, the settings and the station's folder in another.
pub(super) struct Two {
    pub(super) viewer: Arc<Viewer>,
    pub(super) keeper: ScriptedKeeper,
    pub(super) main: Client,
    pub(super) work: Client,
    pub(super) settings: PathBuf,
    pub(super) home: PathBuf,
    _vm: tempfile::TempDir,
    _private: tempfile::TempDir,
}

impl Two {
    pub(super) async fn new() -> Self {
        let vm = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let private = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(private.path()).unwrap();
        let (mut work, keeper) = vm_through_keeper(vm.path());
        work.actor = Actor::Viewer;
        let settings = home.join("atlas.toml");
        std::fs::write(&settings, listing(&vm.path().join("output"), true)).unwrap();
        let listed = Settings::load(&settings).unwrap();
        let mut main = Client::new(listed.selected(&listed.desktops()[0])).unwrap();
        main.controller = atlas_control::Controller::at(CONFIGURED_UID, &vm.path().join("main"));
        let (_, viewer) = bind(main.clone(), 0, Some(&settings)).await.unwrap();
        unpoisoned(&viewer.stand_ins).extend([main.clone(), work.clone()]);
        *unpoisoned(&viewer.station_home) = Some(home.clone());
        viewer.follow_settings().await;
        Self {
            viewer,
            keeper,
            main,
            work,
            settings,
            home,
            _vm: vm,
            _private: private,
        }
    }

    pub(super) fn id(client: &Client) -> String {
        client.desktop().id().as_str().to_owned()
    }
}

/// `method` of `route` asked of `viewer` from its own origin, with `body`
/// when given, as the app when `app`: the reply's status and body.
pub(super) async fn asked(
    viewer: &Arc<Viewer>,
    method: Method,
    route: &str,
    body: Option<&Value>,
    app: bool,
) -> (u16, Vec<u8>) {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("{}{route}", viewer.prefix))
        .header("host", &viewer.authority)
        .header("origin", format!("http://{}", viewer.authority));
    if app {
        request = request.header(APP_SECRET_HEADER, &viewer.app_secret);
    }
    let request = match body {
        Some(body) => {
            let text = body.to_string();
            request
                .header("content-type", "application/json")
                .header("content-length", text.len())
                .body(Body::from(text))
        }
        None => request.body(Body::empty()),
    };
    let reply = respond(viewer.clone(), request.unwrap()).await;
    let status = reply.status().as_u16();
    let bytes = to_bytes(reply.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

pub(super) async fn read(viewer: &Arc<Viewer>, route: &str, app: bool) -> (u16, Value) {
    let (status, body) = asked(viewer, Method::GET, route, None, app).await;
    (status, serde_json::from_slice(&body).unwrap())
}

pub(super) async fn post(
    viewer: &Arc<Viewer>,
    route: &str,
    body: &Value,
    app: bool,
) -> (u16, Value) {
    let (status, body) = asked(viewer, Method::POST, route, Some(body), app).await;
    (status, serde_json::from_slice(&body).unwrap())
}
