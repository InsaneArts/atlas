import Foundation

/// The time a test moves by hand, in seconds of uptime, read by the viewer as its clock; no test waits on the wall
/// clock for a picture to age.
@MainActor
final class TestClock {
  /// The time now.
  var now: TimeInterval = 0

  /// The clock as the viewer takes it.
  var read: @MainActor () -> TimeInterval { { self.now } }
}
