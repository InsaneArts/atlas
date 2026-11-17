//! A cleanup's hands in the real helper: its app list, Atlas's own quit
//! script, the environment worker, and the copy root, under the control
//! the cleanup holds.
use super::{
    plan::{Collection, Job},
    run::{Hands, JobEnd, Quitted},
    targets::{still_in_installs, still_running},
};
use crate::{
    agent::{self, Quitting},
    app_selection::Process,
    client::{Actor, AppList, Client, Verified},
    collect::{self, Intact},
    environment::{self, InstallState, PlanState, RemovalReply, Undone},
    handoff::{Copies, CopyRemoval},
    host_session::Key,
};
use atlas_control::Ownership;
use atlas_core::{
    error::{Code, Error, Result},
    files::Token,
    protocol::{JobId, Shown},
};
use std::time::{Duration, Instant};

pub(super) struct Live<'a> {
    client: &'a Client,
    /// The control a cleanup that acts holds; none for a preview, which
    /// only reads.
    ownership: Option<&'a Ownership>,
    copies: Copies,
    /// A VM's proof, taken once: its apps are read and quit in its session
    /// and generation alone.
    verified: Option<Verified>,
    /// Whether an earlier outcome was unknown when control was taken.
    unknown: bool,
}

impl<'a> Live<'a> {
    /// Hands that only read, for a preview. A VM guest's copies are known
    /// by its inventory, read once here: each one's folder, its identity
    /// and what runs from it. One that cannot be read leaves them unlisted:
    /// none is then proven to be the session's, and each counts as there.
    ///
    /// # Errors
    ///
    /// Refuses a desktop whose copies this build does not know, or cannot
    /// reach.
    pub(super) async fn reading(client: &'a Client) -> Result<Self> {
        client.reachable()?;
        let mut copies = Copies::of(&client.desktop()).ok_or_else(|| client.unreachable())?;
        let mut verified = None;
        if copies.in_a_guest() {
            verified = Some(client.verify_vm().await?);
            if let Ok(listed) = crate::vm::copies::inventory(client).await {
                copies = copies.listed(listed, client.output_dir()?);
            }
        }
        Ok(Self {
            client,
            ownership: None,
            copies,
            verified,
            unknown: false,
        })
    }

    /// Hands that act under `ownership`.
    ///
    /// # Errors
    ///
    /// As [`Live::reading`].
    pub(super) async fn acting(
        client: &'a Client,
        ownership: &'a Ownership,
        unknown: bool,
    ) -> Result<Self> {
        Ok(Self {
            ownership: Some(ownership),
            unknown,
            ..Self::reading(client).await?
        })
    }

    fn held(&self) -> Result<&Ownership> {
        self.ownership
            .ok_or_else(|| Error::new(Code::State, "A cleanup preview changes nothing"))
    }
}

impl Hands for Live<'_> {
    fn copies(&self) -> &Copies {
        &self.copies
    }

    async fn apps(&self) -> Result<AppList> {
        agent::apps_in(self.client, self.verified.as_ref()).await
    }

    async fn remove_copy(&self, copy: &Token, session: &Key, running: &AppList) -> CopyRemoval {
        self.copies
            .remove_with(
                self.client,
                copy,
                (session, self.verified.as_ref()),
                running.rows(),
            )
            .await
    }

    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>> {
        environment::plan_states(self.client, ids).await
    }

    async fn installs(&self, ids: &[Token]) -> Result<Vec<InstallState>> {
        environment::install_states(self.client, ids).await
    }

    async fn quit(
        &self,
        processes: &[Process],
        folders: &[String],
        installs: &[Token],
    ) -> Result<Quitted> {
        let ownership = self.held()?;
        let started = Instant::now();
        // Done once nothing in a fresh list runs from the task's bundles,
        // whatever it is called.
        let left = |apps: &AppList, _: &[u64]| {
            still_running(apps, folders)
                .into_iter()
                .map(|app| match app.name() {
                    Some(name) if !name.is_empty() => name.to_owned(),
                    _ => format!("process {}", app.pid()),
                })
                .collect()
        };
        let quitting = agent::quit_until(
            self.client,
            ownership,
            (processes, self.verified.as_ref()),
            left,
            agent::QUIT_WAIT,
        )
        .await?;
        if let Quitting::TimedOut { still } = quitting {
            return Ok(Quitted::TimedOut(format!(
                "{} did not quit within {} seconds",
                still.join(", "),
                agent::QUIT_WAIT.as_secs()
            )));
        }

        // Processes that are not apps, such as helpers running from an
        // install, are seen only by the helper's own process list.
        loop {
            let answer = environment::install_states(self.client, installs).await?;
            let still = still_in_installs(&answer);
            if still == 0 {
                return Ok(Quitted::Gone);
            }
            if started.elapsed() >= agent::QUIT_WAIT {
                return Ok(Quitted::TimedOut(format!(
                    "{} running from an installed app did not quit within {} seconds",
                    atlas_core::name::count(still, "process"),
                    agent::QUIT_WAIT.as_secs()
                )));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn undo_plan(&self, plan: &Token) -> Result<Undone> {
        environment::undo_plan(self.client, self.held()?, plan).await
    }

    async fn remove_install(&self, install: &Token) -> Result<RemovalReply> {
        environment::remove_app(self.client, self.held()?, install)
            .await
            .map(RemovalReply::read)
    }

    fn intact(&self, collection: &Collection) -> Intact {
        match self.client.output_dir() {
            Ok(output) => collect::intact(output, Some(&collection.task), &collection.receipt),
            Err(_) => Intact {
                unknown: true,
                ..Intact::default()
            },
        }
    }

    async fn job(&self, id: &JobId) -> Result<Shown> {
        crate::jobs::show(self.client, id).await
    }

    async fn jobs_of(&self, session: &Key, task: Option<&Key>) -> Result<Vec<Job>> {
        Ok(crate::jobs::of_session(self.client, session, task)
            .await?
            .into_iter()
            .filter_map(|job| {
                Some(Job {
                    task: job.task?,
                    id: job.id,
                    name: job.name,
                })
            })
            .collect())
    }

    async fn stop_job(&self, task: &Key, id: &JobId) -> Result<JobEnd> {
        self.held()?;
        let reply = crate::jobs::stop_for_cleanup(self.client, task, id).await?;
        Ok(JobEnd::read(&reply))
    }

    fn unknown(&self) -> bool {
        self.unknown
    }

    fn stopped(&self) -> bool {
        // A Stop is for agents: the person's own cleanup goes on.
        self.client.actor == Actor::Agent && self.client.stop_marked()
    }

    fn interrupted(&self) -> bool {
        self.client.controller.interrupt().asked().is_some()
    }
}
