//! Apple jobs that keep restarting in the helper, read from launchd's log.
//!
//! Some macOS agents work only for the login on the screen: in the helper
//! they quit a few milliseconds after starting, and launchd starts them
//! again every ten seconds for as long as the helper stays logged in. With
//! System Integrity Protection on, launchd ignores a disable for them, so
//! nothing Atlas may do stops it. One of them leaves a window server
//! connection behind at many starts, through a macOS bug, which slowly
//! slows the whole Mac; logging the helper out clears what has built up.
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// Where launchd logs each job it starts and how long it ran; anyone may
/// read it.
const LAUNCHD_LOG: &str = "/private/var/log/com.apple.xpc.launchd/launchd.log";
/// How many of launchd's logs are read at most: the current one and those
/// it moved aside at about 7 MB each, `launchd.log.1` and on.
const LOGS: usize = 4;
/// The most of each log's end read: more than launchd lets one grow to.
const LOG_TAIL: u64 = 16 * 1024 * 1024;
/// A job that ran for less than this many milliseconds quit at once.
const SHORT_RUN_MS: u64 = 1_000;
/// More short runs than this in an hour is a job restarting over and over.
const RUNS_AN_HOUR: usize = 30;
/// One hour, in seconds.
const HOUR: i64 = 60 * 60;
/// How far from a whole hour a step forward may be and still be a clock set
/// forward for summer time, in seconds.
const CLOCK_SLACK: i64 = 2 * 60;
/// How many times the logs are read while launchd keeps moving them aside.
const READS: usize = 3;

/// The agents whose restarts in the helper slow the Mac: each start opens
/// a window server connection, which the window server keeps when the agent
/// has quit before it took the connection in.
pub const SLOWING: &[&str] = &[
    // audioaccessoryd: "Exiting: Current console ID: …, getuid: …".
    "com.apple.BTServer.cloudpairing",
];

/// Whether an agent that slows the Mac keeps restarting in the helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Restarting {
    /// None does.
    #[serde(rename = "none")]
    Quiet,
    /// One does, so the Mac slows while the helper stays logged in.
    Restarting,
    /// launchd's log could not be read.
    Unknown,
}

impl Restarting {
    /// From the jobs restarting in the helper, or `None` when launchd's log
    /// could not be read.
    pub fn of(jobs: Option<&[Restart]>) -> Self {
        match jobs {
            None => Self::Unknown,
            Some(jobs) if jobs.iter().any(|job| SLOWING.contains(&job.label.as_str())) => {
                Self::Restarting
            }
            Some(_) => Self::Quiet,
        }
    }
}

/// An Apple job in the helper that keeps quitting at once and restarting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Restart {
    pub label: String,
    /// Its short runs in the last hour of the log.
    pub runs: usize,
}

/// The Apple jobs in helper `uid`'s domains that ran for under a second
/// more than 30 times in the last hour of launchd's log, most first.
pub fn restarting(log: &str, uid: u32) -> Vec<Restart> {
    let domains = [format!("gui/{uid}/"), format!("user/{uid}/")];
    let lines: Vec<(i64, &str)> = log
        .lines()
        .filter_map(|line| Some((seconds(line)?, line)))
        .collect();

    // The hour is counted back from the newest line, step by step, so a
    // clock set back or forward does not stretch or cut it.
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut since = 0;
    let mut later = None;
    for &(at, line) in lines.iter().rev() {
        if let Some(later) = later {
            since += step(at, later);
        }
        if since > HOUR {
            break;
        }
        later = Some(at);
        if let Some((label, ms)) = exit(line, &domains)
            && ms < SHORT_RUN_MS
            && label.starts_with("com.apple.")
        {
            *counts.entry(label).or_default() += 1;
        }
    }

    let mut found: Vec<Restart> = counts
        .into_iter()
        .filter(|(_, runs)| *runs > RUNS_AN_HOUR)
        .map(|(label, runs)| Restart {
            label: label.to_owned(),
            runs,
        })
        .collect();
    found.sort_by(|a, b| b.runs.cmp(&a.runs).then_with(|| a.label.cmp(&b.label)));
    found
}

/// [`restarting`] from launchd's logs on this Mac, the newest first and
/// as many of the ones launchd moved aside as the last hour needs; `None`
/// when the newest cannot be read.
pub fn read_restarting(uid: u32) -> Option<Vec<Restart>> {
    let paths: Vec<PathBuf> = (0..LOGS)
        .map(|index| match index {
            0 => PathBuf::from(LAUNCHD_LOG),
            index => PathBuf::from(format!("{LAUNCHD_LOG}.{index}")),
        })
        .collect();
    let log = read_logs(&paths)?;
    Some(restarting(&log, uid))
}

/// Whether a status reply is the helper's own answer, so it is logged in
/// and what launchd's log says of its jobs is about this login.
pub fn answered(reply: &Value) -> bool {
    reply["ok"] == true
}

