import Darwin
import Foundation

/// Requests on one connection, a line each, each answered with one reply
/// line before the next is read.
public enum LineServer {
  /// How long a reply may take to be written before the connection is given up.
  private static let writing = timeval(tv_sec: 10, tv_usec: 0)

  /// Serve `connection`, which this takes and closes: each line, at most
  /// `limit` bytes, answered by `answer`, until the peer ends it or sends a
  /// line too long.
  public static func serve(_ connection: Int32, limit: Int = 64 * 1024, answer: (Data) async -> Data) async {
    UnixListener.quiet(connection)
    var timeout = writing
    _ = setsockopt(connection, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
    let handle = FileHandle(fileDescriptor: connection, closeOnDealloc: true)
    var line = Data()
    do {
      for try await byte in handle.bytes {
        guard byte != UInt8(ascii: "\n") else {
          let reply = await answer(line)
          try handle.write(contentsOf: reply)
          line.removeAll(keepingCapacity: true)
          continue
        }
        line.append(byte)
        guard line.count <= limit else { return }
      }
    } catch {
      return
    }
  }
}
