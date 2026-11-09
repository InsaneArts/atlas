import Foundation

/// All callers waiting for one identity, until its lookup ends.
nonisolated struct AppIconRequest {
  /// What is looked up.
  var want: AppIconWant
  /// Each independent caller's continuation, removable without cancelling shared work.
  var waiters: [UUID: CheckedContinuation<AppIconFound?, Never>] = [:]
}
