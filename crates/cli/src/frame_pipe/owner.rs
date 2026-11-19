//! Whose pictures a frame service serves: Second Login's helper, by its
//! account, or a VM, by its record key. The endpoint is keyed by it, and a
//! client finds the one service of its desktop by it: Second Login's any
//! observer of this account, a VM's the person's app alone.
use atlas_client::client::Client;
use atlas_core::{
    desktop::{Kind, RecordKey},
    error::Result,
    protocol::CaptureBackend,
};
use serde_json::Value;

/// Whose pictures a frame service serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    /// Second Login's helper, by its account.
    Helper(u32),
    /// A VM, by its record key: its guest's account is no account of this
    /// Mac's, and another VM's guest may have the same.
    Vm(RecordKey),
}

impl Owner {
    /// Whose pictures `client` is served: the helper its socket answers
    /// for, or its VM.
    ///
    /// # Errors
    ///
    /// Second Login's socket that cannot be read.
    pub(crate) fn of(client: &Client) -> Result<Self> {
        Ok(match client.desktop().kind() {
            Kind::User(_) => Self::Helper(client.link.socket()?.peer_uid),
            Kind::Vm(_) => Self::Vm(client.desktop().record_key()),
        })
    }

    /// What its frame service's record names it by, beside the rest.
    pub(crate) fn said(&self) -> (&'static str, Value) {
        match self {
            Self::Helper(uid) => ("helperUID", Value::from(*uid)),
            Self::Vm(key) => ("desktopKey", Value::from(key.as_str())),
        }
    }

    /// Whether the frame service `descriptor` describes serves it.
    pub(crate) fn serves(&self, descriptor: &Value) -> bool {
        let (key, said) = self.said();
        descriptor[key] == said
    }

    /// The words its endpoint's lock records it by: the helper's account,
    /// or the VM's record key.
    pub(crate) fn recorded(&self) -> String {
        match self {
            Self::Helper(uid) => uid.to_string(),
            Self::Vm(key) => key.as_str().to_owned(),
        }
    }

    /// What its pictures are read with: a VM's guest sends CoreGraphics
    /// frames whatever is asked.
    pub(crate) fn backend(&self) -> CaptureBackend {
        match self {
            Self::Helper(_) => CaptureBackend::ScreenCaptureKit,
            Self::Vm(_) => CaptureBackend::CoreGraphics,
        }
    }

    /// Whether only the person's app may observe its pictures: a VM's
    /// frame service is the app's window alone, never an agent's way to
    /// its screen.
    pub(crate) fn app_only(&self) -> bool {
        matches!(self, Self::Vm(_))
    }

    /// What its endpoint's refusals call it.
    pub(crate) fn called(&self) -> &'static str {
        match self {
            Self::Helper(_) => "this helper",
            Self::Vm(_) => "this VM",
        }
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
