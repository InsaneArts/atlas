/// What the person allowed for a task, as the panel's row shows it while it holds: the account, the agent and the
/// time left, and End.
public struct AllowedRowState: Equatable, Sendable, Identifiable {
  /// The grant, as its session and account name it.
  public var id: String
  /// What it allows, such as "Allowed in GitHub for this task".
  public var title: String
  /// The agent and the time left, such as "Claude Code · 52 min left".
  public var detail: String
  /// What VoiceOver reads for End, such as "End what you allowed in GitHub".
  public var endLabel: String

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: The grant.
  ///   - title: What it allows.
  ///   - detail: The agent and the time left.
  ///   - endLabel: What VoiceOver reads for End.
  public init(id: String, title: String, detail: String, endLabel: String) {
    self.id = id
    self.title = title
    self.detail = detail
    self.endLabel = endLabel
  }
}
