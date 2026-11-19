//! The stand-in's updates of its own Atlas, as a machine guest takes them:
//! only while its own Stop is kept, only of a copy staged into it whose
//! manifest digest is the one the update names, refused `changed`
//! otherwise; kept unsettled, answered pending, until a test settles it,
//! and holding every Resume meanwhile.
use super::{StandIn, copies::ROOT};
use atlas_core::files::Token;
use serde_json::{Value, json};
use std::sync::{PoisonError, atomic::Ordering::SeqCst};

/// What an update not yet settled is answered.
fn pending() -> Value {
    json!({"ok": true, "op": "update", "pending": true, "update": "asked"})
}

impl StandIn {
    /// What the update `change`, asked with `request`, is answered: pending
    /// once it is taken, or why it is refused. One asked again is answered
    /// pending while it is unsettled.
    pub(super) fn update(&self, change: &Token, request: &Value) -> Value {
        let refused = |code: &str, error: &str| json!({"ok": false, "code": code, "error": error});
        self.updates_asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        if self.unsettled_update(change) {
            return pending();
        }
        if !self.halted.load(SeqCst) {
            return refused(
                "bad-request",
                "Stop agents first: an update is taken only while they are stopped.",
            );
        }
        let (Some(staged), Some(expect)) = (request["staged"].as_str(), request["expect"].as_str())
        else {
            return refused(
                "bad-request",
                "update requires the staged bundle and its digest",
            );
        };
        let bound = self.copies().iter().find_map(|(copy, kept)| {
            (format!("{ROOT}/{copy}/{}", kept.name) == staged)
                .then(|| kept.snapshot.digest == expect)
        });
        match bound {
            None => refused("bad-request", "This cannot be staged: it is not there"),
            Some(false) => {
                let mut changed = refused(
                    "state",
                    "The bundle staged for this update is not the one its station staged; \
                     nothing was installed.",
                );
                changed["update"] = json!("changed");
                changed
            }
            Some(true) => {
                self.updates
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(change.clone(), request.clone());
                pending()
            }
        }
    }

    /// Whether the update `change` was taken and is not settled.
    pub fn unsettled_update(&self, change: &Token) -> bool {
        self.updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(change)
    }

    /// Settle the update `change` as `phase` (`released`, `rolledBack` or
    /// `refused`), as a guest's reconciliation does: kept as what the change
    /// came to. Whether it was one taken and unsettled.
    pub fn settle_update(&self, change: &Token, phase: &str) -> bool {
        if self
            .updates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(change)
            .is_none()
        {
            return false;
        }
        self.done(
            change,
            &json!({"ok": phase == "released", "op": "update", "update": phase, "why": null}),
        );
        true
    }
}

/// What a change voided is answered, and kept as.
fn voided() -> Value {
    json!({"ok": true, "op": "void", "voided": true})
}

/// The refusal of a change asked before with another request.
fn asked_otherwise() -> Value {
    json!({"ok": false, "code": "bad-request",
        "error": "This change was asked for before with another request."})
}

impl StandIn {
    /// Void the change `change`, as a machine guest does: kept refused for
    /// good when it was never asked; one asked before is refused, never
    /// voided.
    pub(super) fn void(&self, change: &Token) -> Value {
        let mut records = self.records();
        match records.get(change) {
            None => {
                records.insert(change.clone(), Some(voided()));
                voided()
            }
            Some(Some(reply)) if *reply == voided() => voided(),
            Some(_) => asked_otherwise(),
        }
    }

    /// The refusal of a stage or update asked under the name of a change
    /// voided, when `change` is one.
    pub fn refused_as_voided(&self, change: &Token) -> Option<Value> {
        self.records()
            .get(change)
            .is_some_and(|kept| kept.as_ref() == Some(&voided()))
            .then(asked_otherwise)
    }
}
