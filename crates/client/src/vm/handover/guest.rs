//! Bringing each VM's Atlas to the build its keeper runs, once this Mac runs
//! that build: asked of every keeper an update handed over, a step each, and
//! of each VM not on this build when `atlas update` finds Atlas current
//! here. The keeper alone stages and asks the update (its `update-guest`),
//! only while the station's Stop kept for an update holds agents and the
//! guest confirmed it; the station keeps that Stop, under the desktop's
//! exclusion and the update's hold, and asks once it is kept, the exclusion
//! let go, as the keeper takes it for each step it says. Nothing is asked
//! while the person's maintenance of the VM is kept. A guest of this build,
//! a newer one, one whose build cannot be told, one that takes no update
//! from this Mac by the check the keeper's update admits it by, or one
//! whose update to that build already failed, unless the person asks to try
//! again, is left as it is, and no Stop is kept for it.
use super::{Desk, Found, HELLO, Held, Keepers, LOOK, Launchd};
use crate::vm::{
    lifecycle::Lifecycle,
    maintenance::Kept,
    update::{Facts, listed},
};
use atlas_control::StopWhy;
use atlas_core::error::{Code, Error, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// How long the keeper is given to answer: once the guest took the update,
/// the update settled, or this passed. It carries the update on after.
const ASKING: Duration = Duration::from_secs(20);

/// How long a keeper that has not yet seen its guest confirm the Stop kept
/// for the update is asked again.
const SETTLING: Duration = Duration::from_secs(30);

/// Ask the keeper of the VM keyed `key` in the settings at `settings` to
/// bring its guest to the keeper's own build, as `update-finish` does for
/// each keeper an update handed over: what the keeper answered, and which
/// desktop it is.
///
/// # Errors
///
/// Refuses a VM the settings do not list, an uninstall's VM phase under
/// way, the person's maintenance of the VM, a keeper of an older build, and
/// the keeper's own refusal, in its words.
pub fn update_guest(key: &str, settings: &Path) -> Result<Value> {
    let found = Found::of(key, settings)?;
    let desk = Desk::own(found.selected.name.clone(), found.vm.clone())?;
    let lifecycle = Lifecycle::own()?;
    let _admitted = lifecycle.admit()?;
    asked(&Launchd::of(&[found]), &desk, &lifecycle, false, SETTLING)
}

/// [`update_guest`] of `desk` through `keepers`, the person's maintenance
/// read in `lifecycle`, a `retry` of an update of this build that failed
/// when the person asked for one; asked again for `settling` while the
/// keeper waits for its guest to confirm the Stop kept for the update.
pub(in crate::vm) fn asked(
    keepers: &dyn Keepers,
    desk: &Desk,
    lifecycle: &Lifecycle,
    retry: bool,
    settling: Duration,
) -> Result<Value> {
    let (kept, _) = Kept::read(Some(lifecycle), &desk.vm);
    if let Some(refused) = kept.update_refused(&desk.name) {
        return Err(refused);
    }
    if !keepers.loaded(&desk.vm)? {
        return Ok(not_asked(desk, "No session keeper runs for it."));
    }
    let hello = hello_of(keepers, desk)?;
    if !takes_updates(&hello) {
        return Err(older_keeper(&desk.name));
    }
    let request = json!({"op": "update-guest", "retry": retry,
        "deadline_ms": u64::try_from(ASKING.as_millis()).unwrap_or(u64::MAX)});
    let until = Instant::now() + settling;
    loop {
        let mut answer = keepers
            .ask(&desk.vm, &request, ASKING + HELLO)?
            .pop()
            .unwrap_or_default();
        if answer["ok"] == true {
            answer["desktop"] = json!(desk.name);
            return Ok(answer);
        }
        // The keeper takes the Stop kept for the update once its guest
        // confirmed it, which it says within a few seconds.
        let confirming =
            answer["code"] == "stopped" && stopped_for_update(desk) && Instant::now() < until;
        if !confirming {
            return Err(refusal(&answer, &desk.name));
        }
        std::thread::sleep(LOOK);
    }
}

impl Held {
    /// Bring the Atlas of each VM held that runs an older build than its
    /// keeper's to that build, Atlas being current on this Mac: the Stop
    /// kept for an update noted for each, under its exclusion, then each
    /// keeper asked once every exclusion is let go. An update of this build
    /// that failed is tried again only on the person's `retry`. What each VM
    /// came to, in order, as [`update_guest`] says it, or why it was left as
    /// it is; a VM whose Stop could not be noted says so, and every other
    /// is still asked.
    ///
    /// # Errors
    ///
    /// Refuses, unless `allow_disrupt`, while work runs on any VM held, as a
    /// swap does, before any Stop is noted.
    pub fn bring_to_this_build(self, allow_disrupt: bool, retry: bool) -> Result<Vec<Value>> {
        if self.is_empty() {
            return Ok(Vec::new());
        }
        let lifecycle = Lifecycle::own()?;
        self.brought(&lifecycle, (allow_disrupt, retry), SETTLING)
    }

    /// [`Held::bring_to_this_build`], the person's maintenance read in
    /// `lifecycle`, each keeper asked again for `settling`.
    pub(in crate::vm) fn brought(
        self,
        lifecycle: &Lifecycle,
        (allow_disrupt, retry): (bool, bool),
        settling: Duration,
    ) -> Result<Vec<Value>> {
        self.idle(allow_disrupt)?;
        let left: Vec<Option<Value>> = self
            .desks
            .iter()
            .map(|desk| {
                judged(self.keepers.as_ref(), desk, retry).or_else(|| {
                    // The update's own Stop, kept as a handover keeps it: one
                    // noted already holds agents and stays as it is.
                    desk.controller
                        .note_stop_unless_noted(StopWhy::Update)
                        .err()
                        .map(|failed| json!(failed.with("desktop", json!(desk.name))))
                })
            })
            .collect();
        let Self {
            keepers,
            desks,
            _exclusions: exclusions,
            _admitted: admitted,
        } = self;
        drop(exclusions);
        let brought = desks
            .iter()
            .zip(left)
            .map(|(desk, left)| {
                left.unwrap_or_else(|| {
                    asked(keepers.as_ref(), desk, lifecycle, retry, settling)
                        .unwrap_or_else(|refused| json!(refused.with("desktop", json!(desk.name))))
                })
            })
            .collect();
        drop(admitted);
        Ok(brought)
    }
}

/// Why `desk`'s VM is left as it is, said as a step's answer: none when its
/// guest runs an older build than its keeper's and takes an update from it,
/// by the very check the keeper's update admits it by, its keeper serving
/// updates from a program not replaced, and no update of that build kept
/// as failed unless the person asked to `retry` it.
fn judged(keepers: &dyn Keepers, desk: &Desk, retry: bool) -> Option<Value> {
    use crate::vm::update::{NEEDS, Verdict};
    use atlas_session::compatibility::{Usable, judge};
    let refused = |error: Error| Some(json!(error.with("desktop", json!(desk.name))));
    match keepers.loaded(&desk.vm) {
        Ok(true) => {}
        Ok(false) => return Some(not_asked(desk, "No session keeper runs for it.")),
        Err(error) => return refused(error),
    }
    let hello = match hello_of(keepers, desk) {
        Ok(hello) => hello,
        Err(error) => return refused(error),
    };
    let facts = Facts::of_hello(&hello);
    let left = || {
        let mut said = crate::vm::update::said(&desk.name, &facts);
        said["ok"] = json!(true);
        said["desktop"] = json!(desk.name);
        Some(said)
    };
    if facts.verdict() != Verdict::Older {
        return left();
    }
    match declared_of(&hello).map(|declared| judge(&declared, &NEEDS)) {
        Some(Usable::Usable) => {}
        Some(incompatible @ Usable::Incompatible { .. }) => {
            return incompatible.refusal().and_then(refused);
        }
        Some(Usable::MissingCapabilities { .. }) | None => return left(),
    }
    if !takes_updates(&hello) {
        return refused(older_keeper(&desk.name));
    }
    let build = facts.own.digest.as_deref().unwrap_or_default();
    match crate::vm::keeper::update_failed_for(&desk.folder, build) {
        Ok(Some(outcome)) if !retry => Some(failed(desk, outcome)),
        Ok(_) => None,
        Err(_) => refused(
            Error::new(
                Code::State,
                format!(
                    "What the last update of {}'s Atlas came to cannot be read, so it is not \
                     updated.",
                    desk.name
                ),
            )
            .with(
                "next",
                json!(format!("Run atlas doctor --desktop {}.", desk.name)),
            ),
        ),
    }
}

/// What a VM whose update to its keeper's build the guest tried and that
/// did not bring it there, as `outcome` says, is told: its agents are not
/// stopped again for a try that would only end the same way, unless the
/// person asks for one. A release that left it older was a release onto
/// another build.
fn failed(desk: &Desk, outcome: Value) -> Value {
    let why = if outcome["ok"] == true {
        json!(format!(
            "{}'s Atlas was updated, but it does not say it runs this Mac's build.",
            desk.name
        ))
    } else if outcome["why"].is_string() {
        outcome["why"].clone()
    } else {
        outcome["error"].clone()
    };
    json!({"ok": true, "desktop": desk.name, "state": "refused",
        "why": why, "outcome": outcome,
        "next": "Run atlas update --retry-vms to try again."})
}

/// What the guest of a keeper whose hello is `hello` declared, as its
/// update admission judges it: none when the keeper does not say.
fn declared_of(hello: &Value) -> Option<atlas_session::compatibility::Declared> {
    let guest = &hello["guest"];
    Some(atlas_session::compatibility::Declared {
        protocol: u32::try_from(guest["protocol"].as_u64()?).ok()?,
        capabilities: listed(&guest["capabilities"]),
        version: guest["version"].as_str()?.to_owned(),
    })
}

/// The hello of `desk`'s keeper.
fn hello_of(keepers: &dyn Keepers, desk: &Desk) -> Result<Value> {
    keepers
        .ask(&desk.vm, &json!({"op": "hello"}), HELLO)?
        .pop()
        .filter(|hello| hello["ok"] == true)
        .ok_or_else(|| {
            Error::new(
                Code::Transport,
                format!("{}'s session keeper did not answer.", desk.name),
            )
        })
}

/// Whether the keeper whose hello is `hello` updates its guest, from a
/// program not replaced since it started.
fn takes_updates(hello: &Value) -> bool {
    let serves = hello["keeper"]["capabilities"]
        .as_array()
        .is_some_and(|capabilities| {
            capabilities
                .iter()
                .any(|capability| capability == crate::vm::keeper::UPDATE_GUEST)
        });
    serves && hello["program"]["replaced"] == false
}

/// Whether the Stop kept for `desk`'s VM is the update's own.
fn stopped_for_update(desk: &Desk) -> bool {
    desk.controller
        .stop_revision()
        .is_ok_and(|revision| revision.is_some_and(|revision| revision.why() == StopWhy::Update))
}

fn not_asked(desk: &Desk, why: &str) -> Value {
    json!({"ok": true, "desktop": desk.name, "state": "notAsked", "why": why})
}

fn older_keeper(name: &str) -> Error {
    Error::new(
        Code::Update,
        format!(
            "{name}'s session keeper is from an older build; run atlas update again once it is \
             handed over."
        ),
    )
}

/// The keeper's refusal `answer`, in its own words and code, with what to
/// do next when it says, naming its desktop `name`.
fn refusal(answer: &Value, name: &str) -> Error {
    let refused = Error::refused(
        Code::of_reply(answer, Code::Update),
        answer,
        "The VM's session keeper did not answer whether it updates its Atlas.",
    );
    let refused = match answer["next"].as_str() {
        Some(next) => refused.with("next", json!(next)),
        None => refused,
    };
    refused.with("desktop", json!(name))
}

#[cfg(test)]
#[path = "guest_tests.rs"]
mod tests;
