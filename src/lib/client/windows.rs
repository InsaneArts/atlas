//! Window lists read by the daemon in the verified helper session.
use super::{Client, Verified};
use crate::{
    error::{Code, Error, Result},
    native::windows::{Shown, from_reply},
    transport::refusal,
};
use serde_json::{Value, json};
use std::time::Duration;

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
    /// Read windows without starting a process. Older daemons without
    /// session pins are verified again, as for a pinned run.
    pub(crate) async fn windows_in(
        &self,
        verified: Option<&Verified>,
        dock: bool,
        limit: Duration,
    ) -> Result<Shown> {
        let read = async {
            let mut request = json!({"op": "windows", "dock": dock});
            let pin = match verified.and_then(Verified::pin) {
                Some(pin) => Some(pin),
                None => self.verify().await?.pin(),
            };
            if let Some(pin) = pin {
                request["session"] = json!(pin);
            }
            let mut transport = self.transport.clone();
            transport.timeout = limit;
            window_list(transport.call(&request).await?)
        };
        read.await.map_err(Error::certain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Helper, UID};
    use tokio::{io::AsyncReadExt, net::UnixListener};

    #[test]
    fn an_older_daemon_tells_the_person_to_restart_before_listing_windows() {
        let error = window_list(
            json!({"ok": false, "code": "bad-request", "error": "unknown op 'windows'"}),
        )
        .unwrap_err();
        assert_eq!(error.code, Code::Daemon);
        assert!(error.error.contains("older Atlas"));
        assert!(error.error.contains("atlas restart"));
        assert!(!error.uncertain);
    }

    #[test]
    fn a_window_reply_keeps_refusals_and_never_reads_malformed_data_as_an_empty_list() {
        let error =
            window_list(json!({"ok": false, "code": "session-changed", "error": "look again"}))
                .unwrap_err();
        assert_eq!(error.code, Code::SessionChanged);
        for reply in [
            json!({}),
            json!({"ok": true}),
            json!({"ok": true, "windows": [null]}),
        ] {
            assert_eq!(window_list(reply).unwrap_err().code, Code::Windows);
        }
        assert!(
            window_list(json!({"ok": true, "windows": [], "starts": {}}))
                .unwrap()
                .windows
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_window_read_keeps_its_deadline_and_never_leaves_an_uncertain_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let (mut client, helper) = Helper::client(directory.path(), UID);
        let verified = helper
            .serve(client.verify(), |_| panic!("only identity is read"))
            .await
            .unwrap();
        client.transport.path = directory.path().join("waiting.sock");
        let listener = UnixListener::bind(&client.transport.path).unwrap();
        let waiting = async {
            let (mut peer, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let byte = peer.read_u8().await.unwrap();
                if byte == b'\n' {
                    break;
                }
                request.push(byte);
            }
            let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
            assert_eq!(request["op"], "windows");
            assert_eq!(request["session"], json!(verified.pin().unwrap()));
            let mut byte = [0];
            assert_eq!(
                peer.read(&mut byte).await.unwrap(),
                0,
                "the deadline closes the read"
            );
        };
        let reading = client.windows_in(Some(&verified), false, Duration::from_millis(20));
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(reading, waiting)
        })
        .await
        .unwrap();
        let error = result.unwrap_err();
        assert_eq!(error.code, Code::Transport);
        assert!(!error.uncertain);
        assert!(client.controller.pending().unwrap().is_empty());
    }
}
