//! What this Mac's directory services say of an account beside its name and
//! ID: its full name, and whether it administers the Mac.
use crate::account::Title;
use std::process::Command;

/// The full name of account `name`, as people know it, if it has one.
pub fn real_name(name: &str) -> Option<String> {
    asked_real_name(name).titled()
}

pub fn asked_real_name(name: &str) -> Title {
    let Ok(output) = crate::bounded::output(
        Command::new("/usr/bin/dscl").args([".", "-read", &format!("/Users/{name}"), "RealName"]),
        crate::bounded::LOOKUP,
    ) else {
        return Title::Unknown;
    };

    parse_real_name(&output.stdout).map_or(Title::Untitled, Title::Titled)
}

/// The full name in `dscl -read`'s answer for RealName, if it has one.
fn parse_real_name(answer: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(answer).ok()?;
    let value = text.strip_prefix("RealName:")?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Whether `name` administers this Mac (a helper must not); None when macOS
/// cannot say, which never counts as "no".
pub fn administrator(name: &str) -> Option<bool> {
    // An agent's sandbox may keep dseditgroup from the directory service;
    // the account's group list still answers there.
    directory_member(name).or_else(|| listed_member(name))
}

fn directory_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/sbin/dseditgroup").args(["-o", "checkmember", "-m", name, "admin"]),
        crate::bounded::LOOKUP,
    )
    .ok()?;

    let answer = String::from_utf8_lossy(&output.stdout);
    if answer.starts_with("yes ") {
        Some(true)
    } else if answer.starts_with("no ") {
        Some(false)
    } else {
        None
    }
}

/// The admin group's ID, the same on every Mac.
const ADMIN_GID: &str = "80";

/// Whether the admin group is among the group IDs `id` lists for `name`.
/// IDs, not names: a name `id` cannot resolve is printed as its number.
fn listed_member(name: &str) -> Option<bool> {
    let output = crate::bounded::output(
        Command::new("/usr/bin/id").args(["-G", name]),
        crate::bounded::LOOKUP,
    )
    .ok()
    .filter(|output| output.status.success())?;

    parse_admin_listed(&String::from_utf8_lossy(&output.stdout))
}

/// Whether `id -G`'s list holds the admin group's ID; None for no list.
fn parse_admin_listed(groups: &str) -> Option<bool> {
    let mut groups = groups.split_whitespace().peekable();
    groups.peek()?;

    Some(groups.any(|group| group == ADMIN_GID))
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
