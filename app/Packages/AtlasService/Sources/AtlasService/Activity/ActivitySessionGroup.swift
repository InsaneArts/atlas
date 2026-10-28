/// The lines the Activity pane lists under one host session while it shows every session: the session's newest lines,
/// and how many it holds in all.
public struct ActivitySessionGroup: Equatable, Sendable, Identifiable {
  /// The session's key: empty for Terminal, and nil for the person's own rows, listed as You.
  public var key: String?
  /// Its newest lines, newest first.
  public var runs: [ActivityRun]
  /// How many lines the filter and the search keep for it, the shown ones included.
  public var total: Int

  /// The group's identity: its session's key, which no other group shares.
  public var id: String? { key }

  /// Creates a group.
  ///
  /// - Parameters:
  ///   - key: The session's key; nil for the person's own rows.
  ///   - runs: Its newest lines, newest first.
  ///   - total: How many lines it holds in all.
  init(key: String?, runs: [ActivityRun], total: Int) {
    self.key = key
    self.runs = runs
    self.total = total
  }
}
