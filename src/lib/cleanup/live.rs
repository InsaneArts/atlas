//! A cleanup's hands in the real helper: its app list, Atlas's own quit
//! script, the environment worker, and the copy root, under the control
//! the cleanup holds.
use super::run::{Hands, running_from, running_from_installs};
use crate::{
    agent,
    app_selection::Process,
    client::{Actor, Client},
    control::{Ownership, Token},
    environment::{self, PlanState, Undone},
    error::{Error, Result},
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) struct Live<'a> {
    client: &'a Client,
    /// The control a cleanup that acts holds; none for a preview, which
    /// only reads.
    ownership: Option<&'a Ownership>,
    copies: PathBuf,
}

impl<'a> Live<'a> {
    /// Hands that only read, for a preview.
    pub fn reading(client: &'a Client) -> Self {
        Self {
            client,
            ownership: None,
            copies: PathBuf::from(crate::handoff::ROOT),
        }
    }

    /// Hands that act under `ownership`.
    pub fn acting(client: &'a Client, ownership: &'a Ownership) -> Self {
        Self {
            ownership: Some(ownership),
            ..Self::reading(client)
        }
    }

    fn held(&self) -> Result<&Ownership> {
        self.ownership
            .ok_or_else(|| Error::new("state", "A cleanup preview changes nothing"))
    }
}

impl Hands for Live<'_> {
    fn copies(&self) -> &Path {
        &self.copies
    }

    async fn apps(&self) -> Result<Value> {
        self.client.apps().await
    }

    async fn plan_states(&self, ids: &[Token]) -> Result<Vec<PlanState>> {
        environment::plan_states(self.client, ids).await
    }

    async fn installs(&self, ids: &[Token]) -> Result<Value> {
        environment::installs(self.client, ids).await
    }

    async fn quit(
        &self,
        processes: &[Process],
        folders: &[String],
        installs: &[Token],
    ) -> Result<()> {
        let ownership = self.held()?;
        let started = Instant::now();
        // Done once nothing in a fresh list runs from the task's bundles,
        // whatever it is called.
        let left = |apps: &Value, _: &[u64]| {
            running_from(apps, folders)
                .into_iter()
                .map(|process| match process.name.as_str() {
                    "" => format!("process {}", process.pid),
                    _ => process.name,
                })
                .collect()
        };
        agent::quit(self.client, ownership, processes, left, agent::QUIT_WAIT).await?;

        // Processes that are not apps, such as helpers running from an
        // install, are seen only by the helper's own process list.
        loop {
            let answer = environment::installs(self.client, installs).await?;
            let still = running_from_installs(&answer);
            if still.is_empty() {
                return Ok(());
            }
            if started.elapsed() >= agent::QUIT_WAIT {
                return Err(Error::new(
                    "wait-failed",
                    format!(
                        "{} running from an installed app did not quit within {} seconds",
                        crate::ui::text::count(still.len(), "process"),
                        agent::QUIT_WAIT.as_secs()
                    ),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    async fn undo_plan(&self, plan: &Token) -> Result<Undone> {
        environment::undo_plan(self.client, self.held()?, plan).await
    }

    async fn remove_install(&self, install: &Token) -> Result<Value> {
        environment::remove_app(self.client, self.held()?, install).await
    }

    fn stopped(&self) -> bool {
        // A Stop is for agents: the person's own cleanup goes on.
        self.client.actor == Actor::Agent && self.client.stop_marked()
    }

    fn interrupted(&self) -> bool {
        self.client.controller.interrupt().asked().is_some()
    }
}
