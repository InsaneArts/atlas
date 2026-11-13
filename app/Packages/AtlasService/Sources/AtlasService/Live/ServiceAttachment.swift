import Darwin
import Foundation
import Security

/// One attachment to the Atlas service over its private socket. The service's process is checked before anything is
/// read: it must be this account's, run the `atlas` beside this app in its bundle, and its running code satisfy
/// Atlas's own signature. Only then are its lines read: the viewer's address, this app's secret, the service's start,
/// then what the service passes on.
public final class ServiceAttachment: Sendable {
  /// The socket's file descriptor, closed with this attachment.
  private let socket: Int32

  /// The service's capability URL.
  public let url: String
  /// The secret the app names itself by.
  public let secret: String
  /// The service start this attachment reaches.
  public let incarnation: String
  /// The settings file the service serves; empty when it named none.
  public let settings: String

  /// The version of what the service serves that this app speaks.
  static let protocolVersion = "1"
  /// The longest the service may take to say where it is, in seconds: one that says nothing for longer is stuck.
  static let handshakeSeconds = 5

  /// Where the service listens, in the person's Atlas folder.
  public static var path: URL {
    FileManager.default.homeDirectoryForCurrentUser
      .appending(path: "Library/Application Support/Atlas/attach.sock")
  }

  /// Attaches to the service listening at `path`, checking it is `service`, the executable beside this app.
  ///
  /// - Parameters:
  ///   - path: The socket.
  ///   - service: The `atlas` executable the service must run.
  /// - Throws: ``ServiceError`` when nothing listens, the service is not Atlas's, or it says nothing usable.
  public init(path: URL = ServiceAttachment.path, service: URL) throws(ServiceError) {
    let socket = try Self.connect(to: path)
    do throws(ServiceError) {
      try Self.check(peer: socket, runs: service)
      try Self.wait(on: socket, seconds: Self.handshakeSeconds)
      var reader = LineReader(socket: socket)
      guard let url = reader.next(), let secretLine = reader.next(), let start = reader.next(),
        let secret = ViewerClient.appSecret(fromLine: secretLine), start.hasPrefix("incarnation ")
      else { throw ServiceError("Atlas's service did not say where it is.") }
      // A service that names no version, or another, is not one this app speaks to.
      guard reader.next() == "protocol \(Self.protocolVersion)" else {
        throw ServiceError("Atlas in the menu bar is from another version. Restart Atlas.", code: "protocol")
      }
      guard let settingsLine = reader.next(), settingsLine.hasPrefix("settings ") else {
        throw ServiceError("Atlas's service did not say where it is.")
      }
      self.socket = socket
      self.url = url
      self.secret = secret
      self.incarnation = String(start.dropFirst("incarnation ".count))
      self.settings = String(settingsLine.dropFirst("settings ".count))
      // What the service passes on comes when it comes.
      try Self.wait(on: socket, seconds: 0)
    } catch {
      close(socket)
      throw error
    }
  }

  deinit {
    close(socket)
  }

  /// Reads the lines the service passes on, one at a time, until it closes.
  ///
  /// - Returns: The next line, or nil once the service closed the attachment.
  public func nextLine() -> String? {
    var reader = LineReader(socket: socket)
    return reader.next()
  }

  /// Tells the service the last line was taken up, so it stops keeping it for an app that comes after this one.
  public func acknowledge() {
    var byte = UInt8(ascii: "\n")
    _ = write(socket, &byte, 1)
  }

  /// Makes each read of `socket` wait at most `seconds`; 0 waits for as long as it takes.
  ///
  /// - Parameters:
  ///   - socket: The socket.
  ///   - seconds: The longest wait.
  /// - Throws: ``ServiceError`` when the socket refuses it.
  private static func wait(on socket: Int32, seconds: Int) throws(ServiceError) {
    var limit = timeval(tv_sec: seconds, tv_usec: 0)
    guard setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, &limit, socklen_t(MemoryLayout<timeval>.size)) == 0 else {
      throw ServiceError("Atlas could not open a socket.")
    }
  }

  /// A connected socket at `path`.
  ///
  /// - Parameter path: The socket.
  /// - Returns: Its file descriptor.
  /// - Throws: ``ServiceError`` when nothing listens there.
  private static func connect(to path: URL) throws(ServiceError) -> Int32 {
    let socket = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard socket >= 0 else { throw ServiceError("Atlas could not open a socket.") }
    // A write to a service that went away fails; it never ends the app.
    var on: Int32 = 1
    _ = setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.path.utf8)
    let room = MemoryLayout.size(ofValue: address.sun_path)
    guard bytes.count < room else {
      close(socket)
      throw ServiceError("Atlas's service socket path is too long.")
    }
    withUnsafeMutableBytes(of: &address.sun_path) { buffer in
      buffer.copyBytes(from: bytes)
      buffer[bytes.count] = 0
    }
    let connected = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { generic in
        Darwin.connect(socket, generic, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard connected == 0 else {
      close(socket)
      throw ServiceError("Atlas isn't running.", code: "not-running")
    }
    return socket
  }

  /// Refuses a peer that is not this account's, does not run `service`, or whose running code is not Atlas's.
  ///
  /// - Parameters:
  ///   - socket: The connected socket.
  ///   - service: The executable the peer must run.
  /// - Throws: ``ServiceError`` naming why.
  private static func check(peer socket: Int32, runs service: URL) throws(ServiceError) {
    var uid = uid_t()
    var gid = gid_t()
    guard getpeereid(socket, &uid, &gid) == 0, uid == getuid() else {
      throw ServiceError("Atlas's service runs as another account.")
    }
    guard Signing.peer(of: socket, runs: service) else {
      throw ServiceError("The process at Atlas's service socket is not Atlas.")
    }
  }

  /// Lines read from a socket, one byte at a time: the service writes few and short ones.
  private struct LineReader {
    /// The socket.
    let socket: Int32

    /// The next line, without its newline; nil once the socket closed.
    ///
    /// - Returns: The line.
    mutating func next() -> String? {
      var bytes: [UInt8] = []
      var byte: UInt8 = 0
      while read(socket, &byte, 1) == 1 {
        if byte == UInt8(ascii: "\n") { return String(decoding: bytes, as: UTF8.self) }
        bytes.append(byte)
        if bytes.count > 4096 { return nil }
      }
      return nil
    }
  }
}
