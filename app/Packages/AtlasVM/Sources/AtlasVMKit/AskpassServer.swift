import Darwin
import Foundation
import Security

/// The one answer a sign-in's password helper gets: served once, on a
/// socket in a private folder, to the exact helper alone: a process whose
/// parent is the `ssh` it serves and whose running code is this program's.
public final class AskpassServer: Sendable {
  private let listener: UnixListener
  /// A pipe whose write end, written, ends the serving.
  private let waking: (reading: Int32, writing: Int32)

  /// Serve at `path`, in a private folder.
  public init(at path: URL) throws {
    listener = try UnixListener(at: path)
    var ends: [Int32] = [0, 0]
    guard pipe(&ends) == 0 else { throw UnixListenerError.unavailable(path) }
    for end in ends { _ = fcntl(end, F_SETFD, FD_CLOEXEC) }
    waking = (ends[0], ends[1])
  }

  deinit {
    close(waking.reading)
    close(waking.writing)
  }

  /// Give `password` to the first helper of the process `ssh` that asks,
  /// and to nothing else; whether it was given. Waits until one asks, or
  /// until it is stopped.
  public func serve(_ password: String, toChildOf ssh: Int32) -> Bool {
    while true {
      var watched = [
        pollfd(fd: listener.pollable, events: Int16(POLLIN), revents: 0),
        pollfd(fd: waking.reading, events: Int16(POLLIN), revents: 0),
      ]
      guard poll(&watched, 2, -1) >= 0 else {
        if errno == EINTR { continue }
        return false
      }
      if watched[1].revents != 0 { return false }
      guard watched[0].revents & Int16(POLLIN) != 0, let connection = listener.accept() else { continue }
      defer { close(connection) }
      guard Self.parent(ofPeer: connection) == ssh, CodeIdentity.runsThisCode(connection) else { continue }
      let line = Array((password + "\n").utf8)
      let written = line.withUnsafeBytes { Darwin.write(connection, $0.baseAddress, $0.count) }
      return written == line.count
    }
  }

  /// End the serving, asked or not.
  public func stop() {
    var byte: UInt8 = 1
    _ = write(waking.writing, &byte, 1)
  }

  /// The parent of the process that connected `connection`.
  static func parent(ofPeer connection: Int32) -> Int32? {
    var pid: pid_t = 0
    var length = socklen_t(MemoryLayout<pid_t>.size)
    guard getsockopt(connection, SOL_LOCAL, LOCAL_PEERPID, &pid, &length) == 0 else { return nil }
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.stride
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]
    guard sysctl(&name, UInt32(name.count), &info, &size, nil, 0) == 0, size > 0 else { return nil }
    return info.kp_eproc.e_ppid
  }

}
