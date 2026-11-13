//! Deferring service replacement while another session or a job is active.
mod sessions;
pub(crate) use sessions::service_blocker;
#[cfg(test)]
pub(crate) use sessions::service_blocker_with;

use super::Client;
use atlas_control::Ownership;
use atlas_core::{
    desktop::Kind,
    error::{Code, Error, Holder, Result},
    protocol::{JobState, Shown},
};
use std::time::Duration;

/// How long a service change waits for the helper's list of background jobs.
const JOB_CHECK: Duration = Duration::from_secs(2);
/// How long service replacement waits for a pending-state writer.
const MARKER_CHECK: Duration = Duration::from_millis(250);
const NEXT: &str = "Wait for the work to finish, or add --allow-disrupt to permit interrupting it. A controller or an unfinished action can still block the restart.";

fn busy(message: impl Into<String>) -> Error {
    Error::new(Code::Busy, message)
        .held_by(Holder::Agent)
        .with("next", serde_json::json!(NEXT))
}

impl Client {
    /// Take controller exclusion for service replacement, checking active
    /// work before waiting and again under the lock. Permission to interrupt
    /// sessions never bypasses exclusion or settles pending changes.
    ///
    /// # Errors
    ///
    /// Without explicit disruption permission, refuses active or unprovable session and job
    /// state. Controller exclusion and pending-work rules apply even when disruption is allowed.
    pub async fn service_change(&self, recovery: bool) -> Result<Ownership> {
        self.require_service_idle_for(recovery).await?;
        let ownership = self
            .controller
            .acquire_for_service_change(recovery, MARKER_CHECK)?;
        self.require_service_idle_for(recovery).await?;
        Ok(ownership)
    }

    /// Refuse service replacement while another session or background job
    /// is active, unless the person permits disruption. Callers already
    /// holding controller exclusion can check without taking it again.
    ///
    /// # Errors
    ///
    /// Without explicit disruption permission, refuses a service change while another session
    /// or job may still be active, including observations too incomplete to prove idleness.
    pub async fn require_service_idle(&self) -> Result<()> {
        self.require_service_idle_for(false).await
    }

    pub(super) async fn require_service_idle_for(&self, recovery: bool) -> Result<()> {
        if self.allow_disrupt {
            return Ok(());
        }

        let client = self.clone();
        let blocker = tokio::task::spawn_blocking(move || {
            client.output_dir()?;
            Ok::<_, Error>(service_blocker(&client, recovery, super::timestamp()))
        })
        .await
        .map_err(|cause| {
            busy("Atlas could not check which sessions are active.").with_source(cause)
        })??;
        if let Some(message) = blocker {
            return Err(busy(message));
        }

        let first_setup = match self.desktop().kind() {
            Kind::User(login) => crate::user::service::first_setup_in(self, login),
        };
        if first_setup {
            return Ok(());
        }

        self.require_jobs_idle(crate::jobs::shown_all(self)).await
    }

    async fn require_jobs_idle(
        &self,
        read: impl std::future::Future<Output = Result<Vec<Shown>>>,
    ) -> Result<()> {
        let jobs = match tokio::time::timeout(JOB_CHECK, read).await {
            Ok(Ok(jobs)) => jobs,
            Ok(Err(error)) => {
                return Err(busy(format!(
                    "Atlas could not check background jobs: {}",
                    error.error
                ))
                .with_source(error));
            }
            Err(cause) => {
                return Err(
                    busy("The helper did not report its background jobs in time.")
                        .with_source(cause),
                );
            }
        };
        if let Some(message) = job_blocker(&jobs) {
            return Err(busy(message));
        }
        Ok(())
    }
}

/// The first job that has not ended, including a record Atlas cannot read.
fn job_blocker(jobs: &[Shown]) -> Option<String> {
    jobs.iter()
        .find(|job| job.state() != Some(JobState::Ended))
        .map(|job| match job.job() {
            Some(job) => format!("Background job {} is active in the helper.", job.name),
            None => {
                "A background job may still be running, but Atlas cannot read its record.".into()
            }
        })
}

#[cfg(test)]
#[path = "service_change_tests.rs"]
mod tests;
