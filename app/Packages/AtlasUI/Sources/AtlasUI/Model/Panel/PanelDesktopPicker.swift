/// The popup beside ⋯ in the panel's header while more than one desktop is set up: the title of the desktop the panel
/// shows, and the desktops its menu lists, each with its dot and state, the one shown checked.
public struct PanelDesktopPicker: Equatable, Sendable {
  /// The title of the desktop shown, such as "work".
  public var title: String
  /// The desktops its menu lists, in the settings' order.
  public var items: [DesktopItem]
  /// Whether its menu is open, which draws the popup pressed.
  public var isOpen: Bool

  /// Creates the popup.
  ///
  /// - Parameters:
  ///   - title: The title of the desktop shown.
  ///   - items: The desktops its menu lists.
  ///   - isOpen: Whether its menu is open; no by default.
  public init(title: String, items: [DesktopItem], isOpen: Bool = false) {
    self.title = title
    self.items = items
    self.isOpen = isOpen
  }
}
