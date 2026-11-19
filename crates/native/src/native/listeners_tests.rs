use super::*;
use nix::sys::socket::{
    AddressFamily, Backlog, SockFlag, SockType, SockaddrIn6, bind, listen, setsockopt, socket,
    sockopt,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    mem::{align_of, offset_of},
    net::{SocketAddrV6, TcpListener, TcpStream},
};

fn uid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

fn me() -> Instance {
    Instance::current().unwrap()
}

/// The listeners of this process on `port`, as one listing of this
/// account found them.
fn listed(port: u16) -> Vec<Listener> {
    of_account(uid())
        .unwrap()
        .listeners
        .into_iter()
        .filter(|listener| listener.port == port && listener.pid == me().pid)
        .collect()
}

/// A listener on every IPv6 address, taking IPv6 alone when `v6only`.
fn wildcard_v6(v6only: bool) -> TcpListener {
    let fd = socket(
        AddressFamily::Inet6,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
    .unwrap();
    setsockopt(&fd, sockopt::Ipv6V6Only, &v6only).unwrap();
    bind(
        std::os::fd::AsRawFd::as_raw_fd(&fd),
        &SockaddrIn6::from(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0)),
    )
    .unwrap();
    listen(&fd, Backlog::new(4).unwrap()).unwrap();
    TcpListener::from(fd)
}

#[test]
fn a_listener_opened_by_this_test_is_listed_with_its_family_and_port() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();

    let found = listed(port);

    assert_eq!(found.len(), 1, "{found:?}");
    let listener = found[0];
    assert_eq!(listener.owner(), me());
    assert_eq!(listener.family, Family::Ipv4);
    assert_eq!(listener.bound, Ipv4Addr::LOCALHOST);
    assert!(!listener.v6only);
    assert_ne!(listener.socket, 0);

    drop(server);
    assert!(listed(port).is_empty(), "a closed listener is not listed");
}

#[test]
fn a_v6_only_wildcard_listener_is_listed_v6_only_and_a_dual_stack_one_is_not() {
    let only = wildcard_v6(true);
    let dual = wildcard_v6(false);
    let only_port = only.local_addr().unwrap().port();
    let dual_port = dual.local_addr().unwrap().port();

    let [only] = listed(only_port)[..] else {
        panic!("the v6-only listener is listed once");
    };
    let [dual] = listed(dual_port)[..] else {
        panic!("the dual-stack listener is listed once");
    };

    for listener in [only, dual] {
        assert_eq!(listener.family, Family::Ipv6);
        assert_eq!(listener.bound, Ipv6Addr::UNSPECIFIED);
    }
    assert!(only.v6only);
    assert!(!dual.v6only);
}

#[test]
fn a_listener_on_another_address_is_listed_bound_to_it() {
    let server = TcpListener::bind("[::1]:0").unwrap();
    let port = server.local_addr().unwrap().port();

    let [listener] = listed(port)[..] else {
        panic!("the listener is listed once");
    };

    assert_eq!(listener.family, Family::Ipv6);
    assert_eq!(listener.bound, Ipv6Addr::LOCALHOST);
}

#[test]
fn a_listener_closed_and_opened_again_is_another_socket() {
    let first = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = first.local_addr().unwrap().port();
    let [before] = listed(port)[..] else {
        panic!("the first listener is listed once");
    };
    drop(first);

    let _second = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let [after] = listed(port)[..] else {
        panic!("the second listener is listed once");
    };

    assert_ne!(before.socket, after.socket);
}

