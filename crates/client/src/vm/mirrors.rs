//! Which of this account's loopback endpoints a VM's guest mirrors: the TCP
//! listeners a listing finds, folded to the families and ports the station
//! reaches them by on loopback, and kept each in its incarnation while it
//! stays listed. Atlas's own listeners and macOS's own are never mirrored.
use atlas_core::app_bundle::{
    VM_IDENTIFIER,
    signature::{APP_IDENTIFIER, requirement_for},
};
use atlas_native::native::{
    listeners::{self, Listener, Listing},
    process_token::Token,
};
use atlas_session::mirror::{Endpoint, Family};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
};

/// Where macOS keeps its own programs.
const MACOS_OWN: [&str; 3] = ["/System", "/usr/libexec", "/usr/sbin"];

/// The identifiers Atlas's own programs are signed as.
const ATLAS_OWN: [&str; 2] = [APP_IDENTIFIER, VM_IDENTIFIER];

/// This account's listeners, and which are never mirrored, given apart so
/// tests stand in.
pub(crate) trait Listeners {
    /// The listeners now.
    fn listing(&self) -> io::Result<Listing>;

    /// Whether `listener` is never mirrored: Atlas's own, macOS's own, or
    /// one whose process can no longer be told.
    fn excluded(&self, listener: &Listener) -> bool;
}

/// This account's listeners, as the kernel lists them.
pub(crate) struct Account;

impl Listeners for Account {
    fn listing(&self) -> io::Result<Listing> {
        listeners::of_account(nix::unistd::geteuid().as_raw())
    }

    fn excluded(&self, listener: &Listener) -> bool {
        // The token names the listed process only while that process still
        // runs: its PID could name no other one started since.
        let Some(token) = Token::of(listener.pid)
            .ok()
            .filter(|_| listener.owner().running())
        else {
            return true;
        };
        let Ok(program) = token.program() else {
            return true;
        };
        if macos_own(&program) {
            return true;
        }
        let atlas = ATLAS_OWN.iter().any(|identifier| {
            atlas_native::peer::process_satisfies(token.words(), &requirement_for(identifier))
        });
        // A token whose process ended or ran another program since satisfies
        // nothing, which tells nothing: what it runs is read again after.
        atlas || token.program().ok().as_deref() != Some(program.as_path())
    }
}

/// Whether `program` is one of macOS's own.
fn macos_own(program: &Path) -> bool {
    MACOS_OWN.iter().any(|root| program.starts_with(root))
}

/// What one listing gives the mirrors: the families and ports on loopback
/// its listeners take, those that any excluded listener takes, and whether
/// it may have missed some.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Endpoints {
    /// Taken by listeners mirrored and by no excluded one.
    pub(crate) mirrored: BTreeSet<(Family, u16)>,
    /// Taken by a listener `excluded` leaves out, whatever else takes them:
    /// a connection there could reach it.
    pub(crate) excluded: BTreeSet<(Family, u16)>,
    pub(crate) partial: bool,
}

/// The endpoints `listing`'s listeners take, but for any one a listener
/// `excluded` leaves out also takes.
pub(crate) fn endpoints(listing: &Listing, excluded: impl Fn(&Listener) -> bool) -> Endpoints {
    let mut taken = BTreeSet::new();
    let mut left_out = BTreeSet::new();
    for listener in &listing.listeners {
        let families = families(listener);
        if families.is_empty() {
            continue;
        }
        let places = families.iter().map(|family| (*family, listener.port));
        if excluded(listener) {
            left_out.extend(places);
        } else {
            taken.extend(places);
        }
    }
    Endpoints {
        mirrored: taken.difference(&left_out).copied().collect(),
        excluded: left_out,
        partial: listing.partial,
    }
}

/// The families on loopback the station reaches `listener` by: none for a
/// listener on another address.
fn families(listener: &Listener) -> &'static [Family] {
    let bound = listener.bound;
    if bound == Ipv4Addr::LOCALHOST || bound == Ipv4Addr::UNSPECIFIED {
        &[Family::Ipv4]
    } else if bound == Ipv6Addr::LOCALHOST || (bound == Ipv6Addr::UNSPECIFIED && listener.v6only) {
        &[Family::Ipv6]
    } else if bound == Ipv6Addr::UNSPECIFIED {
        &[Family::Ipv4, Family::Ipv6]
    } else {
        &[]
    }
}

/// The endpoints mirrored, each in its incarnation: one listed again after
/// it left the list is a new incarnation, so no connection to the one
/// before is ever taken for a connection to it.
#[derive(Debug, Default)]
pub(crate) struct Incarnations {
    listed: BTreeMap<(Family, u16), u64>,
    last: u64,
}

impl Incarnations {
    /// The endpoints mirrored now, lowest port first and IPv4 before IPv6,
    /// each in the incarnation it has had since it was last listed. A
    /// partial listing shows no endpoint left, so every one it does not name
    /// is kept as it was, unless it found it excluded.
    pub(crate) fn list(&mut self, endpoints: &Endpoints) -> Vec<Endpoint> {
        self.listed.retain(|place, _| {
            !endpoints.excluded.contains(place)
                && (endpoints.partial || endpoints.mirrored.contains(place))
        });
        let last = &mut self.last;
        for place in &endpoints.mirrored {
            self.listed.entry(*place).or_insert_with(|| {
                *last += 1;
                *last
            });
        }
        let mut endpoints: Vec<Endpoint> = self
            .listed
            .iter()
            .map(|(&(family, port), &incarnation)| Endpoint {
                family,
                port,
                incarnation,
            })
            .collect();
        endpoints.sort_by_key(|endpoint| (endpoint.port, endpoint.family));
        endpoints
    }
}

#[cfg(test)]
#[path = "mirrors_tests.rs"]
mod tests;
