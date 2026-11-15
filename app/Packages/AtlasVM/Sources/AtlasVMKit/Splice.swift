import Darwin
import Foundation

/// Two connections joined: each one's bytes written to the other as they
/// come, each way ending on its own, both closed once both ways have.
public enum Splice {
  /// Join `one` and `other`, which this takes and closes when done.
  public static func join(_ one: Int32, _ other: Int32) {
    UnixListener.quiet(one)
    UnixListener.quiet(other)
    let ways = DispatchGroup()
    for (from, to) in [(one, other), (other, one)] {
      ways.enter()
      Thread.detachNewThread {
        copy(from: from, to: to)
        // This way is done: the other end hears its end, the other way goes on.
        shutdown(to, SHUT_WR)
        ways.leave()
      }
    }
    ways.notify(queue: .global()) {
      close(one)
      close(other)
    }
  }

  /// Copy what `from` says to `to` until `from` ends or either fails.
  private static func copy(from: Int32, to: Int32) {
    var buffer = [UInt8](repeating: 0, count: 64 * 1024)
    while true {
      let read = buffer.withUnsafeMutableBytes { bytes in Darwin.read(from, bytes.baseAddress, bytes.count) }
      if read < 0, errno == EINTR { continue }
      guard read > 0 else { return }
      var sent = 0
      while sent < read {
        let wrote = buffer.withUnsafeBytes { bytes in
          Darwin.write(to, bytes.baseAddress?.advanced(by: sent), read - sent)
        }
        if wrote < 0, errno == EINTR { continue }
        guard wrote > 0 else { return }
        sent += wrote
      }
    }
  }
}
