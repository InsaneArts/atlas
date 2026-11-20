import AtlasService

extension ViewerPresenter {
  /// The desktop a session acts on, when it is another than the one the window shows: the list names it once the
  /// service serves more than one desktop. Such a session is listed and its Activity read, while the Screen, control
  /// and recovery stay the shown desktop's.
  ///
  /// - Parameters:
  ///   - session: The session, as the status or the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: Its desktop, or nil for one on the desktop shown.
  static func elsewhere(_ session: ViewerStatus.HostSession, _ snapshot: ViewerSnapshot) -> HostSessionDesktop? {
    guard let desktop = session.desktop, let viewed = snapshot.panes.sessions.list?.viewed, desktop.id != viewed
    else { return nil }
    return desktop
  }

  /// The desktop the session `key` names acts on, when it is another than the one shown.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: Its desktop, or nil for one on the desktop shown or not listed.
  static func elsewhere(_ key: String, _ snapshot: ViewerSnapshot) -> HostSessionDesktop? {
    currentSessions(snapshot)
      .first { $0.key == key }
      .flatMap { elsewhere($0, snapshot) }
  }
}
