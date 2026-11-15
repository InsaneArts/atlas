//! The guest's end of a VM's host-to-guest sockets: a listening socket on a
//! port, and each connection made to it, as descriptors this process owns,
//! non-blocking and never inherited by a program it starts.
use nix::{
    fcntl::{FcntlArg, FdFlag, OFlag, fcntl},
    sys::socket::{AddressFamily, Backlog, SockFlag, SockType, VsockAddr, bind, listen, socket},
};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// Connections waiting to be taken, at most.
const BACKLOG: i32 = 64;

/// Listen on vsock `port`, for the host only reaches a guest that way.
///
/// # Errors
///
/// Fails when the socket cannot be made, bound or listened on: outside a
/// VM, or with the port taken.
pub fn listen_on(port: u32) -> std::io::Result<OwnedFd> {
    let listening = {
        // No program starts while it is open and not yet closed on exec.
        let _alone = crate::spawn::hold();
        let listening = socket(
            AddressFamily::Vsock,
            SockType::Stream,
            SockFlag::empty(),
            None,
        )?;
        private(&listening)?;
        listening
    };
    bind(
        listening.as_raw_fd(),
        &VsockAddr::new(libc::VMADDR_CID_ANY, port),
    )?;
    listen(&listening, Backlog::new(BACKLOG)?)?;
    Ok(listening)
}

/// The next connection made to the listening socket `listening`, of any
/// family; `WouldBlock` when none is waiting.
///
/// # Errors
///
/// Fails as accept(2) does.
pub fn accept(listening: &OwnedFd) -> std::io::Result<OwnedFd> {
    // No program starts while it is open and not yet closed on exec.
    let _alone = crate::spawn::hold();
    let accepted = owned(nix::sys::socket::accept(listening.as_raw_fd())?);
    private(&accepted)?;
    Ok(accepted)
}

/// Make `descriptor` non-blocking and closed on exec.
fn private(descriptor: &OwnedFd) -> std::io::Result<()> {
    fcntl(descriptor, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?;
    let flags = OFlag::from_bits_truncate(fcntl(descriptor, FcntlArg::F_GETFL)?);
    fcntl(descriptor, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    Ok(())
}

#[allow(
    unsafe_code,
    reason = "nix's accept gives a raw descriptor, which nothing else owns"
)]
fn owned(raw: RawFd) -> OwnedFd {
    // SAFETY: `raw` was just returned by a successful accept(2) and nothing
    // else in this process holds it, so this is its only owner.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[cfg(test)]
#[path = "vsock_tests.rs"]
mod tests;
