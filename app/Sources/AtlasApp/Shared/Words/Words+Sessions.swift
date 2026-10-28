import AtlasService
import AtlasStatus
import Foundation

/// The words for host sessions the panel and the main window share: each state's word, each line, who has the
/// helper, and whose action may not have finished.
extension Words {
  /// A state in a word, beside a session's name.
  ///
  /// - Parameter state: What the session is doing.
  /// - Returns: "Acting", "Waiting", "Idle", "Needs you" or "Ended".
  static func sessionState(_ state: HostSessionState) -> String {
    switch state {
    case .acting: "Acting"
    case .waiting: "Waiting"
    case .idle: "Idle"
    case .needsYou: "Needs you"
    case .ended: "Ended"
    }
  }

  /// A session's line under its name.
  ///
  /// - Parameters:
  ///   - line: What the line says, as AtlasStatus decides it.
  ///   - now: The time now, which an ended session's age counts to.
  /// - Returns: "Clicking Show Center in Opie", "Waiting for the helper · 8 s", "Waits until you recover", "A click may
  ///   not have finished", "Last: opened Opie · 4 min ago", "Ended 20 min ago", "Last seen 2 h ago", or empty.
  static func sessionLine(_ line: SessionLine, now: Date) -> String {
    switch line {
    case .acting(let says):
      guard let says, let first = says.first else { return "Has the helper" }
      return first.uppercased() + says.dropFirst()
    case .waitingForHelper(let seconds):
      guard let seconds, seconds >= 1 else { return "Waiting for the helper" }
      return "Waiting for the helper · \(shortAgo(seconds))"
    case .waitingForRecovery:
      return "Waits until you recover"
    case .mayNotHaveFinished(let verb):
      return "\(actionNoun(verb)) may not have finished"
    case .last(let says, let age):
      return idleLine(last: says, ago: age.map(ago) ?? "")
    case .ended(let at):
      return at.map { "Ended \(ago(now.timeIntervalSince($0)))" } ?? "Ended"
    case .lastSeen(let at):
      return at.map { "Last seen \(ago(now.timeIntervalSince($0)))" } ?? "Quiet for a while"
    case .none:
      return ""
    }
  }

  /// Who has the helper, as the panel's line and the main window's chip say it.
  ///
  /// - Parameter name: The session's name.
  /// - Returns: "Claude Code · understudy has the helper".
  static func holderLine(_ name: String) -> String {
    "\(name) has the helper"
  }

  /// Whose action may not have finished, by its host alone, so the line fits beside Needs you.
  ///
  /// - Parameters:
  ///   - verb: The action's verb, such as `click`; nil when unknown.
  ///   - host: The host's name, such as "Claude Code".
  /// - Returns: "A click by Claude Code may not have finished".
  static func unfinishedLine(verb: String?, by host: String) -> String {
    "\(actionNoun(verb)) by \(host) may not have finished"
  }

  /// An action named by its verb, to lead a sentence.
  ///
  /// - Parameter verb: The action's verb, as the service names it; nil when unknown.
  /// - Returns: "A click", "Typing", "A key press", "A scroll", "A drag", "A command" or "An action".
  static func actionNoun(_ verb: String?) -> String {
    switch verb {
    case "click", "double-click", "right-click": "A click"
    case "type": "Typing"
    case "key": "A key press"
    case "scroll": "A scroll"
    case "drag": "A drag"
    case "run": "A command"
    default: "An action"
    }
  }

  /// A host's name: the service's own words when it gives them, else the host's label, else the session's name, as
  /// for a session that named itself.
  ///
  /// - Parameter session: The session.
  /// - Returns: "Claude Code", say.
  static func hostName(_ session: ViewerStatus.HostSession) -> String {
    hostName(session.host, label: session.hostLabel, name: session.name)
  }

  /// A host's name: `label` when the service gives one, else the host's label, else the session's name, as for a
  /// session that named itself.
  ///
  /// - Parameters:
  ///   - host: The app the session runs in.
  ///   - label: The host's name in the service's words; empty when it gives none.
  ///   - name: The session's name.
  /// - Returns: "Claude Code", say.
  static func hostName(_ host: SessionHost, label: String, name: String) -> String {
    if !label.isEmpty { return label }
    return switch host {
    case .claude: "Claude Code"
    case .codex: "Codex"
    case .cursor: "Cursor"
    case .opencode: "opencode"
    case .pi: "Pi"
    case .terminal: "Terminal"
    case .other: name
    }
  }
}
