/// The row that ends a long day of the Activity outline, such as "46 more sessions on Wednesday", which shows them.
public struct ViewerMoreSessions: Equatable, Sendable {
  /// The day's identity, which the press sends.
  public var day: String
  /// What it says.
  public var title: String

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - day: The day's identity.
  ///   - title: What it says.
  public init(day: String, title: String) {
    self.day = day
    self.title = title
  }
}
