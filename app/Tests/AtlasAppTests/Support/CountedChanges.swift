import Observation

/// The changes a watcher reported, counted until its stream ended; observed, so a wait can follow it.
@MainActor
@Observable
final class CountedChanges {
  /// How many changes came.
  var changes = 0
  /// Whether the stream ended.
  var ended = false
}
