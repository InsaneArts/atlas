use super::*;
use atlas_core::process::Instance;
use atlas_native::native::listeners::Family as Socket;
use std::net::{IpAddr, SocketAddr, TcpListener};

/// A listener of process `pid`, bound to `bound` on `port`.
fn listener(pid: i32, bound: &str, port: u16, v6only: bool) -> Listener {
    let bound: IpAddr = bound.parse().unwrap();
    Listener {
        pid,
        start: [1_700_000_000, u64::try_from(pid).unwrap()],
        family: if bound.is_ipv4() {
            Socket::Ipv4
        } else {
            Socket::Ipv6
        },
        bound,
        port,
        v6only,
        socket: u64::from(port),
    }
}

fn listing(listeners: Vec<Listener>) -> Listing {
    Listing {
        listeners,
        partial: false,
    }
}

fn none(_: &Listener) -> bool {
    false
}

/// A listing that found `mirrored` and nothing excluded.
fn found(mirrored: &BTreeSet<(Family, u16)>, partial: bool) -> Endpoints {
    Endpoints {
        mirrored: mirrored.iter().map(|place| (*place, Vec::new())).collect(),
        excluded: BTreeSet::new(),
        partial,
    }
}

/// A listing given, with the processes never mirrored named.
struct Fixed {
    listing: Listing,
    excluded: Vec<i32>,
}

impl Listeners for Fixed {
    fn listing(&self) -> io::Result<Listing> {
        Ok(self.listing.clone())
    }

    fn excluded(&self, listener: &Listener) -> bool {
        self.excluded.contains(&listener.pid)
    }

    fn held_by(&self, _: SocketAddr, _: SocketAddr) -> io::Result<Option<Instance>> {
        Ok(None)
    }
}

fn mirrored(listeners: &impl Listeners) -> BTreeSet<(Family, u16)> {
    places(&endpoints(&listeners.listing().unwrap(), |listener| {
        listeners.excluded(listener)
    }))
}

/// The places `found` mirrors, whoever takes them.
fn places(found: &Endpoints) -> BTreeSet<(Family, u16)> {
    found.mirrored.keys().copied().collect()
}

#[test]
fn a_listener_opened_by_this_test_is_mirrored_on_its_family_and_port() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();

    let places = mirrored(&Account::default());

    assert!(places.contains(&(Family::Ipv4, port)), "{places:?}");
    assert!(!places.contains(&(Family::Ipv6, port)));
}

#[test]
fn loopback_and_every_address_listeners_give_their_families() {
    let places = places(&endpoints(
        &listing(vec![
            listener(10, "127.0.0.1", 8000, false),
            listener(10, "0.0.0.0", 8001, false),
            listener(10, "::1", 8002, false),
        ]),
        none,
    ));

    assert_eq!(
        places,
        BTreeSet::from([
            (Family::Ipv4, 8000),
            (Family::Ipv4, 8001),
            (Family::Ipv6, 8002),
        ])
    );
}

#[test]
fn a_v6_only_wildcard_listener_gives_v6_and_a_dual_stack_one_both() {
    let places = places(&endpoints(
        &listing(vec![
            listener(10, "::", 8765, true),
            listener(10, "::", 8766, false),
        ]),
        none,
    ));

    assert_eq!(
        places,
        BTreeSet::from([
            (Family::Ipv6, 8765),
            (Family::Ipv4, 8766),
            (Family::Ipv6, 8766),
        ])
    );
}

#[test]
fn listeners_on_other_addresses_are_not_mirrored() {
    let places = places(&endpoints(
        &listing(vec![
            listener(10, "192.168.1.5", 8000, false),
            listener(10, "10.0.0.1", 8001, false),
            listener(10, "127.0.0.2", 8002, false),
            listener(10, "fe80::1", 8003, true),
            listener(10, "::ffff:127.0.0.1", 8004, false),
            listener(10, "2001:db8::1", 8005, false),
        ]),
        none,
    ));

    assert!(places.is_empty(), "{places:?}");
}

