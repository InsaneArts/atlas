/// One line of an action's details beside its large picture: what it names, and what Atlas knows of it.
public struct ViewerActionDetail: Equatable, Sendable, Identifiable {
  /// What the line names, such as "What" or "Outcome".
  public var title: String
  /// What Atlas knows, such as "Pressed Return".
  public var value: String
  /// How the value reads: plain, or the outcome's colour.
  public var tone: ViewerMarkTone?

  /// Creates a line.
  ///
  /// - Parameters:
  ///   - title: What the line names.
  ///   - value: What Atlas knows.
  ///   - tone: The outcome's colour, for the outcome's line; none by default.
  public init(title: String, value: String, tone: ViewerMarkTone? = nil) {
    self.title = title
    self.value = value
    self.tone = tone
  }

  /// Each line is named once.
  public var id: String { title }
}
