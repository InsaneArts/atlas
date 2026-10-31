//! Deferring service replacement while another session or a job is active.
use super::Client;
use crate::{
    control::Ownership,
    daemon::protocol::{JobState, Shown},
    error::{Code, Error, Holder, Result},
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
        .with("next", NEXT)
}

impl Client {
    /// Take controller exclusion for service replacement, checking active
    /// work before waiting and again under the lock. Permission to interrupt
    /// sessions never bypasses exclusion or settles pending changes.
    pub async fn service_change(&self, recovery: bool) -> Result<Ownership> {
        self.require_service_idle().await?;
        let ownership = self
            .controller
            .acquire_for_service_change(recovery, MARKER_CHECK)?;
        self.require_service_idle().await?;
        Ok(ownership)
    }

    /// Refuse service replacement while another session or background job
    /// is active, unless the person permits disruption. Callers already
    /// holding controller exclusion can check without taking it again.
    pub async fn require_service_idle(&self) -> Result<()> {
        if self.allow_disrupt {
            return Ok(());
        }

        let client = self.clone();
        let blocker = tokio::task::spawn_blocking(move || {
            client.output_dir()?;
            Ok::<_, Error>(crate::overview::service_blocker(
                &client,
                super::timestamp(),
            ))
        })
        .await
        .map_err(|_| busy("Atlas could not check which sessions are active."))??;
        if let Some(message) = blocker {
            return Err(busy(message));
        }

        // The daemon creates its directory before accepting jobs and keeps
        // it across shutdowns. Only an absent directory establishes a first
        // setup; a missing socket can leave background jobs running.
        if self.transport.path.parent().is_some_and(|directory| {
            std::fs::symlink_metadata(directory)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        }) {
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
            // A daemon from before background jobs cannot have any.
            Ok(Err(error)) if error.code == Code::Daemon && error.error == crate::jobs::OLDER => {
                return Ok(());
            }
            Ok(Err(error)) => {
                return Err(busy(format!(
                    "Atlas could not check background jobs: {}",
                    error.error
                )));
            }
            Err(_) => {
                return Err(busy(
                    "The helper did not report its background jobs in time.",
                ));
            }
        };
        if let Some(message) = job_blocker(&jobs) {
            return Err(busy(message));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

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
