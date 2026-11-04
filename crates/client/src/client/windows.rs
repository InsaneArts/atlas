//! Window lists read by the daemon in the verified helper session.
use super::{Client, Verified};
use crate::last_capture;
use atlas_core::{
    error::{Code, Error, Result},
    screen::windows::{Bounds, Shown, Window, from_reply, hit_windows, same_routing},
    transport::refusal,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Bound an ordinary window observation, including both sides of a remembered look.
/// What the window list said of a look.
#[derive(Debug)]
pub(crate) enum Seen {
    /// The windows on screen, which stayed put through it.
    Stayed(Vec<Window>),
    /// The windows moved or changed while it ran.
    Moved,
    /// No window list was read for it.
    Unread,
}

pub(crate) const WINDOW_LOOKUP: Duration = Duration::from_secs(5);

fn window_list(reply: Value) -> Result<Shown> {
    if reply["ok"] == false {
        if reply["code"] == "bad-request" && reply["error"] == "unknown op 'windows'" {
            return Err(Error::new(
                Code::Daemon,
                "The helper is running an older Atlas that cannot list windows in its \
                 daemon; run atlas restart, then try again",
            ));
        }
        return Err(refusal(&reply));
    }
    from_reply(&reply)
        .ok_or_else(|| Error::new(Code::Windows, "The helper window list is unavailable"))
}

impl Client {
    /// Read windows without starting a process.
    pub(crate) async fn windows_in(
        &self,
        verified: Option<&Verified>,
        dock: bool,
        limit: Duration,
    ) -> Result<Shown> {
        let read = async {
            let pin = match verified {
                Some(verified) => verified.pin()?,
                None => self.verify().await?.pin()?,
            };
            let request = json!({"op": "windows", "dock": dock, "session": pin});
            let mut transport = self.transport.clone();
            transport.timeout = limit;
            window_list(transport.call(&request).await?)
        };
        read.await.map_err(Error::certain)
    }

    /// Carry out `look`, remembering for this host session which windows it
    /// saw when a window read on either side shows they stayed put.
    pub(crate) async fn look<T>(
        &self,
        verified: &Verified,
        look: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        self.look_seeing(verified, false, look)
            .await
            .map(|(result, _)| result)
    }

    /// [`Client::look`], with what the window list says of it: the windows
    /// on screen when they stayed put through the look. A look outside a host
    /// session reads no window list unless `need`ed.
    pub(crate) async fn look_seeing<T>(
        &self,
        verified: &Verified,
        need: bool,
        look: impl std::future::Future<Output = Result<T>>,
    ) -> Result<(T, Seen)> {
        if self.host_session.is_none() && !need {
            return Ok((look.await?, Seen::Unread));
        }
        let before = self.windows_in(Some(verified), true, WINDOW_LOOKUP).await?;
        let result = look.await?;
        let after = self
            .windows_in(Some(verified), false, WINDOW_LOOKUP)
            .await?;
        let unchanged = same_routing(&before.windows, &after.windows);
        self.remember_look(verified, unchanged.then_some(&before))?;
        let seen = if unchanged {
            Seen::Stayed(after.windows)
        } else {
            Seen::Moved
        };
        Ok((result, seen))
    }

    /// Remember what this host session last saw: `snapshot`, or that the
    /// windows moved under it when there is none. No host session keeps none.
    pub(crate) fn remember_look(
        &self,
        verified: &Verified,
        snapshot: Option<&Shown>,
    ) -> Result<()> {
        let Some(session) = &self.host_session else {
            return Ok(());
        };
        let epoch = &verified.session().epoch;
        let display = &epoch.display;
        let windows = snapshot.map(|snapshot| {
            hit_windows(
                &snapshot.windows,
                &Bounds::display(display.width, display.height),
                &snapshot.dock,
                &snapshot.starts,
            )
        });
        last_capture::remember_windows(self.output_dir()?, session.key(), windows.as_deref(), epoch)
    }
}

#[cfg(test)]
mod tests;
