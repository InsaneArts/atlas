import Foundation

/// All callers waiting for one identity, retaining the newest live evidence until it has been processed.
nonisolated struct AppIconRequest {
  /// The combined evidence still to process.
  var want: AppIconWant
  /// Each independent caller's continuation, removable without cancelling shared work.
  var waiters: [UUID: CheckedContinuation<AppIconFound?, Never>] = [:]
}
