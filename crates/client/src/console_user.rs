//! Whether macOS names the person's own account as the one at the screen.
//!
//! macOS names one account as its console user: the last to log in or to
//! come onto the screen. Its own agents read that name, not the window
//! server's record, to tell whether their login is the one in use: an agent
//! of any other account quits at once, and launchd starts it again every
//! ten seconds (see [`crate::console_agents`]). A login made off the
//! screen, as through Screen Sharing, is named too, though the person never
//! left the screen. From then on the agents that restart are those of the
//! person's own login, and they go on after that other login has logged
//! out: macOS names the person again only when their login next comes onto
//! the screen, as after a switch to the login window and back.
use atlas_native::native::{cf, console_user::ConsoleUser};

/// Whether macOS names this login's account as the one at the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Named {
    /// It names this account, or this login is not the one on the screen.
    This,
    /// This login is on the screen and macOS names another account, by
    /// this name.
    Another(String),
    /// This process runs in no login of its own account, or macOS named no
    /// account.
    Unknown,
}

impl Named {
    /// From the login a process of account `uid` runs in and the account
    /// macOS names, each `None` when there is none to read.
    fn of(login: Option<Login>, named: Option<ConsoleUser>, uid: u32) -> Self {
        match (login, named) {
            // A shell switched to another account runs in the login it was
            // started from, which says nothing of that account's own.
            (Some(login), _) if login.uid != uid => Self::Unknown,
            (Some(login), _) if !login.on_screen => Self::This,
            (Some(_), Some(named)) if named.uid == uid => Self::This,
            (Some(_), Some(named)) => Self::Another(named.name),
            (Some(_), None) | (None, _) => Self::Unknown,
        }
    }

    /// As it is now for the login this process runs in. It asks the window
    /// server and macOS, and neither sets a limit on its answer: a caller
    /// that must not wait bounds the call itself.
    pub fn now() -> Self {
        Self::of(
            Login::this(),
            atlas_native::native::console_user::named(),
            nix::unistd::getuid().as_raw(),
        )
    }
}

/// The login a process runs in, as the window server records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Login {
    /// The account it belongs to.
    pub(crate) uid: u32,
    /// Whether it is the one on the screen.
    pub(crate) on_screen: bool,
}

impl Login {
    /// The one this process runs in, or `None` when it runs in none, as in
    /// a remote shell, or the window server leaves out whose it is or
    /// whether it is on the screen.
    pub(crate) fn this() -> Option<Self> {
        let record = cf::session()?;
        Some(Self {
            uid: u32::try_from(cf::integer(&record, "kCGSSessionUserIDKey")?).ok()?,
            on_screen: cf::boolean(&record, "kCGSSessionOnConsoleKey")
                .or_else(|| cf::boolean(&record, "kCGSessionOnConsoleKey"))?,
        })
    }
}

#[cfg(test)]
#[path = "console_user_tests.rs"]
mod tests;
