use super::*;
use atlas_core::process::Instance;
use atlas_native::native::listeners::Family as Socket;
use std::net::{IpAddr, TcpListener};

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
        mirrored: mirrored.clone(),
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
}

fn mirrored(listeners: &impl Listeners) -> BTreeSet<(Family, u16)> {
    endpoints(&listeners.listing().unwrap(), |listener| {
        listeners.excluded(listener)
    })
    .mirrored
}

#[test]
fn a_listener_opened_by_this_test_is_mirrored_on_its_family_and_port() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();

    let places = mirrored(&Account);

    assert!(places.contains(&(Family::Ipv4, port)), "{places:?}");
    assert!(!places.contains(&(Family::Ipv6, port)));
}

#[test]
fn loopback_and_every_address_listeners_give_their_families() {
    let places = endpoints(
        &listing(vec![
            listener(10, "127.0.0.1", 8000, false),
            listener(10, "0.0.0.0", 8001, false),
            listener(10, "::1", 8002, false),
        ]),
        none,
    )
    .mirrored;

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
    let places = endpoints(
        &listing(vec![
            listener(10, "::", 8765, true),
            listener(10, "::", 8766, false),
        ]),
        none,
    )
    .mirrored;

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
    let places = endpoints(
        &listing(vec![
            listener(10, "192.168.1.5", 8000, false),
            listener(10, "10.0.0.1", 8001, false),
            listener(10, "127.0.0.2", 8002, false),
            listener(10, "fe80::1", 8003, true),
            listener(10, "::ffff:127.0.0.1", 8004, false),
            listener(10, "2001:db8::1", 8005, false),
        ]),
        none,
    )
    .mirrored;

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
    let me = Instance::current().unwrap();
    let mut other_start = listener(me.pid, "127.0.0.1", 8765, false);
    other_start.start = [me.start[0].saturating_sub(1), me.start[1]];
    let mut gone = listener(i32::MAX, "127.0.0.1", 8766, false);
    gone.start = me.start;
    let mut mine = listener(me.pid, "127.0.0.1", 8767, false);
    mine.start = me.start;

    assert!(Account.excluded(&other_start));
    assert!(Account.excluded(&gone));
    assert!(
        !Account.excluded(&mine),
        "this test's own process is neither"
    );
}

#[test]
fn an_endpoint_listed_again_after_it_left_gets_a_new_incarnation() {
    let mut incarnations = Incarnations::default();
    let both = BTreeSet::from([(Family::Ipv4, 8765), (Family::Ipv6, 8766)]);
    let one = BTreeSet::from([(Family::Ipv6, 8766)]);

    let first = incarnations.list(&found(&both, false));
    let kept = incarnations.list(&found(&both, false));
    let left = incarnations.list(&found(&one, false));
    let again = incarnations.list(&found(&both, false));

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
    let first = incarnations.list(&found(&BTreeSet::from([(Family::Ipv4, 8765)]), false));

    let partial = incarnations.list(&found(&BTreeSet::from([(Family::Ipv4, 8766)]), true));

    assert_eq!(partial.len(), 2);
    assert_eq!(partial[0], first[0]);
    assert_eq!(partial[1].place(), (Family::Ipv4, 8766));
}

#[test]
fn endpoints_are_listed_lowest_port_first_and_ipv4_before_ipv6() {
    let mut incarnations = Incarnations::default();

    let listed = incarnations.list(&found(
        &BTreeSet::from([
            (Family::Ipv6, 3000),
            (Family::Ipv4, 9000),
            (Family::Ipv4, 3000),
        ]),
        false,
    ));

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

    assert_eq!(found.mirrored, BTreeSet::from([(Family::Ipv6, 7000)]));
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

    let listed = incarnations.list(&Endpoints {
        mirrored: BTreeSet::new(),
        excluded: BTreeSet::from([(Family::Ipv4, 8765)]),
        partial: true,
    });

    let places: Vec<_> = listed.iter().map(Endpoint::place).collect();
    assert_eq!(places, vec![(Family::Ipv4, 8766)]);
}
