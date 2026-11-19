//! What a VM's guest is told to do when its Atlas does not serve what is
//! asked of it, by a command and by doctor, and what status says of
//! updating it: one owner for all three, judged from the guest's build and
//! version beside this Mac's, the one its keeper runs. A command may send
//! the person to doctor; doctor always names a step of its own, so the
//! advice is never a dead end. Judging sends nothing and never starts a VM.
//!
//! A guest is only ever brought to this Mac's build, never taken back to an
//! older one: a guest newer than this Mac has this Mac updated instead.
use atlas_session::compatibility::UPDATES;
use serde_json::{Value, json};

/// What a guest newer than this Mac is told.
const NEWER: &str = "Update Atlas on this Mac.";

/// How a guest's Atlas stands to this Mac's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The very build this Mac runs.
    Current,
    /// Another build, no newer than this Mac's: an update brings it to
    /// this one.
    Older,
    /// A newer version than this Mac's.
    Newer,
    /// Not known: its build or version was not said, or cannot be compared.
    Unknown,
}

/// One build of Atlas as it is said: the SHA-256 of its executable and its
/// version, each none when it is not known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Build {
    pub(crate) digest: Option<String>,
    pub(crate) version: Option<String>,
}

impl Build {
    pub(crate) fn new(digest: Option<&str>, version: Option<&str>) -> Self {
        Self {
            digest: digest.map(str::to_owned),
            version: version.map(str::to_owned),
        }
    }

    /// This process's own: its executable, read the first time it is asked
    /// for, and its version.
    pub(crate) fn own() -> Self {
        static DIGEST: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        Self {
            digest: DIGEST
                .get_or_init(atlas_native::native::service::own_build)
                .clone(),
            version: Some(atlas_core::VERSION.to_owned()),
        }
    }

    /// What a keeper says of it: `{"build", "version"}`, each null when not
    /// known.
    pub(crate) fn said(&self) -> Value {
        json!({"build": self.digest, "version": self.version})
    }

    /// The build `said` names, as [`Build::said`] says one.
    pub(crate) fn of(said: &Value) -> Self {
        Self::new(said["build"].as_str(), said["version"].as_str())
    }

    /// Whether anything of it is known.
    pub(crate) fn known(&self) -> bool {
        self.digest.is_some() || self.version.is_some()
    }

    /// Its version, as numbers to compare; none when it is not known or
    /// not one.
    fn numbers(&self) -> Option<Vec<u64>> {
        self.version
            .as_deref()
            .and_then(atlas_core::app_bundle::exact_version)
    }
}

/// What a guest is judged on: what it declared it serves, its build, and
/// this Mac's.
#[derive(Clone, Debug, Default)]
pub(crate) struct Facts {
    pub(crate) declared: Vec<String>,
    pub(crate) guest: Build,
    pub(crate) own: Build,
}

impl Facts {
    /// How the guest's Atlas stands to this Mac's. The same digest is the
    /// same build; otherwise the versions tell, and another build of the
    /// same version is older only once both digests are known to differ.
    pub(crate) fn verdict(&self) -> Verdict {
        let (guest, own) = (&self.guest, &self.own);
        if guest.digest.is_some() && guest.digest == own.digest {
            return Verdict::Current;
        }
        match (guest.numbers(), own.numbers()) {
            (Some(theirs), Some(ours)) if theirs < ours => Verdict::Older,
            (Some(theirs), Some(ours)) if theirs > ours => Verdict::Newer,
            (Some(_), Some(_)) if guest.digest.is_some() && own.digest.is_some() => Verdict::Older,
            _ => Verdict::Unknown,
        }
    }

    /// Whether the guest declared it takes updates of its own Atlas.
    fn updatable(&self) -> bool {
        self.declared.iter().any(|declared| declared == UPDATES)
    }
}

/// What to do about the VM `name`, whose guest judged on `facts` does not
/// serve what was asked: bring it to this Mac's build, update this Mac, or
/// look closer with doctor; or, when it takes no update, add it again.
pub(crate) fn advice(name: &str, facts: &Facts) -> String {
    match facts.verdict() {
        Verdict::Newer => NEWER.to_owned(),
        Verdict::Current => format!("Run atlas doctor --desktop {name}."),
        Verdict::Older | Verdict::Unknown if !facts.updatable() => format!(
            "{name}'s Atlas cannot be updated from this Mac. Remove the VM and add it again."
        ),
        Verdict::Older => format!("Run atlas update to bring {name}'s Atlas to this Mac's build."),
        Verdict::Unknown => format!("Run atlas doctor --desktop {name}."),
    }
}

/// What doctor tells the person to do about the VM `name`, whose guest
/// judged on `facts` does not serve all this build needs: the advice, but
/// never doctor again. A guest of this very build, or of one not known that
/// takes updates, is restarted, and added again if that does not help.
pub(crate) fn remedy(name: &str, facts: &Facts) -> String {
    match facts.verdict() {
        Verdict::Current => restart(name),
        Verdict::Unknown if facts.updatable() => restart(name),
        Verdict::Older | Verdict::Newer | Verdict::Unknown => advice(name, facts),
    }
}

fn restart(name: &str) -> String {
    format!(
        "Restart {name} with atlas desktop maintain {name} --restart; if it stays, remove it and \
         add it again."
    )
}

/// What the status of the VM `name`, whose guest is judged on `facts`, says
/// of updating it.
pub(crate) fn said(name: &str, facts: &Facts) -> Value {
    match facts.verdict() {
        Verdict::Current => json!({"state": "current"}),
        Verdict::Older => json!({"state": "available", "next": advice(name, facts)}),
        Verdict::Newer => json!({"state": "newer", "next": NEWER}),
        Verdict::Unknown => json!({"state": "unknown"}),
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
