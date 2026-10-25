/// One of Recent's rows: what was done and how long ago, with no mark unless it carries state: a warning sign
/// for what did not work, and a blue dot, cooling away, on the newest row while an action runs.
public struct PanelRecentEntry: Equatable, Sendable, Identifiable {
  /// The action's id from the service, which holds as newer actions arrive, so a row keeps its place and a new one
  /// pushes in above it.
  public var id: String
  /// Who did what, in one line, such as "Clicked Ring in Opie".
  public var line: String
  /// How long ago, in short words, such as "9 s"; empty when unknown.
  public var when: String
  /// Whether the action did not work, which a warning sign leads.
  public var isFailed: Bool
  /// Whether it is the action running now: the newest row, only while an agent works.
  public var isLive: Bool

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - id: The action's id from the service.
  ///   - line: Who did what, in one line.
  ///   - when: How long ago, in short words; empty when unknown.
  ///   - isFailed: Whether the action did not work.
  ///   - isLive: Whether it is the action running now.
  public init(id: String, line: String, when: String, isFailed: Bool = false, isLive: Bool = false) {
    self.id = id
    self.line = line
    self.when = when
    self.isFailed = isFailed
    self.isLive = isLive
  }
}
