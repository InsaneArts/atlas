//! What two accounts share while one asks the other to be its helper: the
//! request naming them, and whether the onboarding window is open.
use super::{
    account_with_uid,
    helper::{ROOT, text_sha256},
    identity::Identity,
};
use crate::{error::Result, files::read_limited};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};

/// Where the person's account names the helper it chose, for Atlas in the
/// helper to find. It holds no secret; the helper's owner confirms it.
pub(crate) const REQUEST: &str = "/Users/Shared/Atlas/onboarding.json";

/// The onboarding window in the helper, while it is open: the helper's
/// daemon then leaves telling the person to the window.
pub(crate) const WINDOW_MARKER: &str = "onboarding-window";

pub(crate) const MAX_RECORD: u64 = 64 * 1024;

/// An account (the controller) asking another one to be its helper.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pairing {
    /// A fresh request identity; old acceptance never grants a new request.
    pub(crate) ticket: String,
    pub(crate) controller_uid: u32,
    pub(crate) controller_name: String,
    pub helper_uid: u32,
    pub(crate) helper_name: String,
    /// The account chosen before any authorization or login wait.
    pub(crate) helper_identity: Identity,
}

/// Read the shared request, trusting it only when it is a plain file the
/// controller it names owns, in a folder no other account can change, and
/// both accounts it names are still the ones with those user IDs.
pub fn request() -> Option<Pairing> {
    read_request(Path::new(REQUEST)).filter(|pairing| {
        let named = |uid: u32, name: &str| {
            account_with_uid(uid)
                .ok()
                .flatten()
                .is_some_and(|account| account.name == name)
        };

        named(pairing.controller_uid, &pairing.controller_name)
            && named(pairing.helper_uid, &pairing.helper_name)
    })
}

pub(crate) fn read_request(path: &Path) -> Option<Pairing> {
    let folder = fs::symlink_metadata(path.parent()?).ok()?;

    // Opened without following a link, and checked as opened.
    let file = crate::files::open_plain(path)?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.mode() & 0o022 != 0 {
        return None;
    }

    let body = read_limited(file, MAX_RECORD)?;
    let value: Value = serde_json::from_slice(&body).ok()?;
    let request = Pairing {
        ticket: match value.get("ticket") {
            Some(Value::String(ticket)) => ticket.clone(),
            None => text_sha256(&String::from_utf8_lossy(&body))[..crate::files::TOKEN_BYTES * 2]
                .to_owned(),
            _ => return None,
        },
        controller_uid: value["controllerUid"].as_u64()?.try_into().ok()?,
        controller_name: value["controllerName"].as_str()?.to_owned(),
        helper_uid: value["helperUid"].as_u64()?.try_into().ok()?,
        helper_name: value["helperName"].as_str()?.to_owned(),
        helper_identity: serde_json::from_value(value["helperIdentity"].clone()).ok()?,
    };
    if request.ticket.len() != crate::files::TOKEN_BYTES * 2
        || !request.ticket.bytes().all(|byte| byte.is_ascii_hexdigit())
        || request.helper_identity.uid != request.helper_uid
        || request.helper_identity.name != request.helper_name
    {
        return None;
    }
    let folder_safe = folder.is_dir()
        && (folder.uid() == request.controller_uid || folder.uid() == 0)
        && folder.mode() & 0o022 == 0;

    (folder_safe && metadata.uid() == request.controller_uid).then_some(request)
}

/// Write the request, readable by every account and changed only by this one.
pub(crate) fn write_request(path: &Path, request: &Pairing) -> Result<()> {
    let body = json!({
        "ticket": request.ticket,
        "controllerUid": request.controller_uid,
        "controllerName": request.controller_name,
        "helperUid": request.helper_uid,
        "helperName": request.helper_name,
        "helperIdentity": request.helper_identity,
    });

    crate::files::write_atomic(path, serde_json::to_string_pretty(&body)?.as_bytes(), 0o644)
}

/// Whether this account's onboarding window is open: the process it names
/// still lives and is Atlas's window (a reused process ID is not).
pub(crate) fn window_open(home: &Path) -> bool {
    let Some(pid) = read_text(&home.join(ROOT).join(WINDOW_MARKER), 64)
        .and_then(|text| text.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    else {
        return false;
    };

    if nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err() {
        return false;
    }

    Command::new("/bin/ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|output| {
            Path::new(String::from_utf8_lossy(&output.stdout).trim()).file_name()
                == Some(std::ffi::OsStr::new(crate::app_bundle::APP_EXECUTABLE))
        })
}

/// Read only a bounded plain text file, without waiting for a pipe's writer.
pub(crate) fn read_text(path: &Path, limit: u64) -> Option<String> {
    String::from_utf8(read_limited(crate::files::open_plain(path)?, limit)?).ok()
}
