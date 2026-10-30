/// One host session's row in the sidebar: its key, its host's icon with the badge saying what it does now, its name,
/// what it last did, every fact about it in its tooltip, and what its context menu offers.
public struct ViewerSessionRow: Equatable, Sendable, Identifiable {
  /// The session's key, which holds as its state changes, so a row keeps its identity as it moves from section to
  /// section; empty for Terminal.
  public var id: String
  /// The host's icon.
  public var icon: RowIcon
  /// The session's name, such as "Claude Code · understudy".
  public var name: String
  /// What it last did, such as "Clicking Show Center in Opie", "Waiting for the helper · 8 s", "Opened Opie · 4 min
  /// ago", "Ended 20 min ago", or "Cleaning up…" while its work is cleaned up.
  public var line: String
  /// The badge on its icon's corner: acting, waiting its turn or needing the person; nil for none, as while idle.
  public var badge: SessionBadge?
  /// Whether its work is being cleaned up now, which a spinner on its icon's corner shows in place of the badge.
  public var isCleaningUp: Bool
  /// Its tooltip: its name and line, then one fact a line, such as "Folder: understudy", and what its last cleanup
  /// did.
  public var help: String
  /// Its context menu's sections; none offers no menu.
  public var menu: [ViewerMenuSection]

  /// Creates a session's row.
  ///
  /// - Parameters:
  ///   - id: The session's key.
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  ///   - line: What it last did.
  ///   - badge: The badge on its icon; none by default.
  ///   - isCleaningUp: Whether its work is being cleaned up now; not by default.
  ///   - help: Its tooltip; its name and line by default.
  ///   - menu: Its context menu's sections; none by default.
  public init(
    id: String,
    icon: RowIcon,
    name: String,
    line: String,
    badge: SessionBadge? = nil,
    isCleaningUp: Bool = false,
    help: String? = nil,
    menu: [ViewerMenuSection] = []
  ) {
    self.id = id
    self.icon = icon
    self.name = name
    self.line = line
    self.badge = badge
    self.isCleaningUp = isCleaningUp
    self.help = help ?? (line.isEmpty ? name : "\(name)\n\(line)")
    self.menu = menu
  }
}
