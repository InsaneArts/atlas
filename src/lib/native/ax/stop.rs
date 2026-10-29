//! Why a scan is incomplete, the words a reply names each reason by, and
//! which recorded AX errors say something went wrong.
use super::ABSENT;
use serde::Serialize;

/// Why a scan is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stop {
    /// Some elements are nested deeper than the scan reads.
    Depth,
    /// The app has more elements than the scan reads.
    Count,
    /// The scan ran out of time.
    Time,
    /// Part of the app could not be read.
    Unread,
    /// The app handed back itself in place of its windows.
    Withheld,
}

impl Stop {
    pub(super) const ALL: [Stop; 5] = [
        Stop::Depth,
        Stop::Count,
        Stop::Time,
        Stop::Unread,
        Stop::Withheld,
    ];

    /// The word a reply names it by.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Stop::Depth => "depth",
            Stop::Count => "count",
            Stop::Time => "time",
            Stop::Unread => "unread",
            Stop::Withheld => "withheld",
        }
    }

    /// The reason a reply names by `word`, if it is one of these.
    pub(crate) fn parse(word: &str) -> Option<Stop> {
        Stop::ALL.into_iter().find(|stop| stop.word() == word)
    }

    /// Why the scan stopped, in plain words.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Stop::Time => "it ran out of time",
            Stop::Count => "the app has more elements than Atlas reads",
            Stop::Depth => "some elements are nested too deep",
            Stop::Unread => "part of the app could not be read",
            Stop::Withheld => {
                "the app withheld its windows, as macOS does while the Mac's screen is locked"
            }
        }
    }
}

/// Why a scan stopped, in plain words joined into one phrase; None when
/// there is no reason.
pub(crate) fn why(stops: impl IntoIterator<Item = Stop>) -> Option<String> {
    let said: Vec<&str> = stops.into_iter().map(Stop::said).collect();

    match said.as_slice() {
        [] => None,
        [one] => Some((*one).to_owned()),
        [rest @ .., last] => Some(format!("{} and {last}", rest.join(", "))),
    }
}

/// Whether an `"attribute:code"` error key says something went wrong,
/// rather than that the element simply lacks the attribute.
pub(crate) fn telling(key: &str) -> bool {
    let code = key
        .rsplit_once(':')
        .and_then(|(_, code)| code.parse::<i32>().ok());

    code.is_none_or(|code| !ABSENT.iter().any(|absent| absent.0 == code))
}
