//! Window lists read by the daemon in the verified helper session.
use super::{Client, Verified};
use crate::last_capture;
use atlas_core::{
    error::{Code, Error, Result},
    screen::windows::{Bounds, Shown, from_reply, hit_windows, same_routing},
    transport::refusal,
};
use serde_json::{Value, json};
use std::time::Duration;

/// Bound an ordinary window observation, including both sides of a remembered look.
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
        if self.host_session.is_none() {
            return look.await;
        }
        let before = self.windows_in(Some(verified), true, WINDOW_LOOKUP).await?;
        let result = look.await?;
        let after = self
            .windows_in(Some(verified), false, WINDOW_LOOKUP)
            .await?;
        let unchanged = same_routing(&before.windows, &after.windows).then_some(&before);
        self.remember_look(verified, unchanged)?;
        Ok(result)
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
