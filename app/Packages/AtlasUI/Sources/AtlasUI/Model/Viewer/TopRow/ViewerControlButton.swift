/// The top row's white capsule: Take Control, Connecting… while asked, Give Back while in control, or Recover… while
/// the helper needs the person.
public struct ViewerControlButton: Equatable, Sendable {
  /// The button's title.
  public var title: String
  /// Whether it can be pressed: an action that exists and waits for something.
  public var isEnabled: Bool
  /// Its tooltip, with its shortcut.
  public var help: String
  /// What pressing it sends.
  public var event: ViewerEvent
  /// Every title the capsule can show, so it keeps the widest one's width whichever it shows; none keeps the title's
  /// own.
  public var titles: [String]

  /// Creates the control capsule.
  ///
  /// - Parameters:
  ///   - title: The button's title.
  ///   - isEnabled: Whether it can be pressed.
  ///   - help: Its tooltip, with its shortcut.
  ///   - event: What pressing it sends.
  ///   - titles: Every title the capsule can show; none by default.
  public init(title: String, isEnabled: Bool, help: String, event: ViewerEvent, titles: [String] = []) {
    self.title = title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
    self.titles = titles
  }
}