#[test]
fn the_hand_declared_socket_info_has_the_sdks_size_offsets_and_alignment() {
    // As the macOS SDK's sys/proc_info.h lays them out, the same for arm64
    // and x86_64, read from a C program built against it.
    assert_eq!(size_of::<libc::proc_fdinfo>(), 8);

    assert_eq!((size_of::<FileInfo>(), align_of::<FileInfo>()), (24, 8));
    assert_eq!(offset_of!(FileInfo, fi_offset), 8);
    assert_eq!(offset_of!(FileInfo, fi_type), 16);
    assert_eq!(offset_of!(FileInfo, fi_guardflags), 20);

    assert_eq!((size_of::<VinfoStat>(), align_of::<VinfoStat>()), (136, 8));
    assert_eq!(
        (size_of::<SockbufInfo>(), align_of::<SockbufInfo>()),
        (24, 4)
    );

    assert_eq!((size_of::<InSockinfo>(), align_of::<InSockinfo>()), (80, 8));
    assert_eq!(offset_of!(InSockinfo, insi_fport), 0);
    assert_eq!(offset_of!(InSockinfo, insi_lport), 4);
    assert_eq!(offset_of!(InSockinfo, insi_gencnt), 8);
    assert_eq!(offset_of!(InSockinfo, insi_flags), 16);
    assert_eq!(offset_of!(InSockinfo, insi_flow), 20);
    assert_eq!(offset_of!(InSockinfo, insi_vflag), 24);
    assert_eq!(offset_of!(InSockinfo, insi_ip_ttl), 25);
    assert_eq!(offset_of!(InSockinfo, rfu_1), 28);
    assert_eq!(offset_of!(InSockinfo, insi_faddr), 32);
    assert_eq!(offset_of!(InSockinfo, insi_laddr), 48);
    assert_eq!(offset_of!(InSockinfo, insi_v4), 64);
    assert_eq!(offset_of!(InSockinfo, insi_v6), 68);

    assert_eq!(
        (size_of::<TcpSockinfo>(), align_of::<TcpSockinfo>()),
        (120, 8)
    );
    assert_eq!(offset_of!(TcpSockinfo, tcpsi_state), 80);
    assert_eq!(offset_of!(TcpSockinfo, tcpsi_timer), 84);
    assert_eq!(offset_of!(TcpSockinfo, tcpsi_mss), 100);
    assert_eq!(offset_of!(TcpSockinfo, tcpsi_flags), 104);
    assert_eq!(offset_of!(TcpSockinfo, rfu_1), 108);
    assert_eq!(offset_of!(TcpSockinfo, tcpsi_tp), 112);

    assert_eq!((size_of::<Proto>(), align_of::<Proto>()), (528, 8));

    assert_eq!(
        (size_of::<SocketInfo>(), align_of::<SocketInfo>()),
        (768, 8)
    );
    assert_eq!(offset_of!(SocketInfo, soi_so), 136);
    assert_eq!(offset_of!(SocketInfo, soi_pcb), 144);
    assert_eq!(offset_of!(SocketInfo, soi_type), 152);
    assert_eq!(offset_of!(SocketInfo, soi_protocol), 156);
    assert_eq!(offset_of!(SocketInfo, soi_family), 160);
    assert_eq!(offset_of!(SocketInfo, soi_options), 164);
    assert_eq!(offset_of!(SocketInfo, soi_state), 168);
    assert_eq!(offset_of!(SocketInfo, soi_error), 178);
    assert_eq!(offset_of!(SocketInfo, soi_oobmark), 180);
    assert_eq!(offset_of!(SocketInfo, soi_rcv), 184);
    assert_eq!(offset_of!(SocketInfo, soi_snd), 208);
    assert_eq!(offset_of!(SocketInfo, soi_kind), 232);
    assert_eq!(offset_of!(SocketInfo, rfu_1), 236);
    assert_eq!(offset_of!(SocketInfo, soi_proto), 240);

    assert_eq!(
        (size_of::<SocketFdinfo>(), align_of::<SocketFdinfo>()),
        (792, 8)
    );
    assert_eq!(offset_of!(SocketFdinfo, pfi), 0);
    assert_eq!(offset_of!(SocketFdinfo, psi), 24);

    assert_eq!(
        (
            PROC_UID_ONLY,
            PROC_PIDFDSOCKETINFO,
            SOCKINFO_TCP,
            TSI_S_LISTEN
        ),
        (4, 3, 2, 1)
    );
    assert_eq!((INI_IPV4, INI_IPV6, IN6P_IPV6_V6ONLY), (1, 2, 0x8000));
}

/// One process as a stand-in kernel describes it: as it is at its first
/// read and at every read after, its other descriptors and its sockets.
#[derive(Clone)]
struct Described {
    before: Process,
    after: Process,
    others: usize,
    sockets: Vec<Tcp>,
}

/// A stand-in for the kernel's lists, honouring each room as the kernel
/// does.
#[derive(Default)]
struct Fake {
    processes: BTreeMap<i32, Described>,
    /// PIDs listed whose processes are gone by the time they are read.
    gone: Vec<i32>,
    unreadable: Vec<i32>,
    reads: RefCell<BTreeMap<i32, usize>>,
}

/// The first descriptor a stand-in socket has.
const FIRST_SOCKET: i32 = 100;

impl Kernel for Fake {
    fn pids(&self, _: u32, room: usize) -> io::Result<Vec<i32>> {
        let mut pids: Vec<i32> = self.processes.keys().copied().collect();
        pids.extend(&self.gone);
        pids.extend(&self.unreadable);
        pids.truncate(room);
        Ok(pids)
    }

