import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The most icons All sessions stacks.
  static let mostStackedIcons = 3
  /// 20: the most ended sessions the sessions column lists.
  static let mostEnded = 20

  /// The sessions column: while two or more host sessions are live, as the service decides, or one is chosen; none
  /// otherwise, so with one session the window is as it was. It says whose work shows, stacks the first live
  /// sessions' icons on All sessions, then lists every live session with its state and line: the status's first, in
  /// the service's order, then those the status had no room to list, from the last read of the list, in its order;
  /// then the sessions that list says ended, newest first, at most ``mostEnded``. An older service that offers no list
  /// gives the status's sessions alone and no Ended part.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The column, or nil for none.
  static func sessions(_ snapshot: ViewerSnapshot) -> ViewerSessionsState? {
    let reading = snapshot.reading.statusReading
    let chosen = snapshot.shownSessions
    guard reading.showsSessions || chosen != .all else { return nil }
    let listed = reading.liveSessions
    let listedKeys = Set(listed.map(\.key))
    let rest = (snapshot.panes.sessions.list?.live ?? [])
      .filter { !listedKeys.contains($0.key) && $0.isLive && $0.state != .ended }
    let live = listed + rest
    let liveKeys = Set(live.map(\.key))
    let ended = (snapshot.panes.sessions.list?.sessions ?? [])
      .filter { $0.isEnded && !liveKeys.contains($0.key) }
      .prefix(mostEnded)
    let icons =
      live
      .prefix(mostStackedIcons)
      .map { $0.host.icon }
    return ViewerSessionsState(
      chosen: chosen,
      icons: icons,
      live: live.map { session in
        ViewerSessionRow(
          id: session.key,
          icon: session.host.icon,
          name: session.name,
          state: ViewerWords.sessionSubtitle(session, line: reading.line(of: session), now: snapshot.date),
          tone: session.state.tone
        )
      },
      ended: ended.map { record in
        ViewerSessionRow(
          id: record.key,
          icon: record.host.icon,
          name: record.name,
          state: ViewerWords.endedSubtitle(record, now: snapshot.date)
        )
      }
    )
  }

  /// One session as every surface names it, looked up once: the status first, then the sessions' list's live
  /// sessions, its records, and last the log's own names, so the sessions column, Activity, Details, Screenshots and
  /// the Screen's chip agree on its name, icon and state.
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
