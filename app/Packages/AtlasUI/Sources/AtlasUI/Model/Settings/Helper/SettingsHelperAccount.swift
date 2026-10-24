/// The helper as an account, which the Helper pane opens with: its name, what it is, and its status as a pill.
public struct SettingsHelperAccount: Equatable, Sendable, Identifiable {
  /// The helper as people know it, such as "Atlas".
  public var name: String
  /// What the account is, such as "The hidden helper login · 2560 × 1440".
  public var line: String
  /// The helper's status, whose word and tone the pill shows.
  public var status: StatusSummary

  /// Creates the account.
  ///
  /// - Parameters:
  ///   - name: The helper as people know it.
  ///   - line: What the account is.
  ///   - status: The helper's status.
  public init(name: String, line: String, status: StatusSummary) {
    self.name = name
    self.line = line
    self.status = status
  }

  /// The pane holds one account.
  public var id: String { name }
}
