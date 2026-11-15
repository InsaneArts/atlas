import Darwin
import Foundation

/// One run of a VM's runner: its process, told from any later process that
/// reuses its PID by when it started, and an ID of its own.
public struct Incarnation: Codable, Equatable, Sendable {
  /// Its process's ID.
  public let pid: Int32
  /// When its process started, in seconds past the epoch.
  public let startSeconds: Int
  /// When its process started, the microseconds past `startSeconds`.
  public let startMicroseconds: Int
  /// An ID no other run has.
  public let id: String

  /// This process's run; none when the kernel does not say when it started.
  public static func current() -> Incarnation? {
    let pid = getpid()
    guard case .running(let seconds, let microseconds) = ProcessLife.of(pid) else { return nil }
    return Incarnation(
      pid: pid,
      startSeconds: seconds,
      startMicroseconds: microseconds,
      id: UUID().uuidString.lowercased()
    )
  }

  /// Whether this run's process has exited: its PID gone, or another
  /// process's. None when the kernel does not say, which is never gone.
  public func exited() -> Bool? {
    switch ProcessLife.of(pid) {
    case .gone:
      true
    case .running(let seconds, let microseconds):
      (seconds, microseconds) != (startSeconds, startMicroseconds)
    case .unknown:
      nil
    }
  }
}
