//! The person's Stop and Resume: kept on the person's side first, told to
//! the helper's input service, and ending the helper's background jobs.
//! Agents' commands read it before they change the helper, and a command
//! under way hears of it from either side's saved marker.
use super::{Actor, Client};
use crate::{
    daemon::protocol::{JobReply, StoppedAll},
    error::{Code, Error, Result},
    session::{HOLD, has_capability},
    transport::Transport,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

impl Client {
    /// Stop agents in the helper (`on`), or let them go on. Stopping ends
    /// an agent's input under way and refuses its next actions; the person
    /// keeps the viewer. Only the person resumes, through a viewer. An input
    /// service too old to stop is refused.
    pub async fn hold(&self, on: bool) -> Result<Value> {
        let held = self.hold_input(on).await;
        if !on {
            return held;
        }

        // Background jobs end with a Stop too. The helper daemon also sees
        // the input service's saved stop, so one that is down ends them
        // itself once it starts; one that cannot be told is said.
        let mut reply = held?;
        let (jobs, warning) = self.end_jobs_for_stop().await;
        if let Some(jobs) = jobs {
            reply["jobs"] = jobs;
        }
        if let Some(warning) = warning {
            match reply["warnings"].as_array_mut() {
                Some(warnings) => warnings.push(json!(warning)),
                None => reply["warnings"] = json!([warning]),
            }
        }
        Ok(reply)
    }

    /// Ask the helper daemon to end every background job for the person's
    /// Stop, within two seconds and never under control, so the person is
    /// never queued: what it said of the jobs (stopping, ended, refused,
    /// unknown and unresolved), and a warning when some may still run or it
    /// could not be told. A daemon that is not running, or that keeps no
    /// jobs, has none to end.
    async fn end_jobs_for_stop(&self) -> (Option<Value>, Option<String>) {
        let daemon = Transport {
            path: self.config.socket_path.clone(),
            timeout: Duration::from_secs(2),
            peer_uid: self.transport.peer_uid,
        };
        let request = json!({"op": "job", "action": "stop-all", "reason": "person"});
        match daemon.call(&request).await {
            // A job it would not signal, could not see, or whose record it
            // cannot read may still run.
            Ok(reply) if reply["ok"] == true => {
                let jobs = match JobReply::deserialize(&reply) {
                    Ok(JobReply::StopAll(jobs)) => jobs,
                    _ => StoppedAll::default(),
                };
                let left = jobs.refused.len() + jobs.unknown.len() + jobs.unresolved.len();
                let warning = (left > 0).then(|| {
                    format!(
                        "The helper did not stop {}, as it could not tell they are still Atlas's; atlas job list --all shows {}",
                        crate::ui::text::count(left, "background job"),
                        if left == 1 { "it" } else { "them" }
                    )
                });
                (Some(json!(jobs)), warning)
            }
            Ok(reply) if reply["code"] == "bad-request" => (None, None),
            Err(error) if error.code == Code::Io && !self.config.socket_path.exists() => {
                (None, None)
            }
            _ => (
                None,
                Some("Atlas could not reach the helper to end its background jobs".to_owned()),
            ),
        }
    }

    /// [`Client::hold`] in the input service and on the person's side.
    async fn hold_input(&self, on: bool) -> Result<Value> {
        if !on && self.actor != Actor::Viewer {
            return Err(Error::new(
                Code::Stopped,
                "Only the person resumes agents, from the Atlas menu bar",
            ));
        }

        // The person's side keeps the Stop first, so it holds even when the
        // helper's input service is down or older: agents' commands read it.
        if on {
            self.controller.note_stop(true)?;
        }

        let unreachable = |why: &str| {
            if !on {
                if !matches!(
                    self.config
                        .input_socket
                        .with_extension("stopped")
                        .try_exists(),
                    Ok(false)
                ) {
                    return Err(Error::new(
                        Code::Stopped,
                        format!("{why} Recover input before agents can resume."),
                    ));
                }
                self.controller.note_stop(false)?;
            }
            Ok(json!({
                "ok": true,
                "held": on,
                "kept": true,
                "warnings": [format!("{why} Agents are {} here all the same.", if on { "stopped" } else { "free to go on" })],
            }))
        };
        let Ok(hello) = self.input_transport().call(&json!({"op": "hello"})).await else {
            return unreachable("The helper's input service isn't running.");
        };
        if !has_capability(&hello, HOLD) {
            return unreachable("The helper's input service is from an older build.");
        }

        let mut reply = match self.native_request(json!({"op": "hold", "on": on})).await {
            Ok(reply) => reply,
            Err(error) if on => {
                let mut reply = if error.uncertain {
                    unreachable("The helper's input service may or may not have been told.")
                } else {
                    unreachable(&format!(
                        "The helper's input service reported a problem while stopping: {}.",
                        error.error.trim_end_matches('.')
                    ))
                }?;
                if error.uncertain {
                    reply["uncertain"] = json!(true);
                }
                return Ok(reply);
            }
            Err(error) => return Err(error),
        };
        if !on {
            self.controller.note_stop(false)?;
        }
        if reply["kept"] == false {
            reply["warnings"] = json!([
                "The helper could not save this, so it lasts only until its input restarts"
            ]);
        }

        Ok(reply)
    }

    /// Once the person stops agents: either side's saved marker appears.
    pub(super) async fn stopped_while_running(&self) {
        let marker = self.config.input_socket.with_extension("stopped");
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if marker.exists() || self.controller.stop_noted() {
                return;
            }
        }
    }

    /// Refuse an agent's command that would change the helper while the
    /// person has stopped agents. Input is also refused by the input service
    /// itself; commands run through the helper daemon, which knows nothing
    /// of a stop. The stop's saved marker counts too, so a stopped helper
    /// whose input service is down still refuses.
    pub async fn require_going(&self) -> Result<()> {
        if self.actor == Actor::Viewer {
            return Ok(());
        }

        let marked = self.stop_marked();
        let hello = self.input_transport().call(&json!({"op": "hello"})).await;
        if marked || hello.is_ok_and(|hello| hello["held"] == true) {
            return Err(Error::stopped());
        }

        Ok(())
    }

    /// Whether a Stop is saved on either side: the input service's own
    /// marker, or the person's note.
    pub(crate) fn stop_marked(&self) -> bool {
        self.config.input_socket.with_extension("stopped").exists() || self.controller.stop_noted()
    }
}
