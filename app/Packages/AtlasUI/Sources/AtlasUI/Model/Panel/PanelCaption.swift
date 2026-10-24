/// The newest action while an agent works, floating over the picture's corner on glass (v3 P8): what it does and
/// how long ago.
public struct PanelCaption: Equatable, Sendable {
  /// What the agent does, in one line, such as "Turning Show Center off in Opie".
  public var text: String
  /// How long ago, in words, such as "just now"; empty when unknown.
  public var time: String

  /// Creates the caption.
  ///
  /// - Parameters:
  ///   - text: What the agent does, in one line.
  ///   - time: How long ago, in words; empty when unknown.
  public init(text: String, time: String) {
    self.text = text
    self.time = time
  }
}
