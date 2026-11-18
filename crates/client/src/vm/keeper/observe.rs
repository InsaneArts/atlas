//! The keeper's `observe`: what a command proves the VM by, gathered in one
//! generation for that command to judge. The guest's hello, its input
//! service's own hello and its proof for the command's own challenge are
//! read on lanes of that generation, then Atlas VM's state; the answer names
//! the generation, the guest key its handshake pinned and the station's own
//! key. The keeper judges none of it and keeps none of it.
use super::{Keeper, Session, read};
use atlas_core::{
    error::{Code, Error},
    protocol::valid_challenge,
};
use atlas_session::{
    compatibility::{INPUT_LANES, MACHINE_PROOF, judge},
    lane::LaneKind,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// The keeper's capability to observe.
pub(super) const CAPABILITY: &str = "observe-v1";
/// How many observations run at once; more are answered busy.
const AT_ONCE: usize = 4;
/// By when each part of an observation ends, in tenths of its deadline, so
/// none that stalls takes the time the others and the answer need: the
/// optional input hello first, the guest's hello and proof next, then Atlas
/// VM's state; the last tenth is the answer's, to reach its client within
/// the client's own deadline.
const INPUT_BY: u32 = 5;
const READS_BY: u32 = 7;
const RUNNER_BY: u32 = 9;

/// The share of `deadline` that is `tenths` of it.
fn share(deadline: Duration, tenths: u32) -> Duration {
    deadline * tenths / 10
}

/// Room for the observations that run at once.
pub(super) fn room() -> tokio::sync::Semaphore {
    tokio::sync::Semaphore::new(AT_ONCE)
}

/// Observe the VM for `challenge` within `deadline`, as `observe` asks.
pub(super) async fn observed(
    keeper: &Keeper,
    challenge: Option<&str>,
    deadline: Duration,
) -> Value {
    // Refused before anything is opened.
    let Some(challenge) = challenge.filter(|challenge| valid_challenge(challenge)) else {
        return refusal(
            Code::BadRequest,
            "An observation needs a challenge of 1 to 128 printable characters.",
        );
    };
    let Ok(_room) = keeper.observing.try_acquire() else {
        return refusal(
            Code::Busy,
            "The keeper is already proving this VM for others; ask again in a moment.",
        );
    };
    let Some(session) = keeper.shared.live() else {
        let why = keeper.shared.state().why.clone();
        return refusal(
            Code::Transport,
            why.as_deref().unwrap_or("The VM's session is not live."),
        );
    };
    if judge(session.control.declared(), &[MACHINE_PROOF])
        .refusal()
        .is_some()
    {
        return refusal(Code::Update, "Update Atlas in the VM.");
    }
    let started = tokio::time::Instant::now();
    let hello = json!({"op": "hello"});
    let proving = json!({"op": "session-proof", "challenge": challenge});
    let reads = share(deadline, READS_BY);
    let (hello, input, proof) = tokio::join!(
        final_reply(&session, LaneKind::Read, &hello, reads),
        input_service(&session, share(deadline, INPUT_BY)),
        final_reply(&session, LaneKind::Read, &proving, reads),
    );
    // Within what is left of its share, at the runner's socket itself.
    let left = (started + share(deadline, RUNNER_BY))
        .saturating_duration_since(tokio::time::Instant::now())
        .max(Duration::from_millis(1));
    let runner = match keeper.reach.runner(left).await {
        Ok(reply) => json!({
            "ok": reply.ok,
            "power": reply.power,
            "confirmed": reply.confirmed,
            "window": reply.window,
            "incarnation": reply.incarnation.map(|incarnation| incarnation.id),
            "fence": reply.fence,
            "error": reply.error,
        }),
        Err(error) => json!({"ok": false, "error": error.error}),
    };
    // What was read counts only while the generation it was read in lives.
    if !keeper
        .shared
        .live()
        .is_some_and(|now| Arc::ptr_eq(&now, &session))
    {
        return refusal(
            Code::Transport,
            "The VM's session ended while it was being proved; ask again.",
        );
    }
    json!({
        "ok": true,
        "op": "observe",
        "generation": session.control.generation(),
        "peer": session.guest,
        "station": keeper.own.public(),
        "status": {"hello": hello, "inputService": input, "sessionProof": proof},
        "runner": runner,
    })
}

/// The guest's input service's own hello, read on an input lane; when the
/// guest has none, or it says none, why, as an observation of its own.
async fn input_service(session: &Session, deadline: Duration) -> Value {
    let unavailable = |error: &str| json!({"available": false, "error": error});
    if judge(session.control.declared(), &[INPUT_LANES])
        .refusal()
        .is_some()
    {
        return unavailable("This guest has no input service.");
    }
    let hello = final_reply(
        session,
        LaneKind::Input { change: None },
        &json!({"op": "hello"}),
        deadline,
    )
    .await;
    if hello["ok"] == true {
        return hello;
    }
    unavailable(
        hello["error"]
            .as_str()
            .unwrap_or("The guest's input service did not say hello."),
    )
}

/// The final reply to `request` on a lane of `kind`, or why there is none.
async fn final_reply(
    session: &Session,
    kind: LaneKind,
    request: &Value,
    deadline: Duration,
) -> Value {
    let mut last = None;
    let read = read(session, kind, request, deadline, |line| {
        last = Some(line);
        true
    })
    .await;
    match (read, last) {
        (Ok(()), Some(line)) => serde_json::from_str(&line).unwrap_or_else(|_| {
            refusal(Code::Protocol, "The guest said something not understood.")
        }),
        (Ok(()), None) => refusal(Code::Transport, "The guest did not answer."),
        (Err(error), _) => serde_json::to_value(&error).unwrap_or_default(),
    }
}

fn refusal(code: Code, why: &str) -> Value {
    serde_json::to_value(Error::new(code, why)).unwrap_or_default()
}

#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;
