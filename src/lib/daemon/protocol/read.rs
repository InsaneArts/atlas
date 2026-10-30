//! Why a file was not collected: one word for each refusal, the same on the
//! wire, in replies and in the helper daemon's words.
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Why a file was not collected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadRefusal {
    /// Not in the helper's home, its own temporary folder or `/tmp`, or in
    /// Atlas's own folder.
    Outside,
    /// In a folder macOS guards, or one that holds other apps' data.
    Protected,
    /// A link somewhere in the path.
    Symlink,
    /// A folder, device or pipe.
    NotRegular,
    /// Its contents are not on this Mac yet.
    Dataless,
    /// Not the helper's.
    Owner,
    /// Larger than allowed.
    TooLarge,
    /// Written in the last two seconds, or while it was read.
    Changing,
    Missing,
    Unreadable,
    /// No name that can be a file here.
    Name,
    /// A file of that name was already collected in this call.
    Exists,
    /// Not enough room here.
    Space,
    /// The helper changed since Atlas checked it.
    SessionChanged,
    /// The helper came on screen.
    OnConsole,
    /// The client went away while the file was read.
    Gone,
}

impl ReadRefusal {
    /// The reason a helper's refusal names, when it is one of these.
    pub fn of(reply: &Value) -> Option<Self> {
        Self::deserialize(&reply["reason"]).ok()
    }

    /// The word for it, as replies give it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Protected => "protected",
            Self::Symlink => "symlink",
            Self::NotRegular => "not-regular",
            Self::Dataless => "dataless",
            Self::Owner => "owner",
            Self::TooLarge => "too-large",
            Self::Changing => "changing",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::Name => "name",
            Self::Exists => "exists",
            Self::Space => "space",
            Self::SessionChanged => "session-changed",
            Self::OnConsole => "on-console",
            Self::Gone => "gone",
        }
    }

    /// What the helper daemon says of it, naming no file.
    pub fn said(self) -> &'static str {
        match self {
            Self::Outside => {
                "Only files in the helper's home, its temporary folder or /tmp are collected"
            }
            Self::Protected => {
                "Files in a folder macOS guards, such as Documents or Library, are never collected"
            }
            Self::Symlink => "A link anywhere in the path is never followed",
            Self::NotRegular => "Only regular files are collected, not folders, devices or pipes",
            Self::Dataless => "The file's contents are not on this Mac yet",
            Self::Owner => "The file does not belong to the helper",
            Self::TooLarge => "The file is larger than allowed",
            Self::Changing => "The file changed in the last 2 seconds, or while it was read",
            Self::Missing => "No such file in the helper",
            Self::Unreadable => "The file could not be read",
            Self::Name => "The file has no name that can be a file here",
            Self::Exists => "A file of that name was already collected",
            Self::Space => "There is not enough room for the file",
            Self::SessionChanged => "The helper changed since Atlas checked it",
            Self::OnConsole => "The helper is on the screen",
            Self::Gone => "The client went away",
        }
    }
}

impl std::fmt::Display for ReadRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_reason_reads_back_as_the_word_replies_give() {
        for reason in [
            ReadRefusal::Outside,
            ReadRefusal::Protected,
            ReadRefusal::Symlink,
            ReadRefusal::NotRegular,
            ReadRefusal::Dataless,
            ReadRefusal::Owner,
            ReadRefusal::TooLarge,
            ReadRefusal::Changing,
            ReadRefusal::Missing,
            ReadRefusal::Unreadable,
            ReadRefusal::Name,
            ReadRefusal::Exists,
            ReadRefusal::Space,
            ReadRefusal::SessionChanged,
            ReadRefusal::OnConsole,
            ReadRefusal::Gone,
        ] {
            assert_eq!(json!(reason), json!(reason.as_str()));
            assert_eq!(
                ReadRefusal::of(&json!({"reason": reason.as_str()})),
                Some(reason)
            );
        }
        assert_eq!(ReadRefusal::of(&json!({"reason": "later"})), None);
    }
}
