import AtlasService
import Foundation

/// The words of a question Atlas asks the person before an agent sends, the same in the panel, the viewer and a
/// notification: what it would do, the task, what Allow for This Task covers, how long it has waited, and what the
/// person allowed.
enum AskWords {
  /// What the question asks about, such as "Send a message in Slack" or "Click “Merge pull request” in github.com".
  ///
  /// - Parameter ask: The question.
  /// - Returns: The title.
  static func title(_ ask: Ask) -> String {
    let account = ask.account
    switch ask.kind {
    case .sendKey, .newline:
      return "Send a message in \(account)"
    case .click:
      guard let element = ask.element else { return "Click in \(account)" }
      return "Click “\(element)” in \(account)"
    case .unnamed:
      return ask.role == "AXButton"
        ? "Click a button with no name in \(account)" : "Click something with no name in \(account)"
    case .password:
      return "Type in a password field in \(account)"
    case .drag:
      return "Drag in \(account)"
    case .unsure:
      if let key = ask.key { return "Press \(keyName(key)) in \(account)" }
      if ask.typed > 0 { return "Type \(characters(ask.typed)) in \(account)" }
      if let element = ask.element { return "Click “\(element)” in \(account)" }
      return "Act in \(account)"
    }
  }

  /// A key as the person reads it: each part capitalised, such as "Cmd+Return" for `cmd+return`.
  ///
  /// - Parameter key: The key as the service names it.
  /// - Returns: Its name.
  static func keyName(_ key: String) -> String {
    key
      .split(separator: "+")
      .map {
        $0.prefix(1)
          .uppercased() + $0.dropFirst()
      }
      .joined(separator: "+")
  }

  /// How many characters, such as "1 character" or "12 characters".
  ///
  /// - Parameter count: The count.
  /// - Returns: The words.
  static func characters(_ count: Int) -> String {
    count == 1 ? "1 character" : "\(count) characters"
  }

  /// What a password field would get, in place of what was typed: "12 characters in a password field".
  ///
  /// - Parameter count: How many characters.
  /// - Returns: The line.
  static func password(_ count: Int) -> String {
    "\(characters(count)) in a password field"
  }

  /// The task line: "Task: post the 0.1.1 release notes".
  ///
  /// - Parameter task: The task the agent named.
  /// - Returns: The line.
  static func task(_ task: String) -> String {
    "Task: \(task)"
  }

  /// The agent asking, from who asks: "Claude Code" of "Claude Code · release".
  ///
  /// - Parameter who: Who asks.
  /// - Returns: The agent's name.
  static func agent(_ who: String) -> String {
    who
      .components(separatedBy: " · ")
      .first ?? who
  }

  /// What Allow for This Task covers: "For this task covers Claude Code in Slack until it finishes or for 1 hour."
  ///
  /// - Parameters:
  ///   - agent: The agent asking.
  ///   - account: The account.
  /// - Returns: The note.
  static func scope(agent: String, account: String) -> String {
    "For this task covers \(agent) in \(account) until it finishes or for 1 hour."
  }

  /// How long a question has waited, in minutes and seconds: "0:42".
  ///
  /// - Parameter seconds: How long, in seconds; less than none reads as none.
  /// - Returns: The time.
  static func waited(_ seconds: TimeInterval) -> String {
    let whole = max(0, Int(seconds))
    return String(format: "%d:%02d", whole / 60, whole % 60)
  }

  /// What VoiceOver reads for a question: "Claude Code · release asks: Send a message in Slack".
  ///
  /// - Parameters:
  ///   - who: Who asks.
  ///   - title: What it asks about.
  /// - Returns: The label.
  static func label(who: String, title: String) -> String {
    "\(who) asks: \(title)"
  }

  /// What the person allowed: "Allowed in GitHub for this task".
  ///
  /// - Parameter account: The account.
  /// - Returns: The title.
  static func allowed(in account: String) -> String {
    "Allowed in \(account) for this task"
  }

  /// The agent and the time left, in whole minutes rounded up: "Claude Code · 52 min left", or "Claude Code · less
  /// than a minute left" in its last minute.
  ///
  /// - Parameters:
  ///   - agent: The agent it covers.
  ///   - seconds: The time left, in seconds.
  /// - Returns: The detail.
  static func timeLeft(agent: String, seconds: TimeInterval) -> String {
    guard seconds > 60 else { return "\(agent) · less than a minute left" }
    return "\(agent) · \(Int((seconds / 60).rounded(.up))) min left"
  }

  /// What VoiceOver reads for End: "End what you allowed in GitHub".
  ///
  /// - Parameter account: The account.
  /// - Returns: The label.
  static func end(_ account: String) -> String {
    "End what you allowed in \(account)"
  }

  /// The status's word while a question waits for the person, beside Needs you.
  static let waiting = "Waiting for you"

  /// The notification's body: who asks and the task, such as "Claude Code · release · post the 0.1.1 release notes".
  ///
  /// - Parameter ask: The question.
  /// - Returns: The body.
  static func notificationBody(_ ask: Ask) -> String {
    [ask.who, ask.task]
      .compactMap { $0 }
      .joined(separator: " · ")
  }
}
