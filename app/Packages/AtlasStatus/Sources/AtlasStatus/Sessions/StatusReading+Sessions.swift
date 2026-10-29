import AtlasService

/// Which host sessions the panel and the window show, decided once, so every surface names the same ones.
extension StatusReading {
  /// Whether the panel and the window speak of sessions: two or more are live, as the service decides.
  public var showsSessions: Bool {
    status.overview.sessionsShown
  }

  /// Whether the panel lists its sessions in Recent's place: one or more are live. The panel lists a lone session
  /// too, so its icon and state show, while every other surface names sessions only from two (``showsSessions``).
  public var listsSessions: Bool {
    !liveSessions.isEmpty
  }

  /// The live sessions in the service's order: needing the person, acting, waiting, idle.
  public var liveSessions: [ViewerStatus.HostSession] {
    status.overview.hostSessions.filter { $0.isLive && $0.state != .ended }
  }

  /// The session that has the helper now, while sessions show: the one the service names as the holder, else the one
  /// that says it holds the helper.
  public var holder: ViewerStatus.HostSession? {
    guard showsSessions else { return nil }
    if let key = status.overview.holder, let named = session(key) { return named }
    return liveSessions.first(where: \.holdsHelper)
  }

  /// The session whose action may not have finished, while sessions show and an outcome is unknown: the first that
  /// needs the person, in the service's order. A command killed outright leaves no row, so no session is named.
  public var causer: ViewerStatus.HostSession? {
    guard showsSessions, status.overview.pending > 0 else { return nil }
    return liveSessions.first { $0.state == .needsYou }
  }

  /// How many sessions there are past those the panel lists, at most `listed`: the rest of the live ones and those
  /// the service did not list.
  ///
  /// - Parameter listed: How many sessions a surface lists.
  /// - Returns: How many more there are.
  public func sessionsPast(_ listed: Int) -> Int {
    max(0, liveSessions.count - listed) + status.overview.moreSessions
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
