//! The person's Stop for every desktop the settings list: a Stop is the
//! person's, for the whole Mac, so `atlas stop` and the Atlas menu bar both
//! hold each desktop, and no choice of desktop (a stale `ATLAS_DESKTOP`, a
//! broken binding) keeps it from any of them.
use crate::client::Client;
use atlas_core::{
    config::desktops::{Listed, Settings},
    error::Result,
};
use serde_json::{Value, json};
use std::borrow::Borrow;

/// Stop the agents on every desktop `settings` list. The command's own
/// client `own`, when it has one, stops its desktop; `make` makes the client
/// of each other one, which takes `own`'s labels so its rows name the same
/// command. See [`stop_each`] for what it answers.
///
/// # Errors
///
/// As [`stop_each`].
pub async fn stop_listed(
    settings: &Settings,
    own: Option<Client>,
    make: impl Fn(&Listed) -> Result<Client>,
) -> Result<Value> {
    let each = settings.desktops().iter().map(|listed| {
        let mine = own
            .as_ref()
            .filter(|own| own.desktop() == listed.desktop.desktop())
            .cloned();
        let client = mine.map_or_else(
            || {
                make(listed).map(|mut other| {
                    if let Some(own) = &own {
                        other.actor = own.actor;
                        other.doing = own.doing.clone();
                        other.host_session = own.host_session.clone();
                        other.controller = other.controller.labelled(own.host_session.as_ref());
                    }
                    other
                })
            },
            Ok,
        );
        (listed.name.clone(), client)
    });
    stop_each(each.collect()).await
}

/// Stop the agents on each desktop of `each`, by its name and its client or
/// why none could be made, one after the other. Each is told even when
/// another could not be. With one desktop its own reply is the answer, as
/// it always was. With more, the reply names each desktop's own, and whether
/// that one's Stop is uncertain: it failed, or its desktop may not have been
/// told. A VM desktop whose keeper is not reached keeps its Stop here all
/// the same, and its reply says so.
///
/// # Errors
///
/// Returns the first desktop's failure once all were tried, with what each
/// one said when there is more than one.
pub async fn stop_each<C: Borrow<Client>>(each: Vec<(String, Result<C>)>) -> Result<Value> {
    let mut held = Vec::new();
    for (name, client) in each {
        let stopped = match client {
            Ok(client) => client.borrow().hold(true).await,
            Err(error) => Err(error),
        };
        held.push((name, stopped));
    }
    if held.len() == 1 {
        return held.remove(0).1;
    }

    let each: Vec<Value> = held
        .iter()
        .map(|(name, stopped)| {
            json!({"desktop": name, "reply": reply_of(stopped), "uncertain": uncertain(stopped)})
        })
        .collect();
    let mut replies = held.into_iter().map(|(_, stopped)| stopped);
    let first = replies.next().unwrap_or_else(|| Ok(json!({"ok": true})));
    match first {
        Err(error) => Err(error.with("desktops", json!(each))),
        Ok(mut reply) => match replies.find_map(Result::err) {
            Some(error) => Err(error.with("desktops", json!(each))),
            None => {
                reply["desktops"] = json!(each);
                Ok(reply)
            }
        },
    }
}

fn reply_of(held: &Result<Value>) -> Value {
    match held {
        Ok(reply) => reply.clone(),
        Err(error) => json!(error),
    }
}

/// Whether a desktop's Stop came to something uncertain: it failed, its
/// reply says it may or may not have been told, or a VM's guest has not
/// confirmed it.
fn uncertain(held: &Result<Value>) -> bool {
    match held {
        Ok(reply) => reply["uncertain"] == true || reply["delivered"] == false,
        Err(_) => true,
    }
}

#[cfg(test)]
#[path = "stop_all_tests.rs"]
mod tests;
