import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation

@testable import AtlasApp

/// The host sessions the panel's suites read, as the service writes them: Claude Code · understudy, Codex ·
/// atlas-landing, Cursor · opie, a Terminal one and Pi · notes-sync, the same as the panel's samples.
enum SessionReplies {
  /// Claude Code with the helper, clicking in Opie.
  static var claudeActing: [String: Any] {
    hostSession(
      "3f9a1c07b2e4",
      host: "claude",
      name: "Claude Code · understudy",
      state: "acting",
      hostLabel: "Claude Code",
      holdsHelper: true,
      latest: ["says": "clicking Show Center in Opie", "ok": true, "started": true, "ageSeconds": 0]
    )
  }

  /// Claude Code, whose click may not have finished.
  static var claudeNeedsYou: [String: Any] {
    hostSession(
      "3f9a1c07b2e4",
      host: "claude",
      name: "Claude Code · understudy",
      state: "needsYou",
      hostLabel: "Claude Code",
      latest: ["says": "clicked Show Center in Opie", "ok": false, "uncertain": true, "ageSeconds": 60],
      uncertainRow: "c1",
      uncertainVerb: "click"
    )
  }

  /// Codex waiting its turn for the helper, 8 s so far, or until the person recovers.
  ///
  /// - Parameter recovery: Whether it waits for the person to recover, rather than for its turn.
  /// - Returns: The entry.
  static func codexWaiting(recovery: Bool = false) -> [String: Any] {
    hostSession(
      "8be20c4d1a77",
      host: "codex",
      name: "Codex · atlas-landing",
      state: "waiting",
      hostLabel: "Codex",
      waitingFor: recovery ? "recovery" : "helper",
      waitingSeconds: recovery ? nil : 8,
      latest: ["says": "opened Notes", "ok": true, "ageSeconds": 31]
    )
  }

  /// Cursor, idle since it opened Opie 4 minutes ago.
  static var cursorIdle: [String: Any] {
    hostSession(
      "5d21e9a04c3f",
      host: "cursor",
      name: "Cursor · opie",
      hostLabel: "Cursor",
      latest: ["says": "opened Opie", "ok": true, "ageSeconds": 240]
    )
  }

  /// Terminal, idle since a command took a screenshot a minute ago.
  static var terminalIdle: [String: Any] {
    hostSession(
      "",
      host: "none",
      name: "Terminal",
      latest: ["says": "took a screenshot", "ok": true, "ageSeconds": 70]
    )
  }

  /// Pi, idle since it opened Notes 20 minutes ago.
  static var piIdle: [String: Any] {
    hostSession(
      "c0ffee5e5510",
      host: "pi",
      name: "Pi · notes-sync",
      hostLabel: "Pi",
      latest: ["says": "opened Notes", "ok": true, "ageSeconds": 1200]
    )
  }

  /// What the panel's controller holds while these sessions are listed: the helper in `mode`, sessions shown while
  /// two or more are listed, Claude Code's newest action as the overview's row, and the helper's screen on show.
  ///
  /// - Parameters:
  ///   - sessions: The entries, in the service's order.
  ///   - mode: The helper's mode, as the service names it.
  ///   - state: The state the service shows for it.
  ///   - pending: Actions of unknown outcome.
  ///   - more: How many more sessions there are than listed.
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives.
  static func snapshot(
    _ sessions: [[String: Any]],
    mode: String = "agent",
    state: String = "working",
    pending: Int = 0,
    more: Int = 0
  ) throws -> PanelSnapshot {
    let holder = sessions.first { $0["holdsHelper"] as? Bool == true }?["key"]
    var overview: [String: Any] = [
      "mode": mode, "state": state, "reason": "", "pending": pending, "sessionsShown": sessions.count > 1,
      "hostSessions": sessions, "moreSessions": more,
      "activity": [
        [
          "id": "c1", "operation": "input", "actor": "agent", "ok": pending == 0, "ageSeconds": pending == 0 ? 0 : 60,
          "says": pending == 0 ? "clicking Show Center in Opie" : "clicked Show Center in Opie",
          "session": "3f9a1c07b2e4", "uncertain": pending > 0,
        ]
      ],
    ]
    overview["holder"] = holder
    return PanelSnapshot(status: try status(["overview": overview]), picture: SampleDesktop.picture)
  }
}
