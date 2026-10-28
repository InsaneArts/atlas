import Foundation

/// A cleanup whose apply got no reply, as when it ran past the client's wait, which may still run in the service:
/// Clean Up This Session's Work… waits until `cleanup.json` shows no step under way, or
/// ``ViewerController/cleanupWaitLimit`` has passed since it was sent.
struct ViewerUnansweredCleanup: Equatable {
  /// The session's key.
  var session: String
  /// When its apply was sent, on the window's wall clock.
  var sentAt: Date
}