    fn descriptors(&self, pid: i32, room: usize) -> io::Result<Option<Vec<libc::proc_fdinfo>>> {
        if self.unreadable.contains(&pid) {
            return Err(io::Error::from_raw_os_error(libc::EPERM));
        }
        let Some(process) = self.processes.get(&pid) else {
            return Ok(None);
        };
        let others = (0..process.others).map(|fd| libc::proc_fdinfo {
            proc_fd: i32::try_from(fd).unwrap(),
            proc_fdtype: 1,
        });
        let sockets = (0..process.sockets.len()).map(|index| libc::proc_fdinfo {
            proc_fd: FIRST_SOCKET + i32::try_from(index).unwrap(),
            proc_fdtype: SOCKET,
        });
        let mut descriptors: Vec<_> = sockets.chain(others).collect();
        descriptors.truncate(room);
        Ok(Some(descriptors))
    }

    fn process(&self, pid: i32) -> io::Result<Option<Process>> {
        let mut reads = self.reads.borrow_mut();
        let read = reads.entry(pid).or_default();
        *read += 1;
        Ok(self.processes.get(&pid).map(|process| {
            if *read == 1 {
                process.before
            } else {
                process.after
            }
        }))
    }

    fn socket(&self, pid: i32, fd: i32) -> io::Result<Option<Tcp>> {
        let index = usize::try_from(fd - FIRST_SOCKET).unwrap();
        Ok(self.processes[&pid].sockets.get(index).copied())
    }
}

const OWN: u32 = 501;

fn listening_on(port: u16) -> Tcp {
    Tcp {
        family: Family::Ipv4,
        local: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        foreign: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        listening: true,
        v6only: false,
        generation: u64::from(port),
    }
}

/// A process of this account started at `started`, listening on `ports`.
fn serving(started: u64, ports: &[u16]) -> Described {
    let process = Process {
        start: [started, 0],
        uid: OWN,
    };
    Described {
        before: process,
        after: process,
        others: 3,
        sockets: ports.iter().map(|port| listening_on(*port)).collect(),
    }
}

fn ports(listing: &Listing) -> Vec<u16> {
    listing
        .listeners
        .iter()
        .map(|listener| listener.port)
        .collect()
}

#[test]
fn a_listing_of_every_process_within_its_bounds_is_whole() {
    let mut kernel = Fake::default();
    kernel.processes.insert(10, serving(1, &[3000, 3001]));
    kernel.processes.insert(11, serving(2, &[]));
    kernel.processes.insert(12, serving(3, &[8765]));
    kernel.gone.push(13);

    let listing = listing(&kernel, OWN).unwrap();

    assert!(!listing.partial);
    assert_eq!(ports(&listing), vec![3000, 3001, 8765]);
    assert_eq!(listing.listeners[2].owner(), Instance::new(12, [3, 0]));
}

#[test]
fn a_bound_hit_reports_a_partial_listing() {
    let mut at_the_bound = Fake::default();
    for pid in 1..=i32::try_from(PROCESSES).unwrap() {
        at_the_bound.processes.insert(pid, serving(1, &[]));
    }
    let mut many_processes = Fake {
        processes: at_the_bound.processes.clone(),
        ..Fake::default()
    };
    many_processes.gone.push(i32::MAX);
    let mut many_descriptors = Fake::default();
    let mut crowded = serving(1, &[8765]);
    crowded.others = DESCRIPTORS;
    many_descriptors.processes.insert(10, crowded);
    let mut full = Fake::default();
    let mut filled = serving(1, &[8765]);
    filled.others = DESCRIPTORS - 1;
    full.processes.insert(10, filled);

    assert!(!listing(&at_the_bound, OWN).unwrap().partial);
    assert!(listing(&many_processes, OWN).unwrap().partial);
    let crowded = listing(&many_descriptors, OWN).unwrap();
    assert!(crowded.partial);
    assert_eq!(ports(&crowded), vec![8765], "what was read is still listed");
    assert!(!listing(&full, OWN).unwrap().partial);
}

#[test]
fn a_process_that_cannot_be_read_makes_the_listing_partial() {
    let mut kernel = Fake::default();
    kernel.processes.insert(10, serving(1, &[8765]));
    kernel.unreadable.push(11);

    let listing = listing(&kernel, OWN).unwrap();

    assert!(listing.partial);
    assert_eq!(ports(&listing), vec![8765]);
}

