/// Which list Activity shows: whose work and under which filter. A new key is another list, which takes the old one's
/// place as a whole; within one key, rows come and go as the work does.
public struct ViewerActivityListKey: Hashable, Sendable {
  /// Whose work the list shows.
  public var session: ViewerSessionChoice
  /// The filter chosen.
  public var filter: ViewerActivityFilter

  /// Creates the key.
  ///
  /// - Parameters:
  ///   - session: Whose work the list shows.
  ///   - filter: The filter chosen.
  public init(session: ViewerSessionChoice, filter: ViewerActivityFilter) {
    self.session = session
    self.filter = filter
  }
}
