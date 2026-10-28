/// The top row's sessions button: the icons of the live sessions and "All sessions", or the chosen session's icon and
/// name, then a chevron; pressed while its menu is open.
public struct ViewerSessionsButton: Equatable, Sendable {
  /// The icons it stacks, at most three: the live sessions', or the chosen one's alone.
  public var icons: [RowIcon]
  /// "All sessions", or the chosen session's name.
  public var title: String
  /// Whether its menu is open, which draws it pressed.
  public var isOpen: Bool
  /// Its tooltip.
  public var help: String

  /// Creates the button's state.
  ///
  /// - Parameters:
  ///   - icons: The icons it stacks.
  ///   - title: "All sessions", or the chosen session's name.
  ///   - isOpen: Whether its menu is open; it is not by default.
  ///   - help: Its tooltip.
  public init(icons: [RowIcon], title: String, isOpen: Bool = false, help: String) {
    self.icons = icons
    self.title = title
    self.isOpen = isOpen
    self.help = help
  }
}
