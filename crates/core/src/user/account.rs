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

pub use crate::account::administrator;

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;
