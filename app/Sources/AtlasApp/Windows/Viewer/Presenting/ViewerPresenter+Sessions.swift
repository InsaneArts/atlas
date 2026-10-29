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
    let choice = snapshot.shownSessions
    guard reading.showsSessions || choice != .all else { return nil }
    let icons: [RowIcon]
    let title: String
    switch choice {
    case .all:
      icons = reading.liveSessions
        .prefix(mostButtonIcons)
        .map { $0.host.icon }
      title = ViewerWords.allSessionsButton
    case .one(let key):
      let chosen = resolvedSession(key, snapshot)
      icons = [chosen.icon]
        .compactMap { $0 }
      title = chosen.name
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
      .filter { $0.isEnded && !liveKeys.contains($0.key) }
      .prefix(ViewerSessionsMenu.mostEnded)
    let liveItems = live.map { session in
      ViewerSessionsMenuItem(
        key: session.key,
        title: session.name,
        subtitle: ViewerWords.sessionSubtitle(session, line: reading.line(of: session), now: snapshot.date),
        icon: session.host.icon
      )
    }
    return ViewerSessionsMenu(
      chosen: snapshot.shownSessions,
      live: liveItems,
      ended: ended.map { record in
        ViewerSessionsMenuItem(
          key: record.key,
          title: record.name,
          subtitle: ViewerWords.endedSubtitle(record, now: snapshot.date),
          icon: record.host.icon
        )
      }
    )
  }

  /// One session as every surface names it, looked up once: the status first, then the sessions' list's live
  /// sessions, its records, and last the log's own names, so the menu, Activity, Details, Screenshots and the Screen's
  /// chip agree on its name, icon and state.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: The session as resolved.
  static func resolvedSession(_ key: String, _ snapshot: ViewerSnapshot) -> ViewerResolvedSession {
    let listed = snapshot.reading.statusReading.session(key)
    let listLive = snapshot.panes.sessions.live(key)
    let record = snapshot.panes.sessions.record(key)
    let logged = snapshot.panes.activity.log?.sessions[key]
    let live = [listed, listLive]
      .compactMap { $0 }
      .first { $0.isLive && $0.state != .ended }
    let ended = listed?.state == .ended || record?.isEnded == true
    let named = listed != nil || listLive != nil || record != nil || logged != nil
    let state: HostSessionState? =
      if let live { live.state } else if ended { .ended } else if named || key.isEmpty { .idle } else { nil }
    return ViewerResolvedSession(
      key: key,
      name: listed?.name ?? listLive?.name ?? record?.name ?? logged?.name
        ?? (key.isEmpty ? ViewerWords.terminalSession : ViewerWords.unnamedSession),
      host: listed?.host ?? listLive?.host ?? record?.host ?? logged?.host ?? (key.isEmpty ? .terminal : nil),
      state: state,
      listed: listed,
      live: live,
      record: record
    )
  }

  /// A resolved session's state and line, as Details heads it: a live one's own, an ended one's age, else Idle with
  /// how long ago it was last seen.
  ///
  /// - Parameters:
  ///   - session: The session, resolved.
  ///   - snapshot: What the controller holds.
  /// - Returns: "Acting · has the helper", "Ended 20 min ago" or "Idle · 4 min ago", say.
  static func sessionWords(_ session: ViewerResolvedSession, _ snapshot: ViewerSnapshot) -> String {
    if let live = session.live {
      return ViewerWords.sessionSubtitle(live, line: snapshot.reading.statusReading.line(of: live), now: snapshot.date)
    }
    if session.state == .ended, let record = session.record {
      return ViewerWords.endedSubtitle(record, now: snapshot.date)
    }
    let seen = session.record?.lastSeenAt ?? session.listed?.lastSeenAt
    return ViewerWords.idle(seen.map { snapshot.date.timeIntervalSince($0) })
  }
}
