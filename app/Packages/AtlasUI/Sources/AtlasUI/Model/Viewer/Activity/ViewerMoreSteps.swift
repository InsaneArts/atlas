/// "Show N more" under an open moment that holds more steps than it lists.
public struct ViewerMoreSteps: Equatable, Sendable {
  /// The moment's identity, which the press sends.
  public var moment: String
  /// How many steps more it holds.
  public var count: Int

  /// Creates the row.
  ///
  /// - Parameters:
  ///   - moment: The moment's identity.
  ///   - count: How many steps more it holds.
  public init(moment: String, count: Int) {
    self.moment = moment
    self.count = count
  }

  /// What it says: "Show 6 more".
  public var title: String { "Show \(count) more" }
}
