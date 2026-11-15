import Darwin
import Foundation

/// A client's end of a Unix socket, for tests.
enum Sockets {
  /// Connect to the socket at `path`; none when it refuses.
  static func connect(to path: URL) -> Int32? {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.path(percentEncoded: false).utf8)
    withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: bytes) }
    let made = socket(AF_UNIX, SOCK_STREAM, 0)
    let connected = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { generic in
        Darwin.connect(made, generic, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard connected == 0 else {
      close(made)
      return nil
    }
    return made
  }

  /// A connected pair.
  static func pair() -> (Int32, Int32) {
    var ends: [Int32] = [0, 0]
    _ = socketpair(AF_UNIX, SOCK_STREAM, 0, &ends)
    return (ends[0], ends[1])
  }

  /// Write all of `text` to `descriptor`.
  static func say(_ text: String, to descriptor: Int32) {
    let bytes = Array(text.utf8)
    _ = bytes.withUnsafeBytes { Darwin.write(descriptor, $0.baseAddress, $0.count) }
  }

  /// Everything `descriptor` says until it ends.
  static func heard(from descriptor: Int32) -> String {
    var all = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    while true {
      let read = buffer.withUnsafeMutableBytes { Darwin.read(descriptor, $0.baseAddress, $0.count) }
      guard read > 0 else { break }
      all.append(contentsOf: buffer.prefix(read))
    }
    return String(decoding: all, as: UTF8.self)
  }
}
