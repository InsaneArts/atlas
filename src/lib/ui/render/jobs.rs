//! Views of background jobs and collected files: what was started or
//! stopped, the jobs of a task, the end of a job's output, and the copies
//! a collection kept. Processes Atlas left running and files it did not
//! collect are always said.
use super::{Rendered, failed, style, text};
use serde_json::Value;

/// A job that was started or stopped: its ID and how far it got, then the
/// processes that left its group, which still run.
pub fn job(value: &Value) -> Rendered {
    if let Some(failure) = failed(value) {
        return failure;
    }
    let id = value["id"].as_str().unwrap_or("?");
    let mut lines = vec![match value["state"].as_str() {
        Some("ended") => match value["how"].as_str() {
            Some(how) => format!("{} Job {id} ended: {}", style::ok(), how_said(how)),
            None => format!("{} Job {id} ended", style::ok()),
        },
        Some(state) => format!(
            "{} Job {id} ({}) is {state}, process {}",
            style::ok(),
            text::printable(value["name"].as_str().unwrap_or("job")),
            value["pid"]
        ),
        None => format!("{} Job {id}", style::ok()),
    }];
    lines.extend(left(value));
    lines.extend(warnings(value));
    Rendered::out(lines.join("\n"))
}

/// A task's jobs, one a line, with what each left running.
pub fn jobs(value: &Value) -> Rendered {
    if let Some(failure) = failed(value) {
        return failure;
    }
    let jobs = value["jobs"].as_array().map_or(&[][..], Vec::as_slice);
    let mut lines = Vec::new();
    if jobs.is_empty() {
        lines.push(format!("{} No background jobs", style::ok()));
    }
    for job in jobs {
        let state = job["state"].as_str().unwrap_or("unknown");
        let mut line = format!(
            "{} {} {}",
            job["id"].as_str().unwrap_or("?"),
            text::printable(job["name"].as_str().unwrap_or("job")),
            match (state, job["how"].as_str()) {
                ("ended", Some(how)) => format!("ended: {}", how_said(how)),
                (state, _) => state.to_owned(),
            }
        );
        if job["mismatch"] == true {
            line.push_str(&format!(
                " {}",
                style::dim("· the helper's record names another process than the task's")
            ));
        }
        if let Some(refused) = job["refused"].as_str() {
            line.push_str(&format!(
                " {}",
                style::dim(format!("· not signalled: {refused}"))
            ));
        }
        lines.push(line);
        lines.extend(left(job));
    }
    lines.extend(warnings(value));
    Rendered::out(lines.join("\n"))
}

/// The end of a job's output, as it is, and whether more came before it.
pub fn log(value: &Value) -> Rendered {
    if let Some(failure) = failed(value) {
        return failure;
    }
    let mut rendered = Rendered::out(value["output"].as_str().unwrap_or("").to_owned());
    if value["outputTruncated"] == true {
        rendered.err = format!(
            "{} This is the end of {} bytes of output; --bytes shows more.",
            style::warn(),
            value["size"]
        );
    }
    rendered
}

/// What a collection kept, one copy a line, and what it did not.
pub fn collect(value: &Value) -> Rendered {
    let mut lines = Vec::new();
    for kept in value["collected"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{} {} ({} bytes, SHA-256 {})",
            style::ok(),
            text::printable(kept["path"].as_str().unwrap_or("?")),
            kept["size"],
            kept["sha256"].as_str().unwrap_or("?")
        ));
    }
    for file in value["failed"].as_array().into_iter().flatten() {
        lines.push(format!(
            "{} {}",
            style::fail(),
            text::printable(file["error"].as_str().unwrap_or("not collected"))
        ));
    }
    let out = lines.join("\n");
    match failed(value) {
        Some(failure) => Rendered {
            out,
            err: failure.err,
        },
        None => Rendered::out(out),
    }
}

/// How a job ended, in words.
fn how_said(how: &str) -> &str {
    match how {
        "exited" => "it exited",
        "stopped" => "stopped",
        "timeout" => "its time ran out",
        "person" => "the person stopped agents",
        "on-console" => "the helper came on screen",
        "gone" => "found gone",
        "restarted" => "the Mac restarted",
        "leader-gone" => "its first process is gone, so Atlas signalled nothing",
        other => other,
    }
}

/// The processes `value` says still run outside the job, never signalled.
fn left(value: &Value) -> Option<String> {
    let left: Vec<String> = value["left"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|process| {
            format!(
                "{} ({})",
                text::printable(process["name"].as_str().unwrap_or("a process")),
                process["pid"]
            )
        })
        .collect();
    (!left.is_empty()).then(|| {
        format!(
            "  {}",
            style::dim(format!(
                "still running, never signalled: {}",
                left.join(", ")
            ))
        )
    })
}

/// The warnings `value` carries, each on a line.
fn warnings(value: &Value) -> Vec<String> {
    value["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|warning| format!("{} {}", style::warn(), text::printable(warning)))
        .collect()
}