#[test]
fn atlas_own_and_macos_own_listeners_are_never_mirrored() {
    let atlas = 20;
    let control_center = 21;
    let server = 22;
    let listeners = Fixed {
        listing: listing(vec![
            listener(atlas, "127.0.0.1", 51_386, false),
            listener(control_center, "0.0.0.0", 5000, false),
            listener(control_center, "::", 7000, true),
            listener(server, "127.0.0.1", 8765, false),
        ]),
        excluded: vec![atlas, control_center],
    };

    assert_eq!(mirrored(&listeners), BTreeSet::from([(Family::Ipv4, 8765)]));
}

#[test]
fn macos_own_programs_are_those_under_its_own_folders() {
    for program in [
        "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter",
        "/usr/libexec/rapportd",
        "/usr/sbin/cupsd",
    ] {
        assert!(macos_own(Path::new(program)), "{program}");
    }
    for program in [
        "/usr/bin/nc",
        "/opt/homebrew/bin/node",
        "/Applications/Atlas.app/Contents/MacOS/atlas",
        "/Systems/server",
        "/usr/libexecutable/server",
    ] {
        assert!(!macos_own(Path::new(program)), "{program}");
    }
}

#[test]
fn a_listener_whose_process_can_no_longer_be_told_is_never_mirrored() {
    let account = Account::default();
    let me = Instance::current().unwrap();
    let mut other_start = listener(me.pid, "127.0.0.1", 8765, false);
    other_start.start = [me.start[0].saturating_sub(1), me.start[1]];
    let mut gone = listener(i32::MAX, "127.0.0.1", 8766, false);
    gone.start = me.start;
    let mut mine = listener(me.pid, "127.0.0.1", 8767, false);
    mine.start = me.start;

    assert!(account.excluded(&other_start));
    assert!(account.excluded(&gone));
    assert!(
        !account.excluded(&mine),
        "this test's own process is neither"
    );
}

#[test]
fn an_endpoint_listed_again_after_it_left_gets_a_new_incarnation() {
    let mut incarnations = Incarnations::default();
    let both = BTreeSet::from([(Family::Ipv4, 8765), (Family::Ipv6, 8766)]);
    let one = BTreeSet::from([(Family::Ipv6, 8766)]);

    let first = incarnations.list(&found(&both, false)).endpoints;
    let kept = incarnations.list(&found(&both, false)).endpoints;
    let left = incarnations.list(&found(&one, false)).endpoints;
    let again = incarnations.list(&found(&both, false)).endpoints;

    assert_eq!(
        kept, first,
        "an endpoint still listed keeps its incarnation"
    );
    assert_eq!(left, vec![first[1]]);
    assert_eq!(again[1], first[1]);
    assert_eq!(again[0].place(), first[0].place());
    assert_ne!(again[0].incarnation, first[0].incarnation);
    assert_ne!(again[0].incarnation, first[1].incarnation);
}

#[test]
fn a_partial_listing_lets_no_endpoint_leave() {
    let mut incarnations = Incarnations::default();
    let first = incarnations
        .list(&found(&BTreeSet::from([(Family::Ipv4, 8765)]), false))
        .endpoints;

    let partial = incarnations
        .list(&found(&BTreeSet::from([(Family::Ipv4, 8766)]), true))
        .endpoints;

    assert_eq!(partial.len(), 2);
    assert_eq!(partial[0], first[0]);
    assert_eq!(partial[1].place(), (Family::Ipv4, 8766));
}

#[test]
fn endpoints_are_listed_lowest_port_first_and_ipv4_before_ipv6() {
    let mut incarnations = Incarnations::default();

    let listed = incarnations
        .list(&found(
            &BTreeSet::from([
                (Family::Ipv6, 3000),
                (Family::Ipv4, 9000),
                (Family::Ipv4, 3000),
            ]),
            false,
        ))
        .endpoints;

    let places: Vec<_> = listed.iter().map(Endpoint::place).collect();
    assert_eq!(
        places,
        vec![
            (Family::Ipv4, 3000),
            (Family::Ipv6, 3000),
            (Family::Ipv4, 9000),
        ]
    );
}

