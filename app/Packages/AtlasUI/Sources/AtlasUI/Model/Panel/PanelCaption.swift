/// The newest action while an agent works, floating over the picture's corner on glass: what it does, how
/// long ago, and whether it failed.
public struct PanelCaption: Equatable, Sendable {
  /// What the agent does, in one line, such as "Turning Show Center off in Opie".
  public var text: String
  /// How long ago, in words, such as "just now"; empty when unknown.
  public var time: String
  /// Whether the action failed, which leads it with an orange dot instead of the breathing blue one.
  public var isFailed: Bool
  /// The icon of the host session that did it, which leads the words in place of the blue dot while sessions show;
  /// nil for none.
  public var icon: RowIcon?

  /// Creates the caption.
  ///
  /// - Parameters:
  ///   - text: What the agent does, in one line.
  ///   - time: How long ago, in words; empty when unknown.
  ///   - isFailed: Whether the action failed; not by default.
  ///   - icon: The icon of the host session that did it; none by default.
  public init(text: String, time: String, isFailed: Bool = false, icon: RowIcon? = nil) {
    self.text = text
    self.time = time
    self.isFailed = isFailed
    self.icon = icon
  }
}
