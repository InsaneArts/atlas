import AtlasService

/// Which host sessions the panel and the window show, decided once, so every surface names the same ones.
extension StatusReading {
  /// Whether the panel and window speak of sessions: two or more remain relevant, as the service decides.
  public var showsSessions: Bool {
    status.overview.sessionsShown
  }

  /// Whether the panel lists its sessions in Recent's place: one or more remain relevant. The panel lists a lone session
  /// too, so its icon and state show, while every other surface names sessions only from two (``showsSessions``).
  public var listsSessions: Bool {
    !relevantSessions.isEmpty
  }

  /// The live sessions in the service's order: needing the person, acting, waiting, idle.
  public var liveSessions: [ViewerStatus.HostSession] {
    relevantSessions.filter(\.isLive)
  }

  /// Whether a session remains useful in the current list, even after its host ended.
  ///
  /// - Parameter session: The service's session facts.
  /// - Returns: Whether it is live, needs recovery, or has proven work.
  public static func isRelevant(_ session: ViewerStatus.HostSession) -> Bool {
    session.isLive || session.state == .needsYou || session.workInProgress
  }

  /// Current sessions in service order, including ended recovery attention or surviving work, once per key.
  public var relevantSessions: [ViewerStatus.HostSession] {
    var keys = Set<String>()
    return status.overview.hostSessions.filter { Self.isRelevant($0) && keys.insert($0.key).inserted }
  }

  /// The session that has the helper now, while sessions show: the one the service names as the holder, else the one
  /// that says it holds the helper.
  public var holder: ViewerStatus.HostSession? {
    guard showsSessions else { return nil }
    if let key = status.overview.holder, let named = session(key), named.isLive, named.holdsHelper { return named }
    return liveSessions.first(where: \.holdsHelper)
  }

  /// The session whose action may not have finished, while sessions show and an outcome is unknown: the first that
  /// needs the person, in the service's order. A command killed outright leaves no row, so no session is named.
  public var causer: ViewerStatus.HostSession? {
    showsSessions ? unresolvedSession : nil
  }

  /// The unresolved action's row, even with one session, when no session label is drawn.
  public var uncertainRow: String? {
    unresolvedSession?.uncertainRow
  }

  /// Recovery follows the unresolved action independently of how many session labels are visible.
  private var unresolvedSession: ViewerStatus.HostSession? {
    guard status.overview.pending > 0 else { return nil }
    return relevantSessions.first { $0.state == .needsYou }
  }

  /// How many sessions there are past those the panel lists, at most `listed`: the rest of the relevant ones and those
  /// the service did not list.
  ///
  /// - Parameter listed: How many sessions a surface lists.
  /// - Returns: How many more there are.
  public func sessionsPast(_ listed: Int) -> Int {
    max(0, relevantSessions.count - listed) + status.overview.moreSessions
  }

  /// A session by its key; the Terminal one for "".
  ///
  /// - Parameter key: The session's key, or nil for none.
  /// - Returns: The listed session with that key, or nil.
  public func session(_ key: String?) -> ViewerStatus.HostSession? {
    guard let key else { return nil }
    return status.overview.hostSessions.first { $0.key == key }
  }
}
