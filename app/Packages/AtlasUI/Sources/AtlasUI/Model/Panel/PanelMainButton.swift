/// The panel's white capsule: the one thing to do next in the helper's state, such as Open Atlas or Recover….
public struct PanelMainButton: Equatable, Sendable {
  /// Its word.
  public var title: String
  /// Whether it can be pressed now.
  public var isEnabled: Bool
  /// What it does, for its tooltip.
  public var help: String
  /// What pressing it sends.
  public var event: PanelEvent

  /// Creates the button.
  ///
  /// - Parameters:
  ///   - title: Its word.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - help: What it does, for its tooltip.
  ///   - event: What pressing it sends.
  public init(title: String, isEnabled: Bool = true, help: String, event: PanelEvent) {
    self.title = title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
  }
}
