/// The footer's main capsule: Take Control, Connecting… while asked, Give Back while in control, or Recover… while
/// the helper needs the person.
public struct ViewerControlButton: Equatable, Sendable {
  /// How the capsule is painted.
  public enum Look: Equatable, Sendable {
    /// The text's colour, the one thing to do next: Take Control, Connecting… and Recover….
    case primary
    /// The accent, while this window holds control: Give Back.
    case accent
  }

  /// The button's title.
  public var title: String
  /// The symbol before the title, such as Take Control's hand; nil draws the title alone.
  public var symbol: SymbolName?
  /// How the capsule is painted.
  public var look: Look
  /// Whether it can be pressed: an action that exists and waits for something.
  public var isEnabled: Bool
  /// Its tooltip, with its shortcut.
  public var help: String
  /// What pressing it sends.
  public var event: ViewerEvent
  /// Every label the capsule takes in turn, its own among them, such as Take Control, Connecting…, Give Back and
  /// Recover…: it keeps the widest one's width whichever it shows, so pressing it never moves it.
  public var labels: [ViewerControlLabel]

  /// The label it shows: its title and its symbol.
  public var label: ViewerControlLabel {
    ViewerControlLabel(title, symbol: symbol)
  }

  /// The labels whose widest sets the capsule's width: ``labels``, with its own label added when they leave it out.
  public var room: [ViewerControlLabel] {
    labels.contains(label) ? labels : labels + [label]
  }

  /// The full action name, matching its menu item even when the visible word is shortened.
  public var accessibilityLabel: String {
    event == .giveBack ? HelperActionNames.giveBack : title
  }

  /// Creates the main capsule.
  ///
  /// - Parameters:
  ///   - title: The button's title.
  ///   - symbol: The symbol before the title; none by default.
  ///   - look: How the capsule is painted; the text's colour by default.
  ///   - isEnabled: Whether it can be pressed.
  ///   - help: Its tooltip, with its shortcut.
  ///   - event: What pressing it sends.
  ///   - labels: Every label the capsule takes in turn; none by default, which keeps its own label's width.
  public init(
    title: String,
    symbol: SymbolName? = nil,
    look: Look = .primary,
    isEnabled: Bool,
    help: String,
    event: ViewerEvent,
    labels: [ViewerControlLabel] = []
  ) {
    self.title = title
    self.symbol = symbol
    self.look = look
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
    self.labels = labels
  }
}