/// The logs at `paths`, newest first: the newest, after the older ones
/// while together they hold less than an hour, oldest first. launchd may
/// move the logs aside while they are read, and an older one is then one
/// read already: all are read again rather than counted twice, a few times
/// at most, and the newest alone is left.
fn read_logs(paths: &[PathBuf]) -> Option<String> {
    let (newest_path, older_paths) = paths.split_first()?;
    let mut alone = None;
    for _ in 0..READS {
        let (newest, file) = tail(newest_path, LOG_TAIL)?;
        let mut held = span(&newest);
        let mut oldest = first_time(&newest);
        let mut files = vec![file];
        let mut older = Vec::new();
        let mut moved = false;
        for path in older_paths {
            if held >= HOUR {
                break;
            }
            let Some((text, file)) = tail(path, LOG_TAIL) else {
                break;
            };
            if files.contains(&file) {
                moved = true;
                break;
            }
            files.push(file);
            held += span(&text)
                + last_time(&text)
                    .zip(oldest)
                    .map_or(0, |(end, start)| step(end, start));
            oldest = first_time(&text).or(oldest);
            older.push(text);
        }
        if !moved {
            older.reverse();
            older.push(newest);
            return Some(joined(older));
        }
        alone = Some(newest);
    }
    alone
}

/// The time from one line to the next, `earlier` then `later`, in the
/// log's local clock. launchd writes many lines a minute, so a step back is
/// the clock set back and a step forward of about an hour is the clock set
/// forward, as summer time does: neither counts.
fn step(earlier: i64, later: i64) -> i64 {
    let gap = later - earlier;
    if gap < 0 || (HOUR - CLOCK_SLACK..=HOUR + CLOCK_SLACK).contains(&gap) {
        0
    } else {
        gap
    }
}

/// The time `log`'s lines span, step by step.
fn span(log: &str) -> i64 {
    let times: Vec<i64> = log.lines().filter_map(seconds).collect();
    times
        .iter()
        .zip(times.iter().skip(1))
        .map(|(earlier, later)| step(*earlier, *later))
        .sum()
}

/// The time of the first line of `log` that has one.
fn first_time(log: &str) -> Option<i64> {
    log.lines().find_map(seconds)
}

/// The time of the last line of `log` that has one.
fn last_time(log: &str) -> Option<i64> {
    log.lines().rev().find_map(seconds)
}

/// `logs`, oldest first, as one text of whole lines.
fn joined(logs: Vec<String>) -> String {
    let mut text = String::new();
    for log in logs {
        text.push_str(&log);
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
    }
    text
}

/// The last `most` bytes of the file at `path`, from the first whole line,
/// and which file it was, by its inode.
fn tail(path: &Path, most: u64) -> Option<(String, u64)> {
    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    let (length, inode) = (metadata.len(), metadata.ino());
    let start = length.saturating_sub(most);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let text = if start == 0 {
        text
    } else {
        text.split_once('\n')
            .map_or_else(String::new, |(_, rest)| rest.to_owned())
    };
    Some((text, inode))
}

/// A log line's time, `2026-11-08 12:32:44.833619`, in seconds of its own
/// clock: only differences between lines are used, so the time zone does
/// not matter.
fn seconds(line: &str) -> Option<i64> {
    let stamp = line.get(..19)?.as_bytes();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = stamp.get(range)?;
        digits
            .iter()
            .all(u8::is_ascii_digit)
            .then(|| std::str::from_utf8(digits).ok()?.parse().ok())
            .flatten()
    };
    let separated = [(4, b'-'), (7, b'-'), (10, b' '), (13, b':'), (16, b':')]
        .iter()
        .all(|&(at, byte)| stamp.get(at) == Some(&byte));
    if !separated {
        return None;
    }
    let days = days_from_civil(number(0..4)?, number(5..7)?, number(8..10)?);
    Some(days * 86_400 + number(11..13)? * 3_600 + number(14..16)? * 60 + number(17..19)?)
}

/// The days from 1970-01-01 to a date of the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let from_march = (month + 9) % 12;
    let of_year = (153 * from_march + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

/// The label and run time of a job's exit in one of `domains`, from a line
/// such as `… (gui/503/com.apple.X [4242]) <Notice>: exited due to
/// exit(0), ran for 63ms`.
fn exit<'a>(line: &'a str, domains: &[String]) -> Option<(&'a str, u64)> {
    let (_, after) = line.split_once(" (")?;
    let (target, message) = after.split_once(')')?;
    let target = target.split(' ').next()?;
    let label = domains
        .iter()
        .find_map(|domain| target.strip_prefix(domain.as_str()))?;
    if label.is_empty() || !message.contains(": exited due to ") {
        return None;
    }
    let (_, ran) = message.rsplit_once(", ran for ")?;
    let ms = ran.trim().strip_suffix("ms")?.parse().ok()?;
    Some((label, ms))
}

#[cfg(test)]
mod tests;
