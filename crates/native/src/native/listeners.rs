//! This account's TCP sockets as the kernel lists them through libproc: the
//! ones listening, each with the process that holds it, and which process
//! holds the other end of a connection made to one.
//!
//! Every process is named by its PID with its start time, read before and
//! after its sockets, so a PID the kernel gave another process meanwhile
//! never lends that process's sockets to this one. A listing reads a bounded
//! number of processes and of each one's descriptors, and says when it hit a
//! bound, as what it left out is not known.
#![allow(
    unsafe_code,
    reason = "FFI: libproc's process and socket listings have no safe binding"
)]
use atlas_core::process::{Instance, Start};
use std::{
    io,
    mem::MaybeUninit,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

/// The most processes one listing reads.
const PROCESSES: usize = 4096;
/// The most descriptors one listing reads of each process.
const DESCRIPTORS: usize = 1024;

/// `PROC_UID_ONLY` of `sys/proc_info.h`: processes by effective user ID.
const PROC_UID_ONLY: u32 = 4;
/// `PROC_PIDFDSOCKETINFO` of `sys/proc_info.h`.
const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
/// `SOCKINFO_TCP` of `sys/proc_info.h`: `soi_proto` holds a `tcp_sockinfo`.
const SOCKINFO_TCP: libc::c_int = 2;
/// `TSI_S_LISTEN` of `sys/proc_info.h`.
const TSI_S_LISTEN: libc::c_int = 1;
/// `INI_IPV4` and `INI_IPV6` of `sys/proc_info.h`, in `insi_vflag`.
const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;
/// `IN6P_IPV6_V6ONLY` of `netinet/in_pcb.h`, in `insi_flags`.
const IN6P_IPV6_V6ONLY: u32 = 0x8000;
/// `PROX_FDTYPE_SOCKET` of `sys/proc_info.h`, as `proc_fdtype` holds it.
const SOCKET: u32 = libc::PROX_FDTYPE_SOCKET as u32;
/// `SZOMB` of `sys/proc.h`, in `pbi_status`: exited, not yet reaped.
const SZOMB: u32 = 5;

/// `struct proc_fileinfo` of `sys/proc_info.h`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct FileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vinfo_stat` of `sys/proc_info.h`, of which nothing is read.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only its size is used"
)]
struct VinfoStat([u64; 17]);

/// `struct sockbuf_info` of `sys/proc_info.h`, of which nothing is read.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only its size is used"
)]
struct SockbufInfo([u32; 6]);

/// `struct in_sockinfo`'s `insi_v6` of `sys/proc_info.h`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only its size is used"
)]
struct InV6 {
    in6_hlim: u8,
    in6_cksum: i32,
    in6_ifindex: u16,
    in6_hops: i16,
}

/// `struct in_sockinfo` of `sys/proc_info.h`. Each address is the union of
/// an `in4in6_addr`, whose last word is the IPv4 address, and an `in6_addr`,
/// both in network order.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct InSockinfo {
    insi_fport: i32,
    insi_lport: i32,
    insi_gencnt: u64,
    insi_flags: u32,
    insi_flow: u32,
    insi_vflag: u8,
    insi_ip_ttl: u8,
    rfu_1: u32,
    insi_faddr: [u32; 4],
    insi_laddr: [u32; 4],
    insi_v4: u8,
    insi_v6: InV6,
}

/// `struct tcp_sockinfo` of `sys/proc_info.h`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct TcpSockinfo {
    tcpsi_ini: InSockinfo,
    tcpsi_state: i32,
    tcpsi_timer: [i32; 4],
    tcpsi_mss: i32,
    tcpsi_flags: u32,
    rfu_1: u32,
    tcpsi_tp: u64,
}

/// `struct socket_info`'s `soi_proto` union of `sys/proc_info.h`: its TCP
/// member, which also begins as its `in_sockinfo` member does, with room for
/// its largest, `un_sockinfo`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct Proto {
    tcp: TcpSockinfo,
    rest: [u8; 528 - size_of::<TcpSockinfo>()],
}

