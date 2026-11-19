//! Which of this account's loopback endpoints a VM's guest mirrors: the TCP
//! listeners a listing finds, folded to the families and ports the station
//! reaches them by on loopback, and kept each in its incarnation while it
//! stays listed, with the listeners that take it. Atlas's own listeners and
//! macOS's own are never mirrored, and at most [`ENDPOINTS`] are, chosen the
//! same way every time.
use atlas_core::{
    app_bundle::{
        VM_IDENTIFIER,
        signature::{APP_IDENTIFIER, requirement_for},
    },
    process::Instance,
};
use atlas_native::native::{
    listeners::{self, Listener, Listing},
    process_token::Token,
};
use atlas_session::mirror::{ENDPOINTS, Endpoint, Family};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    sync::{Mutex, MutexGuard, PoisonError},
};

/// Where macOS keeps its own programs.
const MACOS_OWN: [&str; 3] = ["/System", "/usr/libexec", "/usr/sbin"];

/// The identifiers Atlas's own programs are signed as.
const ATLAS_OWN: [&str; 2] = [APP_IDENTIFIER, VM_IDENTIFIER];

/// This account's listeners, which are never mirrored, and who holds a
/// connection made to one, given apart so tests stand in.
pub(crate) trait Listeners: Send + Sync {
    /// The listeners now.
    fn listing(&self) -> io::Result<Listing>;

    /// Whether `listener` is never mirrored: Atlas's own, macOS's own, or
    /// one whose process can no longer be told.
    fn excluded(&self, listener: &Listener) -> bool;

    /// The process of this account that holds the other end of the
    /// connection from `local` to `peer` now; none while nobody does, as
    /// while it waits in its listener's queue, or when that cannot be told.
    fn held_by(&self, local: SocketAddr, peer: SocketAddr) -> io::Result<Option<Instance>>;
}

/// This account's listeners, as the kernel lists them, and what was judged
/// of each one's process.
#[derive(Default)]
pub(crate) struct Account {
    /// Whether each process is never mirrored, by its audit token, with its
    /// PID: a token names one run of one program, so a process that runs
    /// another since has another token and is judged again.
    judged: Mutex<HashMap<[u32; 8], (i32, bool)>>,
}

