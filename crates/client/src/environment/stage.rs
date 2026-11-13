//! How a step's reply is read, whatever desktop ran it: which step it
//! answers, whether it settled, and the worker's own refusals.
use super::{reply::WorkerReply, *};

/// Say which step a result answers, so it is read in the right direction.
pub(crate) fn operation(mut result: Result<Value>, name: &str) -> Result<Value> {
    if let Ok(value) = &mut result {
        value["operation"] = json!(name);
    }
    result
}

/// Whether the helper confirmed the transaction reached `phase` with nothing
/// left to resume, so its frozen plan (copies of the selected files) is no
/// longer needed: rollback works from the helper's own records.
pub(crate) fn settled(result: &Result<Value>, phase: Phase) -> bool {
    result
        .as_ref()
        .is_ok_and(|value| WorkerReply::read(value).settled(phase))
}

/// The exit status of a worker that refused before running anything; its
/// error is its only output.
pub const WORKER_REFUSED: u8 = 125;

/// A run whose worker refused (no profile, a changed session) as that error;
/// any other reply is the command's own. A refusal names its private request
/// directory, which the command it would have run is never told, so even a
/// command that exits 125 with an error of the same shape stays the
/// command's result.
pub(crate) fn refused(reply: Value, request: &str) -> Result<Value> {
    if reply["exitCode"] != WORKER_REFUSED {
        return Ok(reply);
    }

    let error = run_output(&reply, 65536)
        .ok()
        .and_then(|output| serde_json::from_slice::<Value>(&output).ok())
        .filter(|value| {
            value["ok"] == false && value["error"].is_string() && value["refusal"] == request
        });
    match error {
        // The worker's own code is kept, so a changed session still reads as one.
        Some(error) => Err(Error::new(
            error["code"]
                .as_str()
                .and_then(atlas_core::error::Code::parse)
                .unwrap_or(Code::Environment),
            error["error"]
                .as_str()
                .unwrap_or("The helper environment refused"),
        )),
        None => Ok(reply),
    }
}

pub(crate) fn delivery_uncertain(result: &Result<Value>) -> bool {
    match result {
        Err(error) => error.uncertain,
        Ok(value) => value["timedOut"] == true || !value["signal"].is_null(),
    }
}

pub(crate) fn decode_reply(reply: &Value, mutating: bool, request: &str) -> Result<Value> {
    let invalid = |message: &str| {
        let error = problem(message);
        if mutating { error.uncertain() } else { error }
    };

    if reply["timedOut"] == true || !reply["signal"].is_null() {
        return Err(invalid(
            "Environment worker was interrupted; retain the plan and inspect or resume its transaction",
        ));
    }

    let output = run_output(reply, MAX_RECORD as usize).map_err(|error| match error {
        OutputError::Malformed => invalid("Malformed environment output"),
        OutputError::Oversized => invalid("Environment output exceeded its limit"),
    })?;
    let mut value: Value = serde_json::from_slice(&output).map_err(|cause| {
        invalid("Environment worker did not return a complete result").with_source(cause)
    })?;
    if !value.is_object()
        || !value["ok"].is_boolean()
        || (reply["exitCode"] != 0 && value["ok"] != false)
    {
        return Err(invalid("Environment worker returned an invalid result"));
    }

    // The worker of `request` refuses before it changes anything (another
    // operation is running, the plan changed, nothing was copied with that
    // ID): there is nothing to resume. Any other failed mutation may have
    // stopped part way.
    let refused = reply["exitCode"] == WORKER_REFUSED && value["refusal"] == request;
    if mutating && value["ok"] == false && !refused {
        value["resumeRequired"] = json!(true);
    }

    Ok(value)
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