#[test]
fn an_endpoint_an_excluded_listener_shares_is_never_mirrored() {
    let excluded = 20;
    let allowed = 21;
    let listeners = Fixed {
        listing: listing(vec![
            listener(allowed, "0.0.0.0", 5000, false),
            listener(excluded, "127.0.0.1", 5000, false),
            listener(allowed, "::", 7000, false),
            listener(excluded, "0.0.0.0", 7000, false),
            listener(excluded, "::", 7001, false),
            listener(allowed, "127.0.0.1", 7001, false),
            listener(allowed, "::1", 7001, false),
        ]),
        excluded: vec![excluded],
    };

    let found = endpoints(&listeners.listing().unwrap(), |listener| {
        listeners.excluded(listener)
    });

    assert_eq!(places(&found), BTreeSet::from([(Family::Ipv6, 7000)]));
    assert_eq!(
        found.excluded,
        BTreeSet::from([
            (Family::Ipv4, 5000),
            (Family::Ipv4, 7000),
            (Family::Ipv4, 7001),
            (Family::Ipv6, 7001),
        ])
    );
}

#[test]
fn an_endpoint_found_excluded_is_withdrawn_even_from_a_partial_listing() {
    let mut incarnations = Incarnations::default();
    let both = BTreeSet::from([(Family::Ipv4, 8765), (Family::Ipv4, 8766)]);
    incarnations.list(&found(&both, false));

    let listed = incarnations
        .list(&Endpoints {
            mirrored: BTreeMap::new(),
            excluded: BTreeSet::from([(Family::Ipv4, 8765)]),
            partial: true,
        })
        .endpoints;

    let places: Vec<_> = listed.iter().map(Endpoint::place).collect();
    assert_eq!(places, vec![(Family::Ipv4, 8766)]);
}

/// `count` endpoints on IPv4, from port 9000 up, listed in reverse.
fn ports(count: u16) -> BTreeSet<(Family, u16)> {
    (0..count)
        .rev()
        .map(|at| (Family::Ipv4, 9000 + at))
        .collect()
}

#[test]
fn thirty_two_endpoints_are_all_chosen_and_a_thirty_third_is_left_out() {
    let mut incarnations = Incarnations::default();

    let all = incarnations.list(&found(&ports(32), false));
    let mut more = ports(32);
    more.insert((Family::Ipv6, 8999));
    let one_more = Incarnations::default().list(&found(&more, false));

    assert_eq!(all.endpoints.len(), 32);
    assert!(all.omitted.is_empty());
    assert_eq!(one_more.endpoints.len(), 32);
    assert_eq!(one_more.endpoints[0].place(), (Family::Ipv6, 8999));
    assert_eq!(one_more.omitted, vec![(Family::Ipv4, 9031)]);
}

#[test]
fn the_same_endpoints_are_chosen_every_time_lowest_port_first_and_ipv4_before_ipv6() {
    let mut both = BTreeSet::new();
    for at in 0..20 {
        both.insert((Family::Ipv6, 9000 + at));
        both.insert((Family::Ipv4, 9000 + at));
    }

    let chosen = Incarnations::default().list(&found(&both, false));
    let again = Incarnations::default().list(&found(&both, false));

    let places: Vec<_> = chosen.endpoints.iter().map(Endpoint::place).collect();
    assert_eq!(places[..2], [(Family::Ipv4, 9000), (Family::Ipv6, 9000)]);
    assert_eq!(places[31], (Family::Ipv6, 9015));
    assert_eq!(chosen.omitted[0], (Family::Ipv4, 9016));
    assert_eq!(chosen.omitted.len(), 8);
    assert_eq!(chosen, again);
}

