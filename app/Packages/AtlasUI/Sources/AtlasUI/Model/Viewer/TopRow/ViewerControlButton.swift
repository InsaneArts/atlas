/// The top row's white capsule: Take Control, Connecting… while asked, Give Back while in control, or Recover… while
/// the helper needs the person.
public struct ViewerControlButton: Equatable, Sendable {
  /// The button's title.
  public var title: String
  /// Its title in the narrowest window, such as "Take" for Take Control.
  public var shortTitle: String
  /// Whether it can be pressed.
  public var isEnabled: Bool
  /// Its tooltip, with its shortcut.
  public var help: String
  /// What pressing it sends.
  public var event: ViewerEvent

  /// Creates the control capsule.
  ///
  /// - Parameters:
  ///   - title: The button's title.
  ///   - shortTitle: Its title in the narrowest window; the title itself by default.
  ///   - isEnabled: Whether it can be pressed.
  ///   - help: Its tooltip, with its shortcut.
  ///   - event: What pressing it sends.
  public init(title: String, shortTitle: String? = nil, isEnabled: Bool, help: String, event: ViewerEvent) {
    self.title = title
    self.shortTitle = shortTitle ?? title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
  }
}
