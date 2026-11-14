/// Where a reader stands in the service's feed: the feed and the number of the last event it read.
struct FeedPosition: Equatable, Sendable {
  /// The feed's identity.
  let feed: String
  /// The number of the last event read.
  let after: UInt64
}
