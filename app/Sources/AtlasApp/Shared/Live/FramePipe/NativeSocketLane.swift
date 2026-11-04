import AtlasService
import Darwin
import Foundation

/// One connected Unix stream socket to the frame service. Its reads and writes each run on a queue of their own, off
/// the cooperative pool, one at a time; closing it ends a read under way. The descriptor closes when the lane goes.
nonisolated final class NativeSocketLane: FramePipeTransport {
  /// The socket.
  private let socket: Int32
  /// Where reads block.
  private let reading = DispatchQueue(label: "atlas.frame-pipe.read")
  /// Where writes block.
  private let writing = DispatchQueue(label: "atlas.frame-pipe.write")

  private init(socket: Int32) {
    self.socket = socket
  }

  deinit {
    Darwin.close(socket)
  }

  /// A lane connected to `path`, once the kernel says process `pid` of this account serves it.
  ///
  /// - Parameters:
  ///   - path: The socket's path.
  ///   - pid: The process that must serve it.
  /// - Returns: The lane.
  /// - Throws: ``ServiceError`` when it cannot connect or another process serves it.
  static func connect(to path: String, servedBy pid: Int32) throws -> NativeSocketLane {
    let socket = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard socket >= 0 else { throw ServiceError("The frame service could not be reached") }
    let lane = NativeSocketLane(socket: socket)
    var quiet: Int32 = 1
    setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &quiet, socklen_t(MemoryLayout<Int32>.size))
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard bytes.count < capacity else { throw ServiceError("The frame service's path is too long") }
    withUnsafeMutableBytes(of: &address.sun_path) { field in
      field.copyBytes(from: bytes)
      field[bytes.count] = 0
    }
    address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
    let connected = withUnsafePointer(to: &address) {
      $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        Darwin.connect(socket, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard connected == 0 else { throw ServiceError("The frame service could not be reached") }
    guard NativeSocketPeer.isServed(socket, by: pid, of: getuid()) else {
      throw ServiceError("Another process answers for the frame service")
    }
    return lane
  }

  func read(upTo limit: Int) async throws -> Data {
    try await withCheckedThrowingContinuation { continuation in
      reading.async { [self] in
        var buffer = Data(count: limit)
        let count = buffer.withUnsafeMutableBytes { Darwin.read(socket, $0.baseAddress, limit) }
        guard count >= 0 else {
          continuation.resume(throwing: POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO))
          return
        }
        buffer.count = count
        continuation.resume(returning: buffer)
      }
    }
  }

  func write(_ data: Data) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
      writing.async { [self] in
        var sent = 0
        while sent < data.count {
          let count = data.withUnsafeBytes {
            Darwin.write(socket, $0.baseAddress?.advanced(by: sent), data.count - sent)
          }
          guard count > 0 else {
            continuation.resume(throwing: POSIXError(POSIXErrorCode(rawValue: errno) ?? .EPIPE))
            return
          }
          sent += count
        }
        continuation.resume()
      }
    }
  }

  func close() {
    Darwin.shutdown(socket, SHUT_RDWR)
  }
}
