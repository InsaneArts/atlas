import Foundation
import os

/// A clock whose sleeps never end on their own: each waits until it is
/// cancelled, its deadline kept, so a test sees exactly what a pause waits
/// for and when it starts waiting, never on the wall clock.
final class HeldClock: Clock, Sendable {
  /// A moment on it: how far past its start.
  struct Instant: InstantProtocol {
    /// How far past the clock's start.
    var offset: Duration

    func advanced(by duration: Duration) -> Instant {
      Instant(offset: offset + duration)
    }

    func duration(to other: Instant) -> Duration {
      other.offset - offset
    }

    static func < (lhs: Instant, rhs: Instant) -> Bool {
      lhs.offset < rhs.offset
    }
  }

  /// The deadline of every sleep begun, in order, and who waits for there
  /// to be so many.
  private let state = OSAllocatedUnfairLock(
    initialState: (deadlines: [Duration](), watching: [(Int, CheckedContinuation<Void, Never>)]())
  )

  /// It never moves.
  var now: Instant { Instant(offset: .zero) }

  var minimumResolution: Duration { .nanoseconds(1) }

  /// The deadlines of every sleep begun, as offsets from its start.
  var deadlines: [Duration] { state.withLock { $0.deadlines } }

  func sleep(until deadline: Instant, tolerance: Duration?) async throws {
    let ready = state.withLock { state -> [CheckedContinuation<Void, Never>] in
      state.deadlines.append(deadline.offset)
      let count = state.deadlines.count
      let ready =
        state.watching
        .filter { $0.0 <= count }
        .map(\.1)
      state.watching.removeAll { $0.0 <= count }
      return ready
    }
    for watcher in ready { watcher.resume() }
    let (held, keeping) = AsyncStream<Never>.makeStream()
    for await _ in held {}
    keeping.finish()
    throw CancellationError()
  }

  /// Returns once `count` sleeps have begun.
  func untilSleeps(_ count: Int) async {
    await withCheckedContinuation { (watching: CheckedContinuation<Void, Never>) in
      let now = state.withLock { state -> Bool in
        guard state.deadlines.count < count else { return true }
        state.watching.append((count, watching))
        return false
      }
      if now { watching.resume() }
    }
  }
}
