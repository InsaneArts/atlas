/// The lines of the Activity pane under one host session while it shows every session's work: the session's icon,
/// name and state, its newest lines, and Show all while it holds more.
public struct ViewerActivitySessionGroup: Equatable, Sendable, Identifiable {
  /// The group's identity, which no other group in the pane shares.
  public var id: String
  /// The host's icon; nil for the person's own lines, which the person's blue dot leads.
  public var icon: RowIcon?
  /// The session's name, such as "Claude Code · understudy", or "You".
  public var name: String
  /// Its state in a word, such as "Needs you"; empty for none.
  public var state: String
  /// The state's tone.
  public var tone: StatusTone
  /// Its newest lines, newest first.
  public var lines: [ViewerActivityLine]
  /// Show all, while it holds more lines than it lists; nil otherwise.
  public var showAll: ViewerShowAll?

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - icon: The host's icon; nil for the person's own lines.
  ///   - name: The session's name.
  ///   - state: Its state in a word; empty for none.
  ///   - tone: The state's tone; quiet by default.
  ///   - lines: Its newest lines.
  ///   - showAll: Show all, while it holds more lines than it lists; none by default.
  public init(
    id: String,
    icon: RowIcon?,
    name: String,
    state: String = "",
    tone: StatusTone = .quiet,
    lines: [ViewerActivityLine],
    showAll: ViewerShowAll? = nil
  ) {
    self.id = id
    self.icon = icon
    self.name = name
    self.state = state
    self.tone = tone
    self.lines = lines
    self.showAll = showAll
  }
}
