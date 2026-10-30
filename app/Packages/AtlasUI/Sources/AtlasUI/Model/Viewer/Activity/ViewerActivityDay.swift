/// One day of Activity, owning its moments; today has an empty heading.
public struct ViewerActivityDay: Equatable, Sendable, Identifiable {
  /// The day's identity, such as its first moment's time in seconds since 1970.
  public var id: String
  /// Its name, such as "Yesterday" or "Mon 19 Oct".
  public var title: String

  /// The moments of the day, newest first.
  public var moments: [ViewerActivityMoment]

  /// Creates a day with its moments.
  ///
  /// - Parameters:
  ///   - id: The day's identity.
  ///   - title: Its heading, or empty for today.
  ///   - moments: Its moments.
  public init(id: String, title: String, moments: [ViewerActivityMoment]) {
    self.id = id
    self.title = title
    self.moments = moments
  }
}
