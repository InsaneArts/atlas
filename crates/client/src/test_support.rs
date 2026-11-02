//! What the client's tests and those above it share: temporary clients, a
//! stand-in for the helper's daemon, and unprivileged stand-ins for
//! interrupted mutation transactions.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "test fixtures: failing is how they report a broken test"
)]
mod executor;
mod helper;

pub use executor::{
    Fixture, WORKER, executor_ready, executor_worker, interrupted_executor, pause_executor,
};
pub use helper::{Helper, completed, exited};

use crate::{client::Client, control::Controller};
use atlas_core::{
    config::Config,
    test_support::{CONFIGURED_UID, SETTINGS, UID},
};
use std::path::Path;

/// A client whose endpoints and controller state are entirely temporary.
pub fn client(directory: &Path) -> Client {
    let mut client = Client::new(Config {
        helper_uid: UID,
        socket_path: directory.join("helper/absent.sock"),
        identity_probe: directory.join("absent-probe"),
        input_socket: directory.join("absent-input.sock"),
        output_dir: directory.join("output"),
    })
    .unwrap();
    client.controller = Controller::at(UID, &directory.join("state"));
    client
}

/// A live waiting note beside a temporary client's existing holder. No
/// thread waits for control, and no account's runtime paths are resolved.
pub fn waiting_note(state: &Path, host: Option<&crate::host_session::HostSession>) {
    use crate::host_session::{Host, Instance};
    use atlas_core::time::now_ms;
    use std::fs;

    let holder = fs::read_dir(state)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().ends_with(".holder.json"))
        .unwrap();
    let waiting = holder.with_file_name(
        holder
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".holder.json", ".waiting"),
    );
    fs::create_dir_all(&waiting).unwrap();
    let note = crate::control::Note {
        session: host.map(|host| host.key().clone()),
        host: host.map(|host| host.host()),
        folder: host.and_then(|host| host.folder().map(str::to_owned)),
        named: host
            .filter(|host| host.host() == Host::Named)
            .map(|host| host.id().clone()),
        process: Instance::current().unwrap(),
        since: now_ms(),
    };
    fs::write(
        waiting.join("0000000000000001.json"),
        serde_json::to_vec(&note).unwrap(),
    )
    .unwrap();
}

/// A client read from [`SETTINGS`] kept in `directory`, with its controller
/// state kept in `directory` too.
pub fn client_in(directory: &Path) -> Client {
    let config = Config::parse(SETTINGS.as_bytes(), &directory.join("u.toml")).unwrap();
    let mut client = Client::new(config).unwrap();
    client.controller = Controller::at(CONFIGURED_UID, &directory.join("state"));
    client
}