/// `struct socket_info` of `sys/proc_info.h`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct SocketInfo {
    soi_stat: VinfoStat,
    soi_so: u64,
    soi_pcb: u64,
    soi_type: i32,
    soi_protocol: i32,
    soi_family: i32,
    soi_options: i16,
    soi_linger: i16,
    soi_state: i16,
    soi_qlen: i16,
    soi_incqlen: i16,
    soi_qlimit: i16,
    soi_timeo: i16,
    soi_error: u16,
    soi_oobmark: u32,
    soi_rcv: SockbufInfo,
    soi_snd: SockbufInfo,
    soi_kind: i32,
    rfu_1: u32,
    soi_proto: Proto,
}

/// `struct socket_fdinfo` of `sys/proc_info.h`: what `PROC_PIDFDSOCKETINFO`
/// fills.
#[repr(C)]
#[allow(
    dead_code,
    reason = "a layout mirror of the SDK's: only some fields are read"
)]
struct SocketFdinfo {
    pfi: FileInfo,
    psi: SocketInfo,
}

/// The address family of a socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    Ipv4,
    Ipv6,
}

/// One TCP socket listening, in a process of this account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listener {
    /// The process that holds it, with the time it started.
    pub pid: i32,
    pub start: Start,
    pub family: Family,
    /// The address it is bound to: unspecified for every address.
    pub bound: IpAddr,
    pub port: u16,
    /// Whether an IPv6 socket takes IPv6 alone, never IPv4.
    pub v6only: bool,
    /// The kernel's generation count for the socket: no other socket since
    /// boot has it, so a listener closed and opened again is told apart.
    pub socket: u64,
}

impl Listener {
    /// The process that holds it.
    #[must_use]
    pub fn owner(&self) -> Instance {
        Instance::new(self.pid, self.start)
    }
}

/// This account's listeners as one listing found them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub listeners: Vec<Listener>,
    /// Whether it hit a bound, or a process could not be read, so that
    /// listeners may be missing from it.
    pub partial: bool,
}

/// One TCP socket over IPv4 or IPv6, as its process holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tcp {
    family: Family,
    local: SocketAddr,
    foreign: SocketAddr,
    listening: bool,
    v6only: bool,
    generation: u64,
}

/// Every TCP socket of this account's processes, each with its process.
struct Sockets {
    found: Vec<(Instance, Tcp)>,
    partial: bool,
}

/// A process as the kernel describes it: when it started and its
/// effective account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Process {
    start: Start,
    uid: u32,
}

/// The kernel's lists, given apart so tests stand in. Each asked for at
/// most `room` entries gives at most that many.
trait Kernel {
    /// The processes whose effective user ID is `uid`.
    fn pids(&self, uid: u32, room: usize) -> io::Result<Vec<i32>>;

    /// The descriptors process `pid` has open; none when it is gone.
    fn descriptors(&self, pid: i32, room: usize) -> io::Result<Option<Vec<libc::proc_fdinfo>>>;

    /// Process `pid`; none when it is gone or a zombie.
    fn process(&self, pid: i32) -> io::Result<Option<Process>>;

    /// The TCP socket process `pid`'s descriptor `fd` is; none when it is
    /// another kind of socket, or the descriptor or the process is gone.
    fn socket(&self, pid: i32, fd: i32) -> io::Result<Option<Tcp>>;
}

/// The kernel itself, through libproc.
struct Libproc;

impl Kernel for Libproc {
    fn pids(&self, uid: u32, room: usize) -> io::Result<Vec<i32>> {
        pids(uid, room)
    }

    fn descriptors(&self, pid: i32, room: usize) -> io::Result<Option<Vec<libc::proc_fdinfo>>> {
        descriptors(pid, room)
    }

    fn process(&self, pid: i32) -> io::Result<Option<Process>> {
        process(pid)
    }

    fn socket(&self, pid: i32, fd: i32) -> io::Result<Option<Tcp>> {
        Ok(socket(pid, fd)?.as_ref().and_then(tcp))
    }
}

