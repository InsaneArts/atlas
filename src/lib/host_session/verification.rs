//! Host identity verified from the daemon socket's kernel-reported caller.
use super::{
    ANCESTORS, Facts, Host, HostId, HostSession, Instance, Key, SOURCES, ancestry, store::Store,
};
use crate::{client::Client, process};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// A conversation label and optional host PID hint, never process proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostClaim {
    host: Host,
    id: HostId,
    hint: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous: Option<Instance>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedHost {
    host: Host,
    id: HostId,
    process: Instance,
    #[serde(default)]
    previous: Option<Instance>,
}

impl HostClaim {
    fn env(&self, name: &str) -> Option<String> {
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

pub(crate) fn claims(value: &Value) -> Option<Vec<HostClaim>> {
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

fn from_env() -> Vec<HostClaim> {
    SOURCES
        .iter()
        .filter_map(|(host, names)| {
            let id = names
                .iter()
                .find_map(|name| std::env::var(name).ok().as_deref().and_then(HostId::parse))?;
            let hint = host
                .pid_hint()
                .and_then(|name| std::env::var(name).ok()?.parse::<i32>().ok())
                .filter(|pid| *pid > 0);
            Some(HostClaim {
                host: *host,
                id,
                hint,
                previous: None,
            })
        })
        .collect()
}

impl HostSession {
    /// Resolve locally, asking the daemon when ancestry or the previous
    /// host's exit needs proof. Failure preserves the local metadata.
    pub async fn from_env_verified(client: &Client) -> Option<Self> {
        let local = Self::from_env();
        let mut claims = from_env();
        if let Ok(output) = client.output_dir() {
            let store = Store::at(output);
            for claim in &mut claims {
                claim.previous = store
                    .read(&Key::of(claim.host, claim.id.as_str()))
                    .and_then(|record| record.host_process);
            }
        }
        let request = json!({"op": "host-session", "claims": claims});
        Self::verify_with(local, &claims, client.transport.call(&request)).await
    }

    async fn verify_with(
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

pub(crate) fn verify(claims: &[HostClaim], peer: Option<Instance>, uid: u32) -> Value {
    json!({"ok": true, "hostSession": resolve_peer(claims, peer, uid, process::info)})
}

fn resolve_peer(
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
    let facts = |pid| {
        let info = describe(pid).ok()??;
        (!info.zombie).then_some(Facts {
            parent: info.ppid,
            uid: info.uid,
            command: info.name,
            start: info.instance.start,
        })
    };
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
mod tests;
