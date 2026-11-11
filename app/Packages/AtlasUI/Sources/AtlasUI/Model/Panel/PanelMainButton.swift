/// The panel's main capsule: the one thing to do next in the helper's state, such as Open Atlas or Recover…, in the
/// text's colour, or Give Back in the accent while Atlas's window holds control.
public struct PanelMainButton: Equatable, Sendable {
  /// Its word.
  public var title: String
  /// Whether it can be pressed now.
  public var isEnabled: Bool
  /// What it does, for its tooltip.
  public var help: String
  /// What pressing it sends.
  public var event: PanelEvent
  /// Whether it is filled with the accent, its label white, as the main window's Give Back is.
  public var isAccent: Bool

  /// Creates the button.
  ///
  /// - Parameters:
  ///   - title: Its word.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - help: What it does, for its tooltip.
  ///   - event: What pressing it sends.
  ///   - isAccent: Whether it is filled with the accent; not by default.
  public init(title: String, isEnabled: Bool = true, help: String, event: PanelEvent, isAccent: Bool = false) {
    self.title = title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
    self.isAccent = isAccent
  }
}
