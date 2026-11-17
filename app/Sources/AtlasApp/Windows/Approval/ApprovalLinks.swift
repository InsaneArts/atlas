import Foundation

/// What the approval window asks of the rest of the app, as closures its launch gives it at `init`.
struct ApprovalLinks {
  /// Closes the window, which ends the app.
  var close: @MainActor () -> Void = {}
  /// The time now, to tell how long the request has left.
  var now: @MainActor () -> Date = { Date() }
  /// The clock the window's waits run on, so a test moves its time by hand: the request's expiry, and an answer's
  /// hold before the window closes.
  var clock: any Clock<Duration>
}
