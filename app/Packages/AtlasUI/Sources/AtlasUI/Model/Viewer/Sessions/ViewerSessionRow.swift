/// One host session in the main window's sessions column: its key, its host's icon, its name, and its state and line
/// in its tone.
public struct ViewerSessionRow: Equatable, Sendable, Identifiable {
  /// The session's key, which holds as its state changes, so a row keeps its place; empty for Terminal.
  public var id: String
  /// The host's icon.
  public var icon: RowIcon
  /// The session's name, such as "Claude Code · understudy".
  public var name: String
  /// Its state and line, such as "Acting · has the helper", "Idle · 4 min ago" or "Ended 20 min ago".
  public var state: String
  /// The state's tone: active while it acts, attention while it needs the person, else quiet.
  public var tone: StatusTone

  /// Creates a session's row.
  ///
  /// - Parameters:
  ///   - id: The session's key.
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  ///   - state: Its state and line.
  ///   - tone: The state's tone; quiet by default.
  public init(id: String, icon: RowIcon, name: String, state: String, tone: StatusTone = .quiet) {
    self.id = id
    self.icon = icon
    self.name = name
    self.state = state
    self.tone = tone
  }
}
