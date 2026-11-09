//! What the client's tests and those above it share: temporary clients, a
//! stand-in for the helper's daemon, unprivileged stand-ins for
//! interrupted mutation transactions, and a watch on stalled requests.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    reason = "test fixtures: failing is how they report a broken test"
)]
mod executor;
mod helper;
mod requester;

pub use executor::{
    Fixture, WORKER, executor_ready, executor_worker, interrupted_executor, pause_executor,
};
pub use helper::{Helper, Pictures, completed, exited};
pub use requester::{Requester, WATCHDOG};

use crate::{
    client::Client,
    task::{Claims, Owned, State, Task, store::Store},
};
use atlas_control::Controller;
use atlas_core::{
    config::Config,
    error::Result,
    test_support::{CONFIGURED_UID, SETTINGS, UID},
};
use atlas_native::approval::Refusal;
use std::{os::unix::net::UnixStream, path::Path, time::Instant};

/// The activity log's rotated file.
pub const PREVIOUS: &str = crate::activity_log::PREVIOUS;
/// The folder of the activity log's older files.
pub const HISTORY: &str = crate::activity_log::HISTORY;
/// The warning a doctor gives of web content.
pub const WEB_CONTENT: &str = crate::capability::WEB_CONTENT;
/// The warning a doctor gives of a failed Save/Open check.
pub const NATIVE_DIALOGS_FAILED: &str = crate::capability::NATIVE_DIALOGS_FAILED;

/// Initialize macOS's encoder before a fixture starts its input/save deadline.
/// Its first CGImage call can scan the test executable's bundle resources.
pub(crate) fn prepare_picture_encoder() {
    static READY: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    READY.get_or_init(|| {
        crate::helper_info::jpeg_rgb(&[200; 24], (4, 2, 12), (4, 0.6))
            .expect("the fixture's native picture encoder");
    });
}

/// Claim `owned` for `task`, kept in `store`.
pub fn claim_in(store: Store, task: Task, owned: &Owned, state: State) -> Result<()> {
    Claims::in_store(store, task).claim(owned, state)
}

/// Whether a process exists, whoever owns it.
pub fn process_exists(pid: u32) -> bool {
    crate::client::service::process_exists(pid)
}

/// Talk with the approved step over `stream` until it ends.
pub fn converse_until(
    stream: UnixStream,
    secret: Option<&[u8]>,
    deadline: Instant,
) -> (bool, std::result::Result<String, Refusal>) {
    crate::setup::executor::converse_until(stream, secret, deadline)
}

/// A client whose endpoints and controller state are entirely temporary. It
/// keeps no pictures before actions unless a test turns them on.
pub fn client(directory: &Path) -> Client {
    let mut client = Client::new(Config {
        helper_uid: UID,
        socket_path: directory.join("helper/absent.sock"),
        identity_probe: directory.join("absent-probe"),
        input_socket: directory.join("absent-input.sock"),
        output_dir: directory.join("output"),
        activity: atlas_core::config::Activity {
            pictures_before_actions: false,
            ..Default::default()
        },
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
    let note = atlas_control::Note {
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
