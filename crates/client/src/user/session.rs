//! The user desktop's sessions as this Mac sees them: a fresh proof of
//! its session from the shared Atlas command, and whether its login has
//! gone.
use crate::client::Client;
use atlas_core::{
    error::{Code, Error, Result},
    transport::run_output,
};
use serde_json::{Value, json};
use std::time::Duration;

impl Client {
    /// A fresh proof of the second login `login`'s session from a new run of
    /// the shared Atlas command, which it runs from this disk.
    pub(crate) async fn session_proof_by_command(
        &self,
        _login: &atlas_core::user::User,
    ) -> Result<Value> {
        let command = &self.config.identity_probe;
        let request = json!({
            "op": "run",
            "argv": [command, "session-info"],
            "timeoutMs": 3000,
        });
        let reply = self.transport.call(&request).await?;
        if reply["ok"] != true || reply["exitCode"] != 0 || reply["timedOut"] != false {
            return Err(Error::new(
                Code::Identity,
                format!(
                    "The helper cannot run the shared Atlas command at {}. Run atlas \
                     install.",
                    command.display()
                ),
            ));
        }

        let output = run_output(&reply, 4096)
            .map_err(|_| Error::new(Code::Identity, "Malformed or oversized identity output"))?;

        Ok(serde_json::from_slice(&output)?)
    }
}

impl Client {
    /// [`Client::desktop_gone`] for the second login `login`, whose
    /// processes this Mac can see.
    pub(crate) async fn user_gone(&self, login: &atlas_core::user::User, seconds: u64) -> bool {
        let uid = login.uid;
        let looked = move || async move {
            tokio::task::spawn_blocking(move || atlas_core::user::logins::logged_out(uid))
                .await
                .ok()
                .flatten()
                == Some(true)
        };
        let answers = || async { self.transport.call(&json!({"op": "hello"})).await.is_ok() };

        crate::client::session::gone_within(
            looked,
            answers,
            Duration::from_secs(seconds),
            Duration::from_millis(500),
        )
        .await
    }
}
