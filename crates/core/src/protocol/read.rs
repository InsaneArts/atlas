//! Why a file was not collected: one word for each refusal, the same on the
//! wire, in replies and in the helper daemon's words.
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! read_refusals {
    ($($variant:ident => ($word:literal, $said:literal, $ends_the_call:literal)),+ $(,)?) => {
        /// Why a file was not collected, with its wire word and daemon message.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum ReadRefusal {
            $(#[serde(rename = $word)] $variant),+
        }

        impl ReadRefusal {
            pub fn said(self) -> &'static str {
                match self { $(Self::$variant => $said),+ }
            }

            /// Whether the helper itself changed, so no later file is read either.
            pub fn ends_the_call(self) -> bool {
                match self { $(Self::$variant => $ends_the_call),+ }
            }

            #[cfg(test)]
            const ALL: [Self; [$($word),+].len()] = [$(Self::$variant),+];
        }
    };
}

read_refusals! {
    Outside => ("outside", "Only files in the helper's home, its temporary folder or /tmp are collected", false),
    Protected => ("protected", "Files in a folder macOS guards, such as Documents or Library, are never collected", false),
    Symlink => ("symlink", "A link anywhere in the path is never followed; collect the file itself", false),
    NotRegular => ("not-regular", "Only regular files are collected, not folders, devices or pipes; archive a folder first with ditto -c -k", false),
    Dataless => ("dataless", "The file's contents are not on this Mac yet", false),
    Owner => ("owner", "The file does not belong to the helper", false),
    TooLarge => ("too-large", "The file is larger than allowed: pass --max-size (at most 1G), and collect at most 1 GiB a call", false),
    Changing => ("changing", "The file changed in the last 2 seconds, or while it was read; wait until whatever writes it is done", false),
    Missing => ("missing", "No such file in the helper", false),
    Unreadable => ("unreadable", "The file could not be read", false),
    Name => ("name", "The file has no name that can be a file here", false),
    Exists => ("exists", "A file of that name was already collected", false),
    Space => ("space", "There is not enough room for the file", false),
    SessionChanged => ("session-changed", "The helper changed since Atlas checked it", true),
    OnConsole => ("on-console", "The helper is on the screen", true),
    Gone => ("gone", "The client went away", false),
}

impl ReadRefusal {
    /// Unknown future reasons retain the caller's compatibility fallback.
    pub fn of(reply: &Value) -> Option<Self> {
        Self::deserialize(&reply["reason"]).ok()
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