/// The TCP listeners of the processes whose effective user ID is `uid`.
///
/// # Errors
///
/// Returns an error when the kernel does not list this account's processes,
/// or answers with a length its buffer could not hold.
pub fn of_account(uid: u32) -> io::Result<Listing> {
    listing(&Libproc, uid)
}

fn listing(kernel: &impl Kernel, uid: u32) -> io::Result<Listing> {
    let sockets = sockets(kernel, uid)?;
    let listeners = sockets
        .found
        .into_iter()
        .filter(|(_, tcp)| tcp.listening)
        .map(|(owner, tcp)| Listener {
            pid: owner.pid,
            start: owner.start,
            family: tcp.family,
            bound: tcp.local.ip(),
            port: tcp.local.port(),
            v6only: tcp.v6only,
            socket: tcp.generation,
        })
        .collect();
    Ok(Listing {
        listeners,
        partial: sockets.partial,
    })
}

/// The process of this account that holds the other end of the connection
/// whose own address is `local` and whose peer is `peer`; none when no
/// process holds it yet, as while it waits in its listener's queue, when
/// more than one process holds it, or when the listing was partial: unknown,
/// never a guess.
///
/// It names who holds that end now, not who accepted it, and that is what a
/// check before any byte needs. A descriptor reaches another process only
/// as one of this account's processes passes it on, and passing it is that
/// process choosing to serve the connection. What must never get the
/// connection, an excluded listener's process or another account's, is
/// caught all the same, as it would be the holder.
///
/// # Errors
///
/// Returns an error when the kernel does not list this account's processes,
/// or answers with a length its buffer could not hold.
pub fn held_by(local: SocketAddr, peer: SocketAddr) -> io::Result<Option<Instance>> {
    let sockets = sockets(&Libproc, nix::unistd::geteuid().as_raw())?;
    Ok(holder(&sockets, local, peer))
}

/// The one process in `sockets` holding the end of `local`'s connection to
/// `peer`.
fn holder(sockets: &Sockets, local: SocketAddr, peer: SocketAddr) -> Option<Instance> {
    if sockets.partial {
        return None;
    }
    let mut holders = sockets
        .found
        .iter()
        .filter(|(_, tcp)| !tcp.listening && same(tcp.local, peer) && same(tcp.foreign, local))
        .map(|(owner, _)| *owner);
    let first = holders.next()?;
    holders.all(|other| other == first).then_some(first)
}

/// Whether `one` and `other` name the same address and port, whatever
/// IPv6 flow or scope either carries.
fn same(one: SocketAddr, other: SocketAddr) -> bool {
    one.ip() == other.ip() && one.port() == other.port()
}

fn sockets(kernel: &impl Kernel, uid: u32) -> io::Result<Sockets> {
    // One more than the bound: as many as that says there were more.
    let mut pids = kernel.pids(uid, PROCESSES + 1)?;
    let mut partial = pids.len() > PROCESSES;
    pids.truncate(PROCESSES);
    let mut found = Vec::new();
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        match of_process(kernel, uid, pid) {
            Ok(Some(process)) => {
                partial |= process.partial;
                found.extend(process.found);
            }
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidData => return Err(error),
            Err(_) => partial = true,
        }
    }
    Ok(Sockets { found, partial })
}

/// The TCP sockets of process `pid`, when it is of the account `uid`; none
/// when it ended, or was another process or another account's by the end
/// of the read.
fn of_process(kernel: &impl Kernel, uid: u32, pid: i32) -> io::Result<Option<Sockets>> {
    let Some(mut descriptors) = kernel.descriptors(pid, DESCRIPTORS + 1)? else {
        return Ok(None);
    };
    let partial = descriptors.len() > DESCRIPTORS;
    descriptors.truncate(DESCRIPTORS);
    let mut sockets = descriptors
        .iter()
        .filter(|descriptor| descriptor.proc_fdtype == SOCKET)
        .map(|descriptor| descriptor.proc_fd)
        .peekable();
    if sockets.peek().is_none() {
        return Ok(Some(Sockets {
            found: Vec::new(),
            partial,
        }));
    }
    // The same process of the same account before and after its sockets:
    // each was read of this one, never of another given its PID meanwhile.
    let Some(before) = kernel.process(pid)?.filter(|process| process.uid == uid) else {
        return Ok(None);
    };
    let mut found = Vec::new();
    for fd in sockets {
        if let Some(tcp) = kernel.socket(pid, fd)? {
            found.push((Instance::new(pid, before.start), tcp));
        }
    }
    if !found.is_empty() && kernel.process(pid)? != Some(before) {
        return Ok(None);
    }
    Ok(Some(Sockets { found, partial }))
}

