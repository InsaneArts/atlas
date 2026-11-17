//! An environment change on a machine desktop: its worker is the guest's
//! own Atlas, handed its packet on its standard input. What a change to the
//! guest's records came to is the worker's own answer, and a process that
//! ended is no answer: a worker cut off by a signal or a timeout, or one
//! whose answer did not come whole, leaves what it did unknown, kept as
//! uncertain until its work is proven ended. A command the worker runs
//! replaces it, so what it came to is the command's own end, as any
//! command's is, unless the worker refused before it ran.
use super::Printed;
use crate::worker::{ANSWER, REFUSED};
use atlas_core::{
    error::Code,
    protocol::{failure, interrupted},
};
use serde_json::{Value, json};
use std::path::Path;

/// What an environment change hands its worker, and how its answer is read.
pub(in crate::daemon) struct Handed {
    /// The packet, as the worker reads it.
    pub(in crate::daemon) packet: Vec<u8>,
    /// Whether the worker runs a command, whose output is the command's own
    /// and whose end is what it came to, as any command's is.
    command: bool,
    /// The name its refusal gives, fresh for this run and handed to it in
    /// its packet alone: a command it runs never learns it, so it can never
    /// pass its own output off as the worker's refusal.
    refusal: String,
}

impl Handed {
    /// What `packet` hands the worker, with a fresh name for its refusal.
    pub(in crate::daemon) fn of(packet: &Value) -> Self {
        let refusal = atlas_core::files::Token::new().as_str().to_owned();
        let mut packet = packet.clone();
        if let Some(fields) = packet.as_object_mut() {
            fields.insert("refusal".to_owned(), json!(refusal));
        }
        Self {
            packet: serde_json::to_vec(&packet).unwrap_or_default(),
            command: packet["request"]["operation"] == "run",
            refusal,
        }
    }

    /// The worker's words: `worker`, this daemon's own executable, never a
    /// program a request names, with `--apply` for a request that may
    /// change the desktop.
    pub(in crate::daemon) fn argv(worker: &Path, apply: bool) -> Result<Vec<String>, Value> {
        let worker = worker.to_str().ok_or_else(|| {
            failure(
                Code::SpawnFailed,
                "The desktop's own Atlas cannot be named to run its environment",
            )
        })?;
        let mut argv = vec![
            worker.to_owned(),
            "environment-worker".to_owned(),
            "--machine".to_owned(),
        ];
        if apply {
            argv.push("--apply".to_owned());
        }
        Ok(argv)
    }

    /// Whether only the worker's answer reaches the station: its output is
    /// that answer, never a command's.
    pub(in crate::daemon) fn answers(&self) -> bool {
        !self.command
    }

    /// What the change came to, from the run's `reply` and what the worker
    /// `printed`: the reply to keep, and for a command, that reply with what
    /// it printed.
    pub(in crate::daemon) fn answered(
        &self,
        reply: &Value,
        printed: &Printed,
    ) -> (Value, Option<Value>) {
        // Refused before it ran, or ended unknown: said as it is.
        if reply["ok"] != true {
            return (reply.clone(), None);
        }
        if self.command {
            return match refused(reply, printed, &self.refusal) {
                Some(refusal) => (refusal, None),
                None => (reply.clone(), Some(printed.kept(reply))),
            };
        }
        if reply["timedOut"] == true || !reply["signal"].is_null() {
            return (
                interrupted(
                    Code::Uncertain,
                    "The environment worker was ended before it answered, so what it did is unknown",
                ),
                None,
            );
        }
        match answer(reply, printed) {
            // The worker itself could not say what it did: the change is
            // unknown, its words kept beside it.
            Some(answer) if answer["uncertain"] == true => {
                let code = answer["code"]
                    .as_str()
                    .and_then(Code::parse)
                    .unwrap_or(Code::Uncertain);
                let error = answer["error"]
                    .as_str()
                    .unwrap_or("The environment worker could not say what it did");
                let mut unknown = interrupted(code, error);
                unknown["result"] = answer;
                (unknown, None)
            }
            Some(answer) => (
                json!({"ok": true, "op": "environment", "exitCode": reply["exitCode"],
                    "result": answer}),
                None,
            ),
            None => (
                interrupted(
                    Code::Uncertain,
                    "The environment worker did not give a whole answer, so what it did is unknown",
                ),
                None,
            ),
        }
    }
}

/// The refusal a worker that runs a command gave before it ran it, in its
/// own words: it exited [`REFUSED`] having printed nothing but that refusal,
/// named `name`, the name only its packet told it. Anything else a command
/// prints is never read; what it came to is its own end.
fn refused(reply: &Value, printed: &Printed, name: &str) -> Option<Value> {
    if reply["exitCode"] != REFUSED || printed.truncated {
        return None;
    }
    let refusal: Value = serde_json::from_slice(&printed.output).ok()?;
    if refusal["ok"] != false || refusal["refusal"] != name {
        return None;
    }
    let code = refusal["code"]
        .as_str()
        .and_then(Code::parse)
        .unwrap_or(Code::Environment);
    Some(failure(code, refusal["error"].as_str()?))
}

/// The worker's answer, all it printed: one JSON object saying whether it
/// succeeded, a success only from a worker that exited cleanly. None when
/// it is cut short, more than [`ANSWER`], or anything else.
fn answer(reply: &Value, printed: &Printed) -> Option<Value> {
    if printed.truncated
        || printed.output.len() > ANSWER
        || reply["outputTruncated"] == true
        || reply["outputUnfinished"] == true
    {
        return None;
    }
    let answer: Value = serde_json::from_slice(&printed.output).ok()?;
    let whole = answer.is_object()
        && answer["ok"].is_boolean()
        && (reply["exitCode"] == 0 || answer["ok"] == false);
    whole.then_some(answer)
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
