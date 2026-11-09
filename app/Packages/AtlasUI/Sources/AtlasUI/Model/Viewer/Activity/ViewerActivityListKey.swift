/// Which list Activity shows: whose work, under which filter, and whether it is that work's own reading yet. A new key
/// is another list, which takes the old one's place as a whole; within one key, rows come and go as the work does.
public struct ViewerActivityListKey: Hashable, Sendable {
  /// Whose work the list shows.
  public var session: ViewerSessionChoice
  /// The filter chosen.
  public var filter: ViewerActivityFilter
  /// Whether the rows are a reading of whose work the list shows, so the reading's arrival swaps the list too.
  public var settled: Bool

  /// Creates the key.
  ///
  /// - Parameters:
  ///   - session: Whose work the list shows.
  ///   - filter: The filter chosen.
  ///   - settled: Whether the rows are a reading of whose work the list shows; they are by default.
  public init(session: ViewerSessionChoice, filter: ViewerActivityFilter, settled: Bool = true) {
    self.session = session
    self.filter = filter
    self.settled = settled
  }
}
