import Darwin
import Foundation

/// A socket in a private folder that only this account's processes may
/// connect to, mode 0600, every peer's account checked again as it is
/// taken.
public final class UnixListener: Sendable {
  /// Its path.
  public let path: URL
  private let descriptor: Int32

  /// Listen at `path`, in a private folder. A socket left there by an
  /// earlier run of this account's is replaced; anything else is refused.
  public init(at path: URL) throws(UnixListenerError) {
    self.path = path
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.path(percentEncoded: false).utf8)
    let room = MemoryLayout.size(ofValue: address.sun_path)
    guard bytes.count < room else { throw .pathTooLong(path) }
    withUnsafeMutableBytes(of: &address.sun_path) { sunPath in
      sunPath.copyBytes(from: bytes)
    }
    var status = stat()
    if lstat(path.path(percentEncoded: false), &status) == 0 {
      guard status.st_mode & S_IFMT == S_IFSOCK, status.st_uid == getuid() else { throw .occupied(path) }
      unlink(path.path(percentEncoded: false))
    }
    let made = socket(AF_UNIX, SOCK_STREAM, 0)
    guard made >= 0 else { throw .unavailable(path) }
    _ = fcntl(made, F_SETFD, FD_CLOEXEC)
    let bound = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { generic in
        bind(made, generic, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard bound == 0, chmod(path.path(percentEncoded: false), 0o600) == 0, listen(made, 16) == 0 else {
      close(made)
      throw .unavailable(path)
    }
    descriptor = made
  }

  deinit {
    close(descriptor)
  }

  /// Make a write to `connection` once its peer is gone fail, never signal.
  public static func quiet(_ connection: Int32) {
    var on: Int32 = 1
    _ = setsockopt(connection, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
  }

  /// The next connection from a process of this account's, waiting until
  /// one comes; none once the socket fails. Others are closed unanswered.
  public func accept() -> Int32? {
    while true {
      let connection = Darwin.accept(descriptor, nil, nil)
      guard connection >= 0 else {
        if errno == EINTR || errno == ECONNABORTED { continue }
        return nil
      }
      _ = fcntl(connection, F_SETFD, FD_CLOEXEC)
      Self.quiet(connection)
      var uid = uid_t()
      var gid = gid_t()
      guard getpeereid(connection, &uid, &gid) == 0, uid == getuid() else {
        close(connection)
        continue
      }
      return connection
    }
  }
}
