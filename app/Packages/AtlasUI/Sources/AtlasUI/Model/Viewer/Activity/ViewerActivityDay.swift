/// The quiet heading over the moments of a day before today, such as "Yesterday"; today has none.
public struct ViewerActivityDay: Equatable, Sendable {
  /// The day's identity, such as its first moment's time in seconds since 1970.
  public var id: String
  /// Its name, such as "Yesterday" or "Mon 19 Oct".
  public var title: String

  /// Creates a day's heading.
  ///
  /// - Parameters:
  ///   - id: The day's identity.
  ///   - title: Its name.
  public init(id: String, title: String) {
    self.id = id
    self.title = title
  }
}
