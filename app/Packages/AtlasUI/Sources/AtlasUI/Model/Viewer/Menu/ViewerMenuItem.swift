/// One item of a menu in the main window: its title, the command it sends and its shortcut, whether it can be chosen,
/// is ticked or destroys something, the sections of its submenu, or a fact it only states.
public struct ViewerMenuItem: Equatable, Sendable, Identifiable {
  /// What it says, which tells it from the other items of its menu.
  public var title: String
  /// What choosing it sends; nil for an item that opens its submenu, or states a fact.
  public var command: ViewerCommand?
  /// The shortcut the menu draws beside it, the main menu's own; nil for none.
  public var shortcut: ViewerMenuShortcut?
  /// Whether it can be chosen now.
  public var isEnabled: Bool
  /// Whether it is ticked.
  public var isOn: Bool
  /// What choosing it does: an ordinary command, or one that destroys something, drawn as such.
  public var role: Role
  /// Its submenu's sections, in order; none for an item that sends its command.
  public var submenu: [ViewerMenuSection]
  /// Whether it only states a fact, such as "Folder: understudy", drawn as a line that cannot be chosen.
  public var isInfo: Bool

  /// The item's identity in its menu: its title.
  public var id: String { title }

  /// Its submenu's items, every section's in order.
  public var children: [ViewerMenuItem] { submenu.flatMap(\.items) }

  /// Creates an item that sends its command, or opens a submenu of `children`.
  ///
  /// - Parameters:
  ///   - title: What it says.
  ///   - command: What choosing it sends; none for a submenu.
  ///   - shortcut: The shortcut drawn beside it; none by default.
  ///   - isEnabled: Whether it can be chosen now; yes by default.
  ///   - isOn: Whether it is ticked; no by default.
  ///   - role: What choosing it does; an ordinary command by default.
  ///   - children: Its submenu's items, as one section; none by default.
  public init(
    _ title: String,
    command: ViewerCommand? = nil,
    shortcut: ViewerMenuShortcut? = nil,
    isEnabled: Bool = true,
    isOn: Bool = false,
    role: Role = .normal,
    children: [ViewerMenuItem] = []
  ) {
    self.title = title
    self.command = command
    self.shortcut = shortcut
    self.isEnabled = isEnabled
    self.isOn = isOn
    self.role = role
    self.submenu = children.isEmpty ? [] : [ViewerMenuSection(children)]
    self.isInfo = false
  }

  /// Creates an item that opens a submenu of sections.
  ///
  /// - Parameters:
  ///   - title: What it says.
  ///   - isEnabled: Whether it can be opened now; yes by default.
  ///   - submenu: Its submenu's sections, in order.
  public init(_ title: String, isEnabled: Bool = true, submenu: [ViewerMenuSection]) {
    self.init(title, isEnabled: isEnabled)
    self.submenu = submenu
  }

  /// A line that only states a fact, such as "Folder: understudy".
  ///
  /// - Parameter fact: The fact, in words.
  /// - Returns: The item, which cannot be chosen.
  public static func info(_ fact: String) -> Self {
    var item = Self(fact, isEnabled: false)
    item.isInfo = true
    return item
  }
}
