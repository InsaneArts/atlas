//! A cleanup's hands in the real helper: its app list, Atlas's own quit
//! script, the environment worker, and the copy root, under the control
//! the cleanup holds.
use super::run::{Hands, running_from};
use crate::{
    agent,
    app_selection::Process,
    client::{Actor, Client},
    control::{Ownership, Token},
    environment::{self, PlanState, Undone},
    error::{Error, Result},
};
use serde_json::Value;
use std::path::{Path, PathBuf};

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

    async fn quit(&self, processes: &[Process], folders: &[String]) -> Result<()> {
        let ownership = self.held()?;
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
        agent::quit(self.client, ownership, processes, left, agent::QUIT_WAIT)
            .await
            .map(|_| ())
    }

    async fn undo_plan(&self, plan: &Token) -> Result<Undone> {
        environment::undo_plan(self.client, self.held()?, plan).await
    }

    fn stopped(&self) -> bool {
        // A Stop is for agents: the person's own cleanup goes on.
        self.client.actor == Actor::Agent && self.client.stop_marked()
    }

    fn interrupted(&self) -> bool {
        self.client.controller.interrupt().asked().is_some()
    }
}
