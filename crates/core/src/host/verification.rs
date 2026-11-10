//! Host identity verified from the daemon socket's kernel-reported caller.
use super::{ANCESTORS, Host, HostId, HostSession, Key, SOURCES, ancestry};
use crate::process::{self, Instance};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// A conversation label and optional host PID hint, never process proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostClaim {
    pub host: Host,
    pub id: HostId,
    pub hint: Option<i32>,
    /// The instance this conversation's record last named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Instance>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedHost {
    host: Host,
    id: HostId,
    process: Instance,
    #[serde(default)]
    pub previous: Option<Instance>,
}

impl HostClaim {
    /// The key of the session this claims.
    pub fn key(&self) -> Key {
        Key::of(self.host, self.id.as_str())
    }

    pub(crate) fn env(&self, name: &str) -> Option<String> {
        if SOURCES
            .iter()
            .any(|(host, names)| *host == self.host && names.contains(&name))
        {
            return Some(self.id.as_str().to_owned());
        }
        let hint = self.host.pid_hint()?;
        (name == hint)
            .then(|| self.hint.map(|pid| pid.to_string()))
            .flatten()
    }
}

pub fn claims(value: &Value) -> Option<Vec<HostClaim>> {
    let claims: Vec<HostClaim> = serde_json::from_value(value.clone()).ok()?;
    if claims.is_empty() || claims.len() > SOURCES.len() {
        return None;
    }
    for (index, claim) in claims.iter().enumerate() {
        if claim.host == Host::Named
            || claim.hint.is_some_and(|pid| pid <= 0)
            || claim.previous.is_some_and(|process| !valid(process))
            || claims[..index]
                .iter()
                .any(|before| before.host == claim.host)
        {
            return None;
        }
    }
    Some(claims)
}

/// What this process's environment claims, one claim a host.
pub fn from_env() -> Vec<HostClaim> {
    let env = |name: &str| std::env::var(name).ok();
    Host::sessions(&env)
        .into_iter()
        .map(|(host, id)| HostClaim {
            host,
            id,
            hint: host.hint(&env),
            previous: None,
        })
        .collect()
}

impl HostSession {
    pub async fn verify_with(
        local: Option<Self>,
        claims: &[HostClaim],
        reply: impl std::future::Future<Output = crate::error::Result<Value>>,
    ) -> Option<Self> {
        if local.as_ref().is_some_and(|session| {
            session.host == Host::Named
                || (session.process.is_some()
                    && !claims.iter().any(|claim| {
                        claim.host == session.host
                            && claim.id == session.id
                            && claim.previous.is_some()
                            && claim.previous != session.process
                    }))
        }) {
            return local;
        }
        if claims.is_empty() {
            return local;
        }
        let reply = tokio::time::timeout(Duration::from_millis(250), reply).await;
        match reply {
            Ok(Ok(reply)) if reply["ok"] == true => {
                Self::accept_verified(local, claims, &reply["hostSession"])
            }
            _ => local,
        }
    }

    fn accept_verified(local: Option<Self>, claims: &[HostClaim], reply: &Value) -> Option<Self> {
        let Ok(verified) = serde_json::from_value::<VerifiedHost>(reply.clone()) else {
            return local;
        };
        if !valid(verified.process)
            || !claims
                .iter()
                .any(|claim| claim.host == verified.host && claim.id == verified.id)
        {
            return local;
        }
        let mut session = Self::new(verified.host, verified.id.as_str())?;
        session.process = Some(verified.process);
        session.verified = true;
        session.replaces = verified.previous.filter(|previous| {
            claims.iter().any(|claim| {
                claim.host == verified.host
                    && claim.id == verified.id
                    && claim.previous == Some(*previous)
            })
        });
        session.began = Instance::current().map(|process| process.started_ms());
        // The helper account may verify another account's identity through
        // sysctl, but only the controller account may read its working directory.
        Some(session.with_folder(crate::process::cwd, None))
    }
}

fn valid(process: Instance) -> bool {
    process.pid > 0 && process.start[0] > 0 && process.start[1] < 1_000_000
}

pub fn verify(claims: &[HostClaim], peer: Option<Instance>, uid: u32) -> Value {
    json!({"ok": true, "hostSession": resolve_peer(claims, peer, uid, process::info)})
}

pub(crate) fn resolve_peer(
    claims: &[HostClaim],
    expected: Option<Instance>,
    uid: u32,
    describe: impl Fn(i32) -> crate::error::Result<Option<process::Info>>,
) -> Option<VerifiedHost> {
    let expected = expected.filter(|peer| peer.pid > 0)?;
    let peer = describe(expected.pid).ok()??;
    if peer.instance != expected || peer.uid != uid || peer.zombie {
        return None;
    }
    let facts = |pid| describe(pid).ok()?.filter(|info| !info.zombie);
    let session = HostSession::resolve_for(
        |name| claims.iter().find_map(|claim| claim.env(name)),
        || ancestry::ancestors_from(peer.ppid, ANCESTORS, facts),
        facts,
        uid,
    )?;
    let process = session.process?;
    let after = describe(peer.instance.pid).ok()??;
    if after.instance != peer.instance
        || after.uid != uid
        || after.ppid != peer.ppid
        || after.zombie
    {
        return None;
    }
    let previous = claims
        .iter()
        .find(|claim| claim.host == session.host && claim.id == session.id)
        .and_then(|claim| claim.previous)
        .filter(|previous| {
            *previous != process
                && describe(previous.pid).is_ok_and(|info| {
                    info.is_none_or(|info| info.instance != *previous || info.zombie)
                })
        });
    Some(VerifiedHost {
        host: session.host,
        id: session.id,
        process,
        previous,
    })
}

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;
