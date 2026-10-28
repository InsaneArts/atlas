import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The most icons the sessions button stacks.
  static let mostButtonIcons = 3

  /// The sessions button: while two or more host sessions are live, as the service decides, or one is chosen; the
  /// live sessions' icons and "All sessions", or the chosen session's icon and name; pressed while its menu is open.
  /// None otherwise, so with one session the bar is as it was.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button, or nil for none.
  static func sessionsButton(_ snapshot: ViewerSnapshot) -> ViewerSessionsButton? {
    let reading = snapshot.reading.statusReading
    let choice = snapshot.sessionChoice
    guard reading.showsSessions || choice != .all else { return nil }
    let icons: [RowIcon]
    let title: String
    switch choice {
    case .all:
      icons = reading.liveSessions
        .prefix(mostButtonIcons)
        .map { SessionIcons.icon($0.host) }
      title = ViewerWords.allSessionsButton
    case .one(let key):
      icons = [sessionIcon(key, snapshot)]
        .compactMap { $0 }
      title = sessionName(key, snapshot)
    }
    return ViewerSessionsButton(
      icons: icons,
      title: title,
      isOpen: snapshot.session.openMenu == .sessions,
      help: ViewerWords.sessionsHelp
    )
  }

  /// The sessions menu: whose work shows, then every live session, each with its state and line: the status's first,
  /// in the service's order, then those the status had no room to list, from the last read of the list, in its order;
  /// then the sessions that list says ended, newest first, at most ``ViewerSessionsMenu/mostEnded``. An older service
  /// that offers no list gives the status's sessions alone and no Ended part.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  static func sessionsMenu(_ snapshot: ViewerSnapshot) -> ViewerSessionsMenu {
    let reading = snapshot.reading.statusReading
    let listed = reading.liveSessions
    let listedKeys = Set(listed.map(\.key))
    let rest = (snapshot.panes.sessions.list?.live ?? [])
      .filter { !listedKeys.contains($0.key) && $0.isLive && $0.state != .ended }
    let live = listed + rest
    let liveKeys = Set(live.map(\.key))
    let ended = (snapshot.panes.sessions.list?.sessions ?? [])
      .filter { isEnded($0) && !liveKeys.contains($0.key) }
      .prefix(ViewerSessionsMenu.mostEnded)
    let liveItems = live.map { session in
      ViewerSessionsMenuItem(
        key: session.key,
        title: session.name,
        subtitle: ViewerWords.sessionSubtitle(session, line: reading.line(of: session), now: snapshot.date),
        icon: SessionIcons.icon(session.host)
      )
    }
    return ViewerSessionsMenu(
      chosen: snapshot.sessionChoice,
      live: liveItems,
      ended: ended.map { record in
        ViewerSessionsMenuItem(
          key: record.key,
          title: record.name,
          subtitle: ViewerWords.endedSubtitle(record, now: snapshot.date),
          icon: SessionIcons.icon(record.host)
        )
      }
    )
  }

  /// Whether the list says a session ended: its state, or the time it ended.
  ///
  /// - Parameter record: The session, as the list has it.
  /// - Returns: Whether it ended.
  static func isEnded(_ record: HostSessionRecord) -> Bool {
    record.state == .ended || record.endedAt != nil
  }

  /// A session's name: as the status lists it, else the sessions' list of live and ended ones, else the log's own
  /// names; Terminal for the commands no host named.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Claude Code · understudy", say.
  static func sessionName(_ key: String, _ snapshot: ViewerSnapshot) -> String {
    if let listed = snapshot.reading.statusReading.session(key) { return listed.name }
    if let live = snapshot.panes.sessions.live(key) { return live.name }
    if let record = snapshot.panes.sessions.record(key) { return record.name }
    if let named = snapshot.panes.activity.log?.sessions[key] { return named.name }
    return key.isEmpty ? ViewerWords.terminalSession : ViewerWords.unnamedSession
  }

  /// A session's host, as the status, the sessions' list or the log names it: none for Terminal, and nil for a
  /// session nothing names.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: The host, or nil when nothing names the session.
  static func sessionHost(_ key: String, _ snapshot: ViewerSnapshot) -> SessionHost? {
    if let listed = snapshot.reading.statusReading.session(key) { return listed.host }
    if let live = snapshot.panes.sessions.live(key) { return live.host }
    if let record = snapshot.panes.sessions.record(key) { return record.host }
    if let named = snapshot.panes.activity.log?.sessions[key] { return named.host }
    return key.isEmpty ? .terminal : nil
  }

  /// A session's icon, by its host; none for a session nothing names, which is not Terminal's.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: The host's logo or mark, the `>_` tile, or nil.
  static func sessionIcon(_ key: String, _ snapshot: ViewerSnapshot) -> RowIcon? {
    sessionHost(key, snapshot)
      .map(SessionIcons.icon)
  }
}
