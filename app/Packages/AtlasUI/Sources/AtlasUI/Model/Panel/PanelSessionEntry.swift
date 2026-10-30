/// One of Sessions' rows: the host's icon, the session's name, what it does or did last, and its state at the
/// trailing edge.
public struct PanelSessionEntry: Equatable, Sendable, Identifiable {
  /// The session's key, which holds as its state changes, so a row keeps its place.
  public var id: String
  /// The host's icon.
  public var icon: RowIcon
  /// The session's name, such as "Claude Code · understudy".
  public var name: String
  /// What it does now or did last, such as "Waiting for the helper · 8 s"; empty hides the line.
  public var line: String
  /// Whether the line asks something of the person, which draws it orange.
  public var isUrgent: Bool
  /// Its state in a word, such as "Acting".
  public var state: String
  /// The state's tone: active, attention or quiet.
  public var tone: StatusTone
  /// The state's mark: a dot, or a ring while it waits.
  public var mark: StatusMark
  /// What VoiceOver reads for the row: the name, the state, then the line, such as "Claude Code · understudy, Acting,
  /// Clicking Show Center in Opie".
  public var label: String

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: The session's key.
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  ///   - line: What it does now or did last; empty hides the line.
  ///   - isUrgent: Whether the line asks something of the person; not by default.
  ///   - state: Its state in a word.
  ///   - tone: The state's tone.
  ///   - mark: The state's mark; a dot by default.
  ///   - label: What VoiceOver reads for the row.
  public init(
    id: String,
    icon: RowIcon,
    name: String,
    line: String,
    isUrgent: Bool = false,
    state: String,
    tone: StatusTone,
    mark: StatusMark = .dot,
    label: String
  ) {
    self.id = id
    self.icon = icon
    self.name = name
    self.line = line
    self.isUrgent = isUrgent
    self.state = state
    self.tone = tone
    self.mark = mark
    self.label = label
  }
}