/// Process `pid` as the kernel keeps it; none when it is gone or a zombie.
fn process(pid: i32) -> io::Result<Option<Process>> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let room = libc::c_int::try_from(size_of::<libc::proc_bsdinfo>()).map_err(io::Error::other)?;
    nix::errno::Errno::clear();
    // SAFETY: the buffer is exactly one `proc_bsdinfo`, aligned for it and
    // writable for the byte length passed; the kernel writes at most that.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            room,
        )
    };
    if filled == 0
        && let Some(error) = failure()
    {
        return gone_or(error);
    }
    if entries(filled, room, size_of::<libc::proc_bsdinfo>())? != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The kernel filled part of a process's information",
        ));
    }
    // SAFETY: the buffer was zeroed, and every field of the structure is a
    // plain integer, so any bytes the kernel wrote or left make a valid one.
    let info = unsafe { info.assume_init() };
    Ok(
        (i64::from(info.pbi_pid) == i64::from(pid) && info.pbi_status != SZOMB).then_some(
            Process {
                start: [info.pbi_start_tvsec, info.pbi_start_tvusec],
                uid: info.pbi_uid,
            },
        ),
    )
}

/// The processes whose effective user ID is `uid`, at most `room` of them.
fn pids(uid: u32, room: usize) -> io::Result<Vec<i32>> {
    let mut pids: Vec<libc::c_int> = vec![0; room];
    let bytes = byte_length(&pids)?;
    // SAFETY: the buffer is writable for the byte length passed, and the
    // kernel writes whole process IDs into it.
    let filled =
        unsafe { libc::proc_listpids(PROC_UID_ONLY, uid, pids.as_mut_ptr().cast(), bytes) };
    // The call answers 0 for a failure; this process is always one of its
    // account's, so a success is never empty.
    if filled == 0 {
        return Err(io::Error::last_os_error());
    }
    pids.truncate(entries(filled, bytes, size_of::<libc::c_int>())?);
    Ok(pids)
}

/// The descriptors process `pid` has open, at most `room` of them; none
/// when it is gone.
fn descriptors(pid: i32, room: usize) -> io::Result<Option<Vec<libc::proc_fdinfo>>> {
    let empty = libc::proc_fdinfo {
        proc_fd: 0,
        proc_fdtype: 0,
    };
    let mut descriptors = vec![empty; room];
    let bytes = byte_length(&descriptors)?;
    // The call answers 0 both for no descriptors and for a failure, and
    // only a failure sets the error number.
    nix::errno::Errno::clear();
    // SAFETY: the buffer is writable for the byte length passed, and the
    // kernel writes whole `proc_fdinfo` entries into it.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            descriptors.as_mut_ptr().cast(),
            bytes,
        )
    };
    if filled == 0
        && let Some(error) = failure()
    {
        return gone_or(error);
    }
    descriptors.truncate(entries(filled, bytes, size_of::<libc::proc_fdinfo>())?);
    Ok(Some(descriptors))
}

