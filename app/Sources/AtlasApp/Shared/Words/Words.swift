import AtlasCommands
import AtlasService
import AtlasStatus
import Foundation

/// Words for times, operations and facts, shared by the panel, the viewer and the setup window, each written once.
/// The domain packages return kinds; these are their words.
///
/// One punctuation rule holds for every word Atlas.app makes: a line a window shows on its own as a sentence (a hint, a
/// notice, a reason to wait) ends with its full stop, and a label, a title, a mode or a fact's value has none.
enum Words {
  /// The helper's name when the status gives none.
  static let helper = "Helper"
  /// A fact whose value is not known yet: the same dash in the panel and the viewer.
  static let unknown = "—"
  /// The helper's display, as the panel's and the viewer's facts name it.
  static let displayFact = "Display"
  /// The helper's reason before the service has given one.
  static let connecting = "Connecting to the helper"
  /// Why the helper's desktop cannot be watched, when the service says neither what to do next nor what went wrong.
  static let helperUnavailable = "Helper unavailable"
  /// The View menu's item while pictures arrive, and the pause button's name.
  static let pausePictureItem = "Pause Picture"
  /// The View menu's item while the picture is paused, and the pause button's name.
  static let resumePictureItem = "Resume Picture"
  /// This window holds control: the reason a window gives.
  static let yourControl = "You have mouse and keyboard control"
  /// Another window holds control: the mode's word.
  static let inUseElsewhere = "In use elsewhere"
  /// Another window holds control: the line beside the word, and the reason a window gives.
  static let inUseElsewhereLine = "Another Atlas viewer has control"
  /// The person holds control: the line beside the word, in the panel and the main window.
  static let controlLine = "The agent waits"
  /// The person stopped agents: the line beside the word.
  static let stoppedLine = "Resume when you're ready"
  /// Nothing is happening and no agent has acted yet: the line beside Idle, so a fresh helper says what it is for.
  static let readyLine = "Ready for your agents"
  /// No answer has come yet since Atlas opened: the line beside the word.
  static let startingLine = "Starting the helper…"
  /// No answer has come since the helper's answers stopped: the line beside the word.
  static let reconnectingLine = "Reconnecting…"
  /// An action's outcome is unknown: the line beside the word.
  static let unfinishedLine = "An action may not have finished"
  /// The helper's input stopped, which Recover starts again: the line beside the word.
  static let inputStoppedLine = "Mouse and keyboard stopped in the helper"
  /// Nothing is happening and no agent has acted yet: the reason a window gives.
  static let nothingHappening = "Nothing is happening in the helper"
  /// While the helper is recovered, from the panel or the viewer.
  static let recovering = "Recovering the helper…"
  /// Recovery gave no answer in time and may still be under way.
  static let stillRecovering = "The helper is still recovering. This can take a minute."
  /// Once recovery ran without restarting the helper, from the panel or the viewer.
  static let readyAgain = "The helper is ready again."
  /// Once recovery restarted the helper, which ended any control.
  static let recovered = "The helper was recovered. Take control again when ready."
  /// While the screen locks so the person can switch accounts, from the panel or the setup window.
  static let locking = "Locking the screen…"

  /// An agent's name.
  ///
  /// - Parameter agent: The agent.
  /// - Returns: "Claude" or "Codex".
  static func agentTitle(_ agent: AgentID) -> String {
    switch agent {
    case .claude: "Claude"
    case .codex: "Codex"
    }
  }

  /// Seconds ago as a short relative time.
  ///
  /// - Parameter seconds: How long ago.
  /// - Returns: "just now", or seconds, minutes or hours ago.
  static func ago(_ seconds: Double) -> String {
    guard seconds.isFinite else { return unknown }
    if seconds < 5 { return "just now" }
    if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
    if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
    guard let hours = Int(exactly: (seconds / 3600).rounded()) else { return unknown }
    return "\(hours) h ago"
  }