#[test]
fn a_process_of_another_account_or_replaced_while_read_lends_no_listener() {
    let mut kernel = Fake::default();
    let mut other_account = serving(1, &[3000]);
    other_account.before.uid = 502;
    other_account.after.uid = 502;
    let mut account_changed = serving(2, &[3001]);
    account_changed.after.uid = 502;
    let mut replaced = serving(3, &[3002]);
    replaced.after.start = [4, 0];
    kernel.processes.insert(10, other_account);
    kernel.processes.insert(11, account_changed);
    kernel.processes.insert(12, replaced);
    kernel.processes.insert(13, serving(5, &[3003]));

    let listing = listing(&kernel, OWN).unwrap();

    assert_eq!(ports(&listing), vec![3003]);
    assert!(!listing.partial);
}

#[test]
fn the_kernel_gives_no_more_processes_or_descriptors_than_the_room_asked() {
    let _servers = [
        TcpListener::bind("127.0.0.1:0").unwrap(),
        TcpListener::bind("127.0.0.1:0").unwrap(),
    ];

    assert_eq!(pids(uid(), 2).unwrap().len(), 2);
    assert_eq!(descriptors(me().pid, 2).unwrap().unwrap().len(), 2);
    assert!(descriptors(me().pid, DESCRIPTORS).unwrap().unwrap().len() > 2);
}

#[test]
fn a_process_read_through_the_kernel_is_its_start_and_account() {
    let process = process(me().pid).unwrap().unwrap();

    assert_eq!(process.start, me().start);
    assert_eq!(process.uid, uid());
}

#[test]
fn a_returned_length_larger_than_its_buffer_is_refused() {
    let refused = entries(36, 32, 4).unwrap_err();

    assert_eq!(refused.kind(), io::ErrorKind::InvalidData);
    assert_eq!(entries(32, 32, 4).unwrap(), 8);
    assert_eq!(entries(0, 32, 4).unwrap(), 0);
    assert!(entries(6, 32, 4).is_err(), "a length within an entry");
    assert!(entries(-1, 32, 4).is_err(), "a negative length");
}

#[test]
fn held_by_names_this_tests_own_process_for_a_connection_it_made_and_accepted() {
    for address in ["127.0.0.1:0", "[::1]:0"] {
        let server = TcpListener::bind(address).unwrap();
        let client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        let (_accepted, _) = server.accept().unwrap();

        let found = held_by(client.local_addr().unwrap(), client.peer_addr().unwrap()).unwrap();

        assert_eq!(found, Some(me()), "{address}");
    }
}

#[test]
fn held_by_names_a_dual_stack_listeners_process_for_an_ipv4_connection() {
    let server = wildcard_v6(false);
    let port = server.local_addr().unwrap().port();
    let client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (_accepted, _) = server.accept().unwrap();

    let found = held_by(client.local_addr().unwrap(), client.peer_addr().unwrap()).unwrap();

    assert_eq!(found, Some(me()));
}

#[test]
fn held_by_names_nobody_for_an_address_pair_nobody_holds() {
    let unheld = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = unheld.local_addr().unwrap();
    drop(unheld);

    let found = held_by(local, SocketAddr::from((Ipv4Addr::LOCALHOST, 9))).unwrap();

    assert_eq!(found, None);
}

#[test]
fn a_connection_waiting_in_its_listeners_queue_is_held_by_nobody_yet() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(server.local_addr().unwrap()).unwrap();

    let found = held_by(client.local_addr().unwrap(), client.peer_addr().unwrap()).unwrap();

    assert_eq!(found, None);
}

#[test]
fn a_connection_held_by_two_processes_or_found_in_a_partial_listing_is_unknown() {
    let local = SocketAddr::from((Ipv4Addr::LOCALHOST, 50_000));
    let peer = SocketAddr::from((Ipv4Addr::LOCALHOST, 8765));
    let end = Tcp {
        family: Family::Ipv4,
        local: peer,
        foreign: local,
        listening: false,
        v6only: false,
        generation: 7,
    };
    let one = Instance::new(10, [1, 0]);
    let other = Instance::new(11, [2, 0]);

    let held =
        |found: Vec<(Instance, Tcp)>, partial| holder(&Sockets { found, partial }, local, peer);

    assert_eq!(held(vec![(one, end)], false), Some(one));
    assert_eq!(held(vec![(one, end), (one, end)], false), Some(one));
    assert_eq!(held(vec![(one, end), (other, end)], false), None);
    assert_eq!(held(vec![(one, end)], true), None);
}
