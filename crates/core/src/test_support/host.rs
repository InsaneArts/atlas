//! A host resolved from a claim whose process can't be read, for the tests of
//! host verification and of the records built on it.
use crate::{
    host::{
        Host, HostId, HostSession,
        verification::{self, HostClaim, VerifiedHost},
    },
    process::{self, Instance},
    test_support::UID,
};

pub fn claim() -> Vec<HostClaim> {
    vec![HostClaim {
        host: Host::Codex,
        id: HostId::parse("thread").unwrap(),
        hint: None,
        previous: None,
    }]
}

/// Verify `claims` against the socket peer `expected`, reading processes
/// through `describe`.
pub fn peer_resolved(
    claims: &[HostClaim],
    expected: Option<Instance>,
    uid: u32,
    describe: impl Fn(i32) -> crate::error::Result<Option<process::Info>>,
) -> Option<VerifiedHost> {
    verification::resolve_peer(claims, expected, uid, describe)
}

pub fn resolve_peer(
    claims: &[HostClaim],
    pid: Option<i32>,
    uid: u32,
    describe: impl Fn(i32) -> Option<process::Info>,
) -> Option<VerifiedHost> {
    peer_resolved(
        claims,
        pid.map(|pid| Instance::new(pid, [100, 7])),
        uid,
        |pid| Ok(describe(pid)),
    )
}

pub fn process(pid: i32) -> Option<process::Info> {
    let (ppid, uid, name) = match pid {
        90 => (80, UID, "atlas"),
        80 => (70, UID, "codex"),
        // A different account above the host must not end the daemon's walk.
        70 => (1, 0, "login"),
        1 => (0, 0, "launchd"),
        _ => return None,
    };
    Some(process::Info {
        instance: Instance::new(pid, [100, 7]),
        ppid,
        pgid: pid,
        uid,
        ruid: uid,
        svuid: uid,
        zombie: false,
        stopped: false,
        name: name.into(),
    })
}

pub fn denied() -> HostSession {
    HostSession::resolve_for(
        |name| claim().iter().find_map(|claim| claim.env(name)),
        Vec::new,
        |_| None,
        UID,
    )
    .unwrap()
}