#[test]
fn an_omitted_endpoint_takes_the_place_of_one_that_left_in_a_new_incarnation() {
    let mut incarnations = Incarnations::default();
    let first = incarnations.list(&found(&ports(33), false));
    let mut left = ports(33);
    left.remove(&(Family::Ipv4, 9000));

    let after = incarnations.list(&found(&left, false));

    assert_eq!(first.omitted, vec![(Family::Ipv4, 9032)]);
    assert!(after.omitted.is_empty());
    assert_eq!(after.endpoints.len(), 32);
    assert_eq!(
        after.endpoints[..31],
        first.endpoints[1..],
        "kept as they were"
    );
    let taken = after.endpoints[31];
    assert_eq!(taken.place(), (Family::Ipv4, 9032));
    assert!(
        first
            .endpoints
            .iter()
            .all(|endpoint| endpoint.incarnation != taken.incarnation)
    );
}

#[test]
fn an_endpoint_keeps_its_incarnation_while_the_same_listeners_take_it_and_a_partial_listing_keeps_those_last_found()
 {
    let mut incarnations = Incarnations::default();
    let server = listener(30, "127.0.0.1", 8765, false);
    let wildcard = listener(30, "0.0.0.0", 8765, false);
    let chosen = incarnations.list(&endpoints(&listing(vec![server, wildcard]), none));
    let endpoint = chosen.endpoints[0];

    let again = incarnations.list(&endpoints(&listing(vec![wildcard, server]), none));
    let partial = incarnations.list(&Endpoints {
        partial: true,
        ..Endpoints::default()
    });
    let kept = incarnations.owners(&endpoint).map(<[Listener]>::to_vec);
    let older = Endpoint {
        incarnation: endpoint.incarnation + 1,
        ..endpoint
    };

    assert_eq!(again.endpoints, vec![endpoint], "found in another order");
    assert_eq!(partial.endpoints, vec![endpoint]);
    assert_eq!(kept, Some(vec![server, wildcard]));
    assert_eq!(incarnations.owners(&older), None, "another incarnation");
}

#[test]
fn an_endpoint_taken_by_another_process_or_socket_is_a_new_incarnation() {
    let mut incarnations = Incarnations::default();
    let server = listener(30, "127.0.0.1", 8765, false);
    let first = incarnations
        .list(&endpoints(&listing(vec![server]), none))
        .endpoints[0];
    let other_process = listener(31, "127.0.0.1", 8765, false);
    let mut pid_reused = server;
    pid_reused.start = [server.start[0] + 1, server.start[1]];
    let mut reopened = server;
    reopened.socket += 1;

    let mut seen = vec![first];
    for replaced in [other_process, pid_reused, reopened] {
        let now = incarnations
            .list(&endpoints(&listing(vec![replaced]), none))
            .endpoints[0];
        assert_eq!(now.place(), first.place());
        assert!(
            seen.iter()
                .all(|before| before.incarnation != now.incarnation),
            "{replaced:?}"
        );
        assert_eq!(incarnations.owners(&now), Some(&[replaced][..]));
        assert_eq!(incarnations.owners(&seen[seen.len() - 1]), None);
        seen.push(now);
    }
}

#[test]
fn what_was_judged_of_a_process_is_kept_by_its_token_and_forgotten_once_it_is_not_listed() {
    let me = Instance::current().unwrap();
    let mut mine = listener(me.pid, "127.0.0.1", 8767, false);
    mine.start = me.start;
    let account = Account::default();
    account.judged().insert([0; 8], (i32::MAX, true));

    let first = account.excluded(&mine);
    let again = account.excluded(&mine);
    let judged = account.judged().len();
    let _server = TcpListener::bind("127.0.0.1:0").unwrap();
    account.listing().unwrap();

    assert!(!first && !again);
    assert_eq!(judged, 2, "judged once, by its token");
    let left: Vec<i32> = account.judged().values().map(|(pid, _)| *pid).collect();
    assert_eq!(left, vec![me.pid], "a process not listed is forgotten");
}