  /// Seconds ago as the shortest time a row's right edge holds, as the panel's Recent rows show it.
  ///
  /// - Parameter seconds: How long ago.
  /// - Returns: "now" under a second, then "9 s", "2 min" or "3 h".
  static func shortAgo(_ seconds: Double) -> String {
    guard seconds.isFinite else { return unknown }
    if seconds < 1 { return "now" }
    if seconds < 60 { return "\(Int(seconds.rounded(.down))) s" }
    if seconds < 3600 { return "\(Int((seconds / 60).rounded(.down))) min" }
    guard let hours = Int(exactly: (seconds / 3600).rounded(.down)) else { return unknown }
    return "\(hours) h"
  }

  /// What an agent did last, and when, beside Idle.
  ///
  /// - Parameters:
  ///   - last: What it did, such as "turned Show Center off".
  ///   - ago: How long ago, in words; empty when unknown.
  /// - Returns: "Last: turned Show Center off · 2 min ago", or without the time when it is unknown.
  static func idleLine(last: String, ago: String) -> String {
    ago.isEmpty ? "Last: \(last)" : "Last: \(last) · \(ago)"
  }

  /// The window an app shows on the screen, beside Idle.
  ///
  /// - Parameter app: The app's name.
  /// - Returns: "Showing Opie's window".
  static func showingLine(app: String) -> String {
    "Showing \(app)'s window"
  }

  /// What an operation did, after who did it: the service's own words when it gives them, so every app says the same.
  ///
  /// - Parameter row: The recorded operation.
  /// - Returns: What it did, such as "clicked Save in TextEdit" or "ran a command", or what could not be done.
  static func describe(_ row: ViewerStatus.Activity) -> String {
    if let says = row.says { return says }
    let count = row.actionCount ?? 0
    let actions = "\(count) action\(count == 1 ? "" : "s")"
    switch (row.operation, row.ok) {
    case ("input", true): return count > 0 ? "sent \(actions)" : "used the mouse and keyboard"
    case ("input", false): return count > 0 ? "couldn't send \(actions)" : "couldn't use the mouse and keyboard"
    case ("run", true): return "ran a command"
    case ("run", false): return "couldn't run a command"
    case ("restart", true): return "recovered the helper"
    case ("restart", false): return "couldn't recover the helper"
    case ("logout", true): return "logged out the helper"
    case ("logout", false): return "couldn't log out the helper"
    case ("screenshot", true): return "took a screenshot"
    case ("screenshot", false): return "couldn't take a screenshot"
    case ("apps", true): return "listed the open apps"
    case ("apps", false): return "couldn't list the open apps"
    case (_, true): return "did something in the helper"
    case (_, false): return "couldn't finish an action"
    }
  }

  /// A message as a sentence a window shows on its own: with its full stop, which a command's message may lack.
  ///
  /// - Parameter text: The message.
  /// - Returns: The message ending in a full stop, or another sentence's end; empty stays empty.
  static func sentence(_ text: String) -> String {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let last = trimmed.last, !".!?…".contains(last) else { return trimmed }
    return trimmed + "."
  }

  /// "1512 × 982", or nil when unknown.
  ///
  /// - Parameter size: A display's size, in points.
  /// - Returns: Width × height in whole points, or `nil` for no size or an empty one.
  static func size(_ size: ViewerStatus.Size?) -> String? {
    guard let size, size.isValid,
      let width = Int(exactly: size.width.rounded()),
      let height = Int(exactly: size.height.rounded())
    else { return nil }
    return "\(width) × \(height)"
  }

  /// How fresh the picture is, from the age of the last one.
  ///
  /// - Parameter age: The last picture's age, in seconds; `nil` before the first.
  /// - Returns: "Live", how long ago it updated, or that it is stale.
  static func freshness(_ age: Double?) -> String {
    guard let age, age.isFinite else { return "Waiting for the first picture" }
    if age < Freshness.live { return "Live" }
    if age < Freshness.stale { return "Updated \(Int(age.rounded())) s ago" }
    return "Picture is stale"
  }
}
