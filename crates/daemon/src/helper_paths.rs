//! Where the helper keeps its own files, inside its home: what the daemon
//! and the input service read and write as they run.
use std::path::PathBuf;

/// Private Atlas directory in the helper home and its entries.
pub const ROOT: &str = ".atlas";
/// Where the helper daemon's socket lives.
const DAEMON: &str = "helper";
const INPUT: &str = "input";
const LOGS: &str = "logs";

/// The paths inside one helper home.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperPaths {
    uid: u32,
    home: PathBuf,
}

impl HelperPaths {
    pub(crate) fn new(uid: u32, home: PathBuf) -> Self {
        Self { uid, home }
    }

    pub fn root(&self) -> PathBuf {
        self.home.join(ROOT)
    }

    pub fn logs(&self) -> PathBuf {
        self.root().join(LOGS)
    }

    pub fn daemon(&self) -> PathBuf {
        self.root().join(DAEMON)
    }

    /// The input service's folder.
    pub(crate) fn input(&self) -> PathBuf {
        self.root().join(INPUT)
    }

    pub fn input_socket(&self) -> PathBuf {
        self.input().join("input.sock")
    }

    /// The input service's saved stop, beside its socket: present while the
    /// person has stopped agents.
    pub(crate) fn stop_marker(&self) -> PathBuf {
        self.input_socket().with_extension("stopped")
    }

    pub fn socket(&self) -> PathBuf {
        self.daemon().join(format!("{}.sock", self.uid))
    }
}
