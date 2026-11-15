import Darwin
import Foundation

/// Atlas VM as `ssh`'s password helper: the password read once from the
/// socket the running Atlas VM serves it on, and printed for `ssh` alone.
enum Askpass {
  /// Print the password `socket` gives: whether it gave one.
  static func answer(from socket: URL) -> Bool {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(socket.path(percentEncoded: false).utf8)
    guard bytes.count < MemoryLayout.size(ofValue: address.sun_path) else { return false }
    withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: bytes) }
    let connection = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    defer { close(connection) }
    let connected = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        connect(connection, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard connected == 0 else { return false }
    var line = [UInt8](repeating: 0, count: 512)
    let read = line.withUnsafeMutableBytes { Darwin.read(connection, $0.baseAddress, $0.count) }
    guard read > 0 else { return false }
    FileHandle.standardOutput
      .write(Data(line.prefix(read)))
    return true
  }
}
