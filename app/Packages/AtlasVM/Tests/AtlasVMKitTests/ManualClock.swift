import Foundation
import os

/// A clock a test moves on by hand, read from any thread.
final class ManualClock: Sendable {
  private let instant = OSAllocatedUnfairLock(initialState: ContinuousClock.now)
  private let date = OSAllocatedUnfairLock(initialState: Date(timeIntervalSince1970: 1_800_000_000))

  /// The instant now.
  var now: ContinuousClock.Instant { instant.withLock { $0 } }

  /// The date now.
  var today: Date { date.withLock { $0 } }

  /// Move it on by `duration`.
  func advance(by duration: Duration) {
    instant.withLock { $0 += duration }
    let seconds = Double(duration.components.seconds) + Double(duration.components.attoseconds) / 1e18
    date.withLock { $0 += seconds }
  }
}
