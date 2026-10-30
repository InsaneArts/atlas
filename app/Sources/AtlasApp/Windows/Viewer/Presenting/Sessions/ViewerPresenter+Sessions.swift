import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
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
    let listLive = snapshot.panes.sessions.live(key)
    let record = snapshot.panes.sessions.record(key)
    let logged = snapshot.panes.activity.log?.sessions[key] ?? snapshot.panes.apps.sessions[key]
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
}
