/// How Atlas keeps pictures of agent actions, as its settings say: on, for seven days, with typed text kept whole,
/// unless the person chose otherwise.
public struct PicturesSettings: Equatable, Sendable {
  /// 7: the days pictures are kept unless the person chose otherwise.
  public static let defaultKeepDays = 7

  /// Whether pictures are kept around each agent action.
  public var isOn: Bool
  /// How many days a picture is kept.
  public var keepDays: Int
  /// The most characters of typed text kept; nil keeps it whole.
  public var typedTextLimit: Int?

  /// Creates the settings, Atlas's defaults unless said otherwise.
  ///
  /// - Parameters:
  ///   - isOn: Whether pictures are kept; on by default.
  ///   - keepDays: How many days a picture is kept; seven by default.
  ///   - typedTextLimit: The most characters of typed text kept; none by default.
  public init(isOn: Bool = true, keepDays: Int = defaultKeepDays, typedTextLimit: Int? = nil) {
    self.isOn = isOn
    self.keepDays = keepDays
    self.typedTextLimit = typedTextLimit
  }
}
