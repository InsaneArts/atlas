/// How old a picture may be, in seconds: the header and control agree on it.
package enum Freshness {
  /// Younger than this reads as live.
  package static let live: Double = 3
  /// This old, the picture is stale: the header says so and control ends.
  package static let stale: Double = 6.5
}
