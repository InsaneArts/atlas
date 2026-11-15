use super::*;
use nix::sys::socket::SockaddrLike;
use std::io::{Read, Write};

#[test]
fn a_connection_is_taken_owned_non_blocking_and_closed_on_exec() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("socket");
    let listening = OwnedFd::from(std::os::unix::net::UnixListener::bind(&path).unwrap());
    private(&listening).unwrap();

    let none = accept(&listening).unwrap_err();
    assert_eq!(none.kind(), std::io::ErrorKind::WouldBlock);

    let mut station = std::os::unix::net::UnixStream::connect(&path).unwrap();
    let accepted = accept(&listening).unwrap();
    let descriptor = fcntl(&accepted, FcntlArg::F_GETFD).unwrap();
    assert!(FdFlag::from_bits_truncate(descriptor).contains(FdFlag::FD_CLOEXEC));
    let status = OFlag::from_bits_truncate(fcntl(&accepted, FcntlArg::F_GETFL).unwrap());
    assert!(status.contains(OFlag::O_NONBLOCK));

    let mut guest = std::os::unix::net::UnixStream::from(accepted);
    station.write_all(b"hello").unwrap();
    guest.set_nonblocking(false).unwrap();
    let mut heard = [0; 5];
    guest.read_exact(&mut heard).unwrap();
    assert_eq!(&heard, b"hello");
}

#[test]
fn listening_on_vsock_outside_a_vm_fails_rather_than_listening_elsewhere() {
    // This Mac is no guest; if it ever can listen, the port is vsock's.
    if let Ok(listening) = listen_on(0x4154) {
        let family = nix::sys::socket::getsockname::<nix::sys::socket::SockaddrStorage>(
            listening.as_raw_fd(),
        )
        .unwrap()
        .family();
        assert_eq!(family, Some(AddressFamily::Vsock));
    }
}
