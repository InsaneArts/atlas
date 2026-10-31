import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The status's sessions, supplemented by the cached list only while the status is truncated or not yet read.
  /// A complete status omitting a key supersedes its cached work and lifetime without inferring an end.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The current entries, once per key, with the status's facts first.
  static func currentSessions(_ snapshot: ViewerSnapshot) -> [ViewerStatus.HostSession] {
    let cached = snapshot.panes.sessions.list?.live ?? []
    let overview = snapshot.reading.status.overview
    let entries =
      if !snapshot.reading.hasStatus {
        cached
      } else if overview.moreSessions > 0 {
        overview.hostSessions + cached
      } else {
        overview.hostSessions
      }
    var keys = Set<String>()
    return entries.filter { keys.insert($0.key).inserted }
  }

  /// One session as every surface names it, looked up once: the status first, then the sessions' list's live
  /// sessions, its records, and last the names the log or the apps list gives, so the sidebar, Activity, Screenshots,
  /// Apps, the menus and the card's header agree on its name, icon and state.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - snapshot: What the controller holds.
  /// - Returns: The session as resolved.
  static func resolvedSession(_ key: String, _ snapshot: ViewerSnapshot) -> ViewerResolvedSession {
    let listed = snapshot.reading.statusReading.session(key)
    let current = currentSessions(snapshot)
      .first { $0.key == key }
    let listLive = listed == nil ? current : nil
    let record = snapshot.panes.sessions.record(key)
    let logged = snapshot.panes.activity.log?.sessions[key] ?? snapshot.panes.apps.sessions[key]
    let live = current.flatMap { StatusReading.isRelevant($0) ? $0 : nil }
    let ended = current?.state == .ended || (current == nil && record?.isEnded == true)
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
}
