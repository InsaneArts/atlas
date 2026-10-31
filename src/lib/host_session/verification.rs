//! Host identity verified from the daemon socket's kernel-reported caller.
use super::{ANCESTORS, Facts, Host, HostId, HostSession, Instance, SOURCES, ancestry};
use crate::{process, transport::Transport};
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
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedHost {
    host: Host,
    id: HostId,
    process: Instance,
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
            })
        })
        .collect()
}

impl HostSession {
    /// Resolve locally, asking the configured daemon only when process reads
    /// cannot establish a host. Failure leaves the ordinary unknown label.
    pub async fn from_env_verified(transport: &Transport) -> Option<Self> {
        let local = Self::from_env();
        let claims = from_env();
        let request = json!({"op": "host-session", "claims": claims});
        Self::verify_with(local, &claims, transport.call(&request)).await
    }

    async fn verify_with(
        local: Option<Self>,
        claims: &[HostClaim],
        reply: impl std::future::Future<Output = crate::error::Result<Value>>,
    ) -> Option<Self> {
        if local
            .as_ref()
            .is_some_and(|session| session.process.is_some() || session.host == Host::Named)
        {
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
        if verified.process.pid <= 0
            || verified.process.start[0] == 0
            || verified.process.start[1] >= 1_000_000
            || !claims
                .iter()
                .any(|claim| claim.host == verified.host && claim.id == verified.id)
        {
            return local;
        }
        let mut session = Self::new(verified.host, verified.id.as_str())?;
        session.process = Some(verified.process);
        session.verified = true;
        session.began = Instance::current().map(|process| process.started_ms());
        // The helper account may verify another account's identity through
        // sysctl, but only the controller account may read its working directory.
        Some(session.with_folder(crate::process::cwd, None))
    }
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
    Some(VerifiedHost {
        host: session.host,
        id: session.id,
        process,
    })
}

#[cfg(test)]
mod tests;
