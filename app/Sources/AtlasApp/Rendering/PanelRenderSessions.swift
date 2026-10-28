import AtlasService
import Foundation

/// The host sessions `ATLAS_RENDER_SESSIONS` puts in the live status for design checks: Claude Code · understudy,
/// Codex · atlas-landing, Cursor · opie and, past the panel's three rows, a Terminal one and Pi · notes-sync.
enum PanelRenderSessions: String, CaseIterable, Sendable {
  /// One at work, Claude Code: no session is named and the panel is as it always was.
  case one
  /// Two at work: Claude Code has the helper and Cursor is idle.
  case two
  /// Three at work: Claude Code has the helper, Codex waits its turn, Cursor is idle.
  case waiting
  /// Claude Code's click may not have finished, and Codex waits until the person recovers.
  case needsYou
  /// Five at work: more than the panel's rows.
  case more

  /// The overview with these sessions in it: their entries, the holder, and each session's newest action as the
  /// overview's rows; while one needs the person, an action of unknown outcome.
  ///
  /// - Parameter overview: The overview the live service gave.
  /// - Returns: The overview to draw.
  func applied(to overview: ViewerStatus.Overview) -> ViewerStatus.Overview {
    var shown = overview
    let sessions = sessions
    // The service speaks of sessions only while two or more are live.
    shown.sessionsShown = sessions.count > 1
    shown.hostSessions = sessions
    shown.holder = sessions.first(where: \.holdsHelper)?.key
    shown.activity = sessions.compactMap { session in
      session.latest.map { latest in
        var row = latest
        row.session = session.key.isEmpty ? nil : session.key
        return row
      }
    }
    .sorted { ($0.ageSeconds ?? 0) < ($1.ageSeconds ?? 0) }
    if self == .needsYou { shown.pending = max(1, shown.pending) }
    return shown
  }

  /// The sessions, in the service's order.
  private var sessions: [ViewerStatus.HostSession] {
    switch self {
    case .one: [Self.claude(acting: true)]
    case .two: [Self.claude(acting: true), Self.cursor]
    case .waiting: [Self.claude(acting: true), Self.codex(waitingFor: .helper), Self.cursor]
    case .needsYou: [Self.claude(acting: false), Self.codex(waitingFor: .recovery), Self.cursor]
    case .more: [Self.claude(acting: true), Self.codex(waitingFor: .helper), Self.cursor, Self.terminal, Self.pi]
    }
  }

  /// Claude Code · understudy, clicking in Opie with the helper, or with its click of unknown outcome.
  ///
  /// - Parameter acting: Whether it has the helper, rather than needing the person.
  /// - Returns: The session.
  private static func claude(acting: Bool) -> ViewerStatus.HostSession {
    ViewerStatus.HostSession(
      key: "3f9a1c07b2e4",
      name: "Claude Code · understudy",
      host: .claude,
      hostLabel: "Claude Code",
      folder: "understudy",
      state: acting ? .acting : .needsYou,
      holdsHelper: acting,
      latest: acting
        ? ViewerStatus.Activity(
          id: "c1",
          operation: "input",
          ok: true,
          ageSeconds: 0,
          says: "clicking Show Center in Opie"
        )
        : ViewerStatus.Activity(
          id: "c1",
          operation: "input",
          ok: false,
          ageSeconds: 60,
          says: "clicked Show Center in Opie",
          uncertain: true
        ),
      uncertainRow: acting ? nil : "c1",
      uncertainVerb: acting ? nil : "click"
    )
  }

  /// Codex · atlas-landing, waiting its turn for 8 s or until the person recovers.
  ///
  /// - Parameter waitingFor: What it waits for.
  /// - Returns: The session.
  private static func codex(waitingFor: HostSessionWait) -> ViewerStatus.HostSession {
    ViewerStatus.HostSession(
      key: "8be20c4d1a77",
      name: "Codex · atlas-landing",
      host: .codex,
      hostLabel: "Codex",
      folder: "atlas-landing",
      state: .waiting,
      waitingFor: waitingFor,
      waitingSeconds: waitingFor == .helper ? 8 : nil,
      latest: ViewerStatus.Activity(id: "x1", operation: "input", ok: true, ageSeconds: 31, says: "opened Notes")
    )
  }

  /// Cursor · opie, idle since it opened Opie 4 minutes ago.
  private static let cursor = ViewerStatus.HostSession(
    key: "5d21e9a04c3f",
    name: "Cursor · opie",
    host: .cursor,
    hostLabel: "Cursor",
    folder: "opie",
    latest: ViewerStatus.Activity(id: "u1", operation: "input", ok: true, ageSeconds: 240, says: "opened Opie")
  )

  /// Terminal, idle since a command took a screenshot a minute ago.
  private static let terminal = ViewerStatus.HostSession(
    key: "",
    name: "Terminal",
    host: .terminal,
    latest: ViewerStatus.Activity(
      id: "t1",
      operation: "screenshot",
      ok: true,
      ageSeconds: 70,
      says: "took a screenshot"
    )
  )

  /// Pi · notes-sync, idle since it opened Notes 20 minutes ago.
  private static let pi = ViewerStatus.HostSession(
    key: "c0ffee5e5510",
    name: "Pi · notes-sync",
    host: .pi,
    hostLabel: "Pi",
    folder: "notes-sync",
    latest: ViewerStatus.Activity(id: "p1", operation: "input", ok: true, ageSeconds: 1200, says: "opened Notes")
  )
}