/// What process `pid`'s descriptor `fd` says of its socket; none when the
/// descriptor or the process is gone.
fn socket(pid: i32, fd: i32) -> io::Result<Option<SocketFdinfo>> {
    let mut info = MaybeUninit::<SocketFdinfo>::zeroed();
    let room = libc::c_int::try_from(size_of::<SocketFdinfo>()).map_err(io::Error::other)?;
    nix::errno::Errno::clear();
    // SAFETY: the buffer is exactly one `socket_fdinfo`, aligned for it and
    // writable for the byte length passed; the kernel writes at most that.
    let filled = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            info.as_mut_ptr().cast(),
            room,
        )
    };
    if filled == 0
        && let Some(error) = failure()
    {
        return gone_or(error);
    }
    if entries(filled, room, size_of::<SocketFdinfo>())? != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The kernel filled part of a socket's information",
        ));
    }
    // SAFETY: the buffer was zeroed, and every field of the structure is a
    // plain integer, so any bytes the kernel wrote or left make a valid one.
    Ok(Some(unsafe { info.assume_init() }))
}

/// The error number a call that answered 0 set, if it set one.
fn failure() -> Option<io::Error> {
    (nix::errno::Errno::last_raw() != 0).then(io::Error::last_os_error)
}

/// None for an error that says what was asked of is gone, else the error.
fn gone_or<T>(error: io::Error) -> io::Result<Option<T>> {
    match error.raw_os_error() {
        Some(libc::ESRCH | libc::EBADF) => Ok(None),
        _ => Err(error),
    }
}

/// How many whole entries of `entry` bytes a call filled, when it says it
/// filled `filled` bytes of a buffer of `room`. A length the buffer could
/// not hold, or that ends within an entry, is refused.
fn entries(filled: libc::c_int, room: libc::c_int, entry: usize) -> io::Result<usize> {
    usize::try_from(filled)
        .ok()
        .filter(|filled| {
            usize::try_from(room).is_ok_and(|room| *filled <= room) && filled % entry == 0
        })
        .map(|filled| filled / entry)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "The kernel said it filled {filled} bytes of a {room}-byte buffer of \
                     {entry}-byte entries"
                ),
            )
        })
}

/// The byte length of `buffer`, as libproc takes it.
fn byte_length<T>(buffer: &[T]) -> io::Result<libc::c_int> {
    libc::c_int::try_from(size_of_val(buffer)).map_err(io::Error::other)
}

/// The TCP socket `info` describes; none for any other.
fn tcp(info: &SocketFdinfo) -> Option<Tcp> {
    let socket = &info.psi;
    let family = match socket.soi_family {
        libc::AF_INET => Family::Ipv4,
        libc::AF_INET6 => Family::Ipv6,
        _ => return None,
    };
    if socket.soi_kind != SOCKINFO_TCP {
        return None;
    }
    let tcp = &socket.soi_proto.tcp;
    let internet = &tcp.tcpsi_ini;
    Some(Tcp {
        family,
        local: address(
            internet.insi_vflag,
            internet.insi_laddr,
            internet.insi_lport,
        )?,
        foreign: address(
            internet.insi_vflag,
            internet.insi_faddr,
            internet.insi_fport,
        )?,
        listening: tcp.tcpsi_state == TSI_S_LISTEN,
        v6only: family == Family::Ipv6 && internet.insi_flags & IN6P_IPV6_V6ONLY != 0,
        generation: internet.insi_gencnt,
    })
}

/// The address and port a socket's protocol block holds: IPv6 when it has
/// IPv6, else the IPv4 address in its last word. Both are in network order,
/// as the words are laid out in memory.
fn address(versions: u8, words: [u32; 4], port: i32) -> Option<SocketAddr> {
    let port = u16::from_be(u16::try_from(port).ok()?);
    let ip = if versions & INI_IPV6 != 0 {
        let mut bytes = [0; 16];
        for (bytes, word) in bytes.chunks_exact_mut(4).zip(words) {
            bytes.copy_from_slice(&word.to_ne_bytes());
        }
        IpAddr::from(Ipv6Addr::from(bytes))
    } else if versions & INI_IPV4 != 0 {
        IpAddr::from(Ipv4Addr::from(words[3].to_ne_bytes()))
    } else {
        return None;
    };
    Some(SocketAddr::new(ip, port))
}

#[cfg(test)]
#[path = "listeners_tests.rs"]
mod tests;
