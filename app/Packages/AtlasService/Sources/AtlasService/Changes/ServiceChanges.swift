import Foundation
import os

/// What the service's feed says changed, for the readers that wait on it. Each kind counts its changes; a reader
/// notes the count before it reads and, after, waits for it to move or for its pause to end. A change that came while
/// it read is read again once ``soonest`` has passed since it read, so none is lost and a busy feed folds into one
/// read. While the feed is followed a reader's pause stretches to ``fallback``: the feed wakes it when there is
/// something to read.
public final class ServiceChanges: Sendable {
  /// The longest a reader waits while the feed is followed.
  public static let fallback: Duration = .seconds(30)
  /// The least a reader waits between two reads a change woke: the changes that come meanwhile are read together.
  public static let soonest: Duration = .seconds(1)

  /// The changes counted per kind, whether the feed is followed, and the readers' waits.
  private let state = OSAllocatedUnfairLock(initialState: State())

  /// Changes not followed yet: every reader waits its own pause.
  public init() {}

  /// The changes of a service that keeps no feed: never followed, never told.
  static let unfollowed = ServiceChanges()

  /// How many changes of `kinds` were counted: a reader notes it before it reads.
  ///
  /// - Parameter kinds: The kinds the reader shows.
  /// - Returns: The count.
  public func count(of kinds: Set<ChangeKind>) -> UInt64 {
    state.withLock { $0.count(of: kinds) }
  }

  /// Whether the feed is followed now.
  public var followed: Bool {
    state.withLock(\.followed)
  }

  /// Waits for a change of `kinds` since the count `seen`, or for `pause` (``fallback`` while the feed is followed),
  /// whichever comes first; a change, even one that came already, ends it no sooner than ``soonest`` (or `pause`,
  /// when shorter) after it began. A cancelled wait ends at once.
  ///
  /// - Parameters:
  ///   - pause: The reader's own pause, while the feed is not followed.
  ///   - kinds: The kinds the reader shows.
  ///   - seen: The count it noted before it last read.
  ///   - clock: The clock the pause runs on.
  public func pause(
    _ pause: Duration,
    on kinds: Set<ChangeKind>,
    since seen: UInt64,
    clock: any Clock<Duration>
  ) async {
    let wait = followed ? max(pause, Self.fallback) : pause
    await waiting(wait, floor: min(pause, Self.soonest), on: kinds, since: seen, clock: clock)
  }

  /// ``pause(_:on:since:clock:)`` on a clock of a known kind, so its deadlines are counted from one moment.
  ///
  /// - Parameters:
  ///   - wait: The longest it waits.
  ///   - floor: The least it waits once a change came.
  ///   - kinds: The kinds the reader shows.
  ///   - seen: The count it noted before it last read.
  ///   - clock: The clock the pause runs on.
  private func waiting<C: Clock<Duration>>(
    _ wait: Duration,
    floor: Duration,
    on kinds: Set<ChangeKind>,
    since seen: UInt64,
    clock: C
  ) async {
    let start = clock.now
    await withTaskGroup(of: Void.self) { group in
      group.addTask { try? await clock.sleep(until: start.advanced(by: wait), tolerance: nil) }
      group.addTask {
        await self.change(of: kinds, since: seen)
        try? await clock.sleep(until: start.advanced(by: floor), tolerance: nil)
      }
      await group.next()
      group.cancelAll()
    }
  }

  /// Counts a change of each of `kinds`, and wakes the readers that show any: as the feed says it, or a test.
  ///
  /// - Parameter kinds: What changed.
  public func tell(_ kinds: Set<ChangeKind>) {
    let woken = state.withLock { $0.tell(kinds) }
    for reader in woken { reader.resume() }
  }

  /// Notes whether the feed is followed, as its follower finds it, or a test; every reader waiting wakes to wait again
  /// at the right pause.
  ///
  /// - Parameter followed: Whether it is.
  public func follow(_ followed: Bool) {
    let woken = state.withLock { state -> [CheckedContinuation<Void, Never>] in
      guard state.followed != followed else { return [] }
      state.followed = followed
      return state.wakeAll()
    }
    for reader in woken { reader.resume() }
  }

  /// Returns once a change of `kinds` was counted since `seen`, or the task is cancelled.
  ///
  /// - Parameters:
  ///   - kinds: The kinds.
  ///   - seen: The count noted.
  private func change(of kinds: Set<ChangeKind>, since seen: UInt64) async {
    let id = UUID()
    state.withLock { $0.begin(id) }
    defer { state.withLock { $0.end(id) } }
    await withTaskCancellationHandler {
      await withCheckedContinuation { (waiting: CheckedContinuation<Void, Never>) in
        let now = state.withLock { $0.wait(id, for: kinds, since: seen, waiting) }
        if now { waiting.resume() }
      }
    } onCancel: {
      let waiting = state.withLock { $0.cancel(id) }
      waiting?.resume()
    }
  }
}