impl Account {
    fn judged(&self) -> MutexGuard<'_, HashMap<[u32; 8], (i32, bool)>> {
        self.judged.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Listeners for Account {
    fn listing(&self) -> io::Result<Listing> {
        let listing = listeners::of_account(nix::unistd::geteuid().as_raw())?;
        // Only the processes listed now are kept judged.
        let owners: HashSet<i32> = listing
            .listeners
            .iter()
            .map(|listener| listener.pid)
            .collect();
        self.judged().retain(|_, (pid, _)| owners.contains(pid));
        Ok(listing)
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
        if let Some((_, excluded)) = self.judged().get(&token.words()) {
            return *excluded;
        }
        // What cannot be told for certain is excluded, and judged again.
        let Some(excluded) = judged(&token) else {
            return true;
        };
        let mut judged = self.judged();
        judged.retain(|_, (pid, _)| *pid != listener.pid);
        judged.insert(token.words(), (listener.pid, excluded));
        excluded
    }

    fn held_by(&self, local: SocketAddr, peer: SocketAddr) -> io::Result<Option<Instance>> {
        listeners::held_by(local, peer)
    }
}

/// Whether the process `token` names is never mirrored; none when what it
/// runs cannot be told for certain.
fn judged(token: &Token) -> Option<bool> {
    let program = token.program().ok()?;
    if macos_own(&program) {
        return Some(true);
    }
    let atlas = ATLAS_OWN.iter().any(|identifier| {
        atlas_native::peer::process_satisfies(token.words(), &requirement_for(identifier))
    });
    // A token whose process ended or ran another program since satisfies
    // nothing, which tells nothing: what it runs is read again after.
    if atlas {
        return Some(true);
    }
    (token.program().ok().as_deref() == Some(program.as_path())).then_some(false)
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
    /// Taken by listeners mirrored and by no excluded one, each with the
    /// listeners that take it.
    pub(crate) mirrored: BTreeMap<(Family, u16), Vec<Listener>>,
    /// Taken by a listener `excluded` leaves out, whatever else takes them:
    /// a connection there could reach it.
    pub(crate) excluded: BTreeSet<(Family, u16)>,
    pub(crate) partial: bool,
}

/// The endpoints `listing`'s listeners take, but for any one a listener
/// `excluded` leaves out also takes.
pub(crate) fn endpoints(listing: &Listing, excluded: impl Fn(&Listener) -> bool) -> Endpoints {
    let mut taken: BTreeMap<(Family, u16), Vec<Listener>> = BTreeMap::new();
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
            for place in places {
                taken.entry(place).or_default().push(*listener);
            }
        }
    }
    taken.retain(|place, _| !left_out.contains(place));
    Endpoints {
        mirrored: taken,
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

/// The endpoints mirrored, each in its incarnation with the listeners that
/// take it: one listed again after it left the list, or taken by other
/// listeners than before, is a new incarnation, so no connection to the one
/// before is ever taken for a connection to it.
#[derive(Debug, Default)]
pub(crate) struct Incarnations {
    listed: BTreeMap<(Family, u16), Place>,
    last: u64,
}

/// One endpoint listed: its incarnation, and the listeners last found
/// taking it.
#[derive(Debug)]
struct Place {
    incarnation: u64,
    owners: Vec<Listener>,
}

/// Who `owners` are, whatever order a listing found them in: each one's
/// process, by its PID and start, and its socket.
fn identity(owners: &[Listener]) -> BTreeSet<(i32, [u64; 2], u64)> {
    owners
        .iter()
        .map(|owner| (owner.pid, owner.start, owner.socket))
        .collect()
}

/// The endpoints a listing chose, and those it left out past the bound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Chosen {
    /// At most [`ENDPOINTS`], lowest port first and IPv4 before IPv6.
    pub(crate) endpoints: Vec<Endpoint>,
    /// The rest, in the same order.
    pub(crate) omitted: Vec<(Family, u16)>,
}

impl Incarnations {
    /// The endpoints mirrored now, the lowest [`ENDPOINTS`] by port, IPv4
    /// before IPv6, each in the incarnation it has had since it was last
    /// listed and taken by the very listeners it is taken by now; the rest
    /// left out, each forgotten so that it is a new incarnation once
    /// chosen. A partial listing shows no endpoint left, so every one it
    /// does not name is kept as it was, its listeners those last found,
    /// unless it found it excluded.
    pub(crate) fn list(&mut self, endpoints: &Endpoints) -> Chosen {
        self.listed.retain(|place, _| {
            !endpoints.excluded.contains(place)
                && (endpoints.partial || endpoints.mirrored.contains_key(place))
        });
        for (place, owners) in &endpoints.mirrored {
            let same = self
                .listed
                .get(place)
                .is_some_and(|listed| identity(&listed.owners) == identity(owners));
            if !same {
                self.last += 1;
                let taken = Place {
                    incarnation: self.last,
                    owners: owners.clone(),
                };
                self.listed.insert(*place, taken);
            }
        }
        let mut places: Vec<(Family, u16)> = self.listed.keys().copied().collect();
        places.sort_by_key(|&(family, port)| (port, family));
        let omitted = places.split_off(places.len().min(ENDPOINTS));
        for place in &omitted {
            self.listed.remove(place);
        }
        let endpoints = places
            .into_iter()
            .filter_map(|place| {
                let listed = self.listed.get(&place)?;
                Some(Endpoint {
                    family: place.0,
                    port: place.1,
                    incarnation: listed.incarnation,
                })
            })
            .collect();
        Chosen { endpoints, omitted }
    }

    /// The listeners that take `endpoint`, while it is listed in that very
    /// incarnation.
    pub(crate) fn owners(&self, endpoint: &Endpoint) -> Option<&[Listener]> {
        self.listed
            .get(&endpoint.place())
            .filter(|listed| listed.incarnation == endpoint.incarnation)
            .map(|listed| listed.owners.as_slice())
    }
}

#[cfg(test)]
#[path = "mirrors_tests.rs"]
mod tests;
