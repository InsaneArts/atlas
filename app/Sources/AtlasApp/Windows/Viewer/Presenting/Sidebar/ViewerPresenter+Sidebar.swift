import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The most icons All sessions stacks.
  static let mostStackedIcons = 3
  /// 20: the recent ended sessions the sidebar lists, plus the chosen session if it is older.
  static let mostEnded = 20

  /// The sidebar: hidden while the person hid it, else beside the card, until the window's width hides it
  /// (``ViewerState/fitted(_:)``); shown over the card while the toggle or Find showed it there; its search; the
  /// sessions; the Library; and the helper's health.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The sidebar.
  static func sidebar(_ snapshot: ViewerSnapshot) -> ViewerSidebarState {
    ViewerSidebarState(
      mode: snapshot.session.sidebarHidden ? .hidden : .full,
      overlay: snapshot.session.overlayShown,
      search: snapshot.session.search,
      sessions: sessions(snapshot),
      library: library(snapshot),
      helper: helperLine(snapshot)
    )
  }

  /// The sidebar's sessions, always shown, however many are live: whose work shows, the first live sessions' icons
  /// stacked on All sessions with how many are live, then each live session by what it does, in the service's order:
  /// those that need the person under Needs you, first and only while one does, those acting or waiting their turn
  /// under Working, and the rest under Idle; the status's sessions first, then those it had no room to list, from the
  /// last read of the list. Then the sessions that list says ended, newest first, the recent ``mostEnded`` plus any
  /// older chosen session, counted under
  /// Ended and listed only while it is open, but for the chosen one, which stays listed under the folded heading. An
  /// open search keeps only the sessions whose name or line holds its query, and a section left with none folds away.
  /// An older service that offers no list gives the status's sessions alone and no Ended.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The sessions.
  static func sessions(_ snapshot: ViewerSnapshot) -> ViewerSessionsState {
    let reading = snapshot.reading.statusReading
    let chosen = snapshot.shownSessions
    let listed = reading.liveSessions
    let listedKeys = Set(listed.map(\.key))
    let rest = (snapshot.panes.sessions.list?.live ?? [])
      .filter { !listedKeys.contains($0.key) && $0.isLive && $0.state != .ended }
    let live = listed + rest
    let liveKeys = Set(live.map(\.key))
    let history = (snapshot.panes.sessions.list?.sessions ?? [])
      .filter { $0.isEnded && !liveKeys.contains($0.key) }
    let ended = history.enumerated()
      .filter { $0.offset < mostEnded || chosen == .one($0.element.key) }
      .map(\.element)
    let icons =
      live
      .prefix(mostStackedIcons)
      .map { $0.host.icon }
    let query = snapshot.session.search.query
    var groups = [ViewerSessionSection.needsYou, .working, .idle]
      .map { section in
        let rows =
          live
          .filter { self.section(of: $0.state) == section }
          .map { sessionRow($0, snapshot) }
          .filter { matches($0, query: query) }
        return ViewerSessionGroup(section, rows: rows)
      }
      .filter { !$0.rows.isEmpty }
    let endedRows = ended.map { endedRow($0, snapshot) }
      .filter { matches($0, query: query) }
    let endedShown = snapshot.session.endedShown
    if !endedRows.isEmpty {
      let drawn = endedShown ? endedRows : endedRows.filter { chosen == .one($0.id) }
      groups.append(ViewerSessionGroup(.ended, rows: drawn))
    }
    return ViewerSessionsState(
      chosen: chosen,
      icons: icons,
      liveCount: live.count,
      groups: groups,
      endedCount: endedRows.count,
      endedShown: endedShown
    )
  }

  /// The section a live session is listed under, by its state.
  ///
  /// - Parameter state: The session's state.
  /// - Returns: Needs you, Working while it acts or waits, else Idle.
  static func section(of state: HostSessionState) -> ViewerSessionSection {
    switch state {
    case .needsYou: .needsYou
    case .acting, .waiting: .working
    case .idle, .ended: .idle
    }
  }

  /// The badge on a session's icon, by its state: acting, waiting its turn, or needing the person; none while idle.
  ///
  /// - Parameter state: The session's state.
  /// - Returns: The badge, or nil.
  static func badge(of state: HostSessionState) -> SessionBadge? {
    switch state {
    case .acting: .acting
    case .waiting: .waiting
    case .needsYou: .needsYou
    case .idle, .ended: nil
    }
  }

  /// A live session's row: its icon and badge, its name, what it last did, its tooltip and its menu; while its work
  /// is cleaned up, "Cleaning up…" with a spinner in the badge's place.
  ///
  /// - Parameters:
  ///   - session: The session, as the status or the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The row.
  private static func sessionRow(_ session: ViewerStatus.HostSession, _ snapshot: ViewerSnapshot) -> ViewerSessionRow {
    let line = ViewerWords.sessionRowLine(
      session,
      line: snapshot.reading.statusReading.line(of: session),
      now: snapshot.date
    )
    let record = snapshot.panes.sessions.record(session.key)
    return row(
      session.key,
      icon: session.host.icon,
      name: session.name,
      line: line,
      badge: badge(of: session.state),
      facts: sessionFacts(session, record: record, snapshot),
      snapshot
    )
  }

  /// An ended session's row: its icon, its name and when it ended, with no badge, its tooltip and its menu.
  ///
  /// - Parameters:
  ///   - record: The session, as the list has it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The row.
  private static func endedRow(_ record: HostSessionRecord, _ snapshot: ViewerSnapshot) -> ViewerSessionRow {
    row(
      record.key,
      icon: record.host.icon,
      name: record.name,
      line: ViewerWords.endedSubtitle(record, now: snapshot.date),
      badge: nil,
      facts: sessionFacts(nil, record: record, snapshot),
      snapshot
    )
  }

  /// A session's row from what it shows: while its work is cleaned up, "Cleaning up…" in place of its line and a
  /// spinner in place of its badge; its tooltip with every fact and what its last cleanup did; and its menu.
  ///
  /// - Parameters:
  ///   - key: The session's key.
  ///   - icon: The host's icon.
  ///   - name: The session's name.
  ///   - line: What it last did.
  ///   - badge: What it does now.
  ///   - facts: Every fact about it.
  ///   - snapshot: What the controller holds.
  /// - Returns: The row.
  private static func row(
    _ key: String,
    icon: RowIcon,
    name: String,
    line: String,
    badge: SessionBadge?,
    facts: [Fact],
    _ snapshot: ViewerSnapshot
  ) -> ViewerSessionRow {
    // Terminal's session has no key a cleanup can name.
    let cleaning = !key.isEmpty && snapshot.session.cleanup.isBusy(for: key)
    let shownLine = cleaning ? ViewerWords.cleaningUpLine : line
    let receipt =
      snapshot.session.cleanup.receipts[key]
      .map(cleanupReceipt) ?? []
    return ViewerSessionRow(
      id: key,
      icon: icon,
      name: name,
      line: shownLine,
      badge: cleaning ? nil : badge,
      isCleaningUp: cleaning,
      help: sessionHelp(name: name, line: shownLine, facts: facts, receipt: receipt),
      menu: sessionMenu(key: key, needsYou: badge == .needsYou, facts: facts, snapshot)
    )
  }

  /// Whether a row is kept by the search: every row while there is no query, else one whose name or line holds it,
  /// ignoring case.
  ///
  /// - Parameters:
  ///   - row: The row.
  ///   - query: The query.
  /// - Returns: Whether it is kept.
  static func matches(_ row: ViewerSessionRow, query: String) -> Bool {
    let query = query.trimmingCharacters(in: .whitespaces)
    guard !query.isEmpty else { return true }
    return row.name.localizedCaseInsensitiveContains(query) || row.line.localizedCaseInsensitiveContains(query)
  }
}
