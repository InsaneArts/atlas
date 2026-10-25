/// The Agents tab's words: each agent's rows and states, an agent that did not connect, and forgetting a project, with
/// its alert and what follows it.
extension SettingsWords {
  /// The Claude app's name.
  static let claudeApp = "Claude app"
  /// An agent before `atlas agents status` has answered: the app's one word for a check under way.
  static let checking = StatusWord.checking.text
  /// An agent `atlas agents status` could not report.
  static let notChecked = "Couldn't check"
  /// An agent not on this Mac, under its name.
  static let notInstalled = "Not installed"

  /// Why an agent did not connect, in its own row: one line, no full stop.
  ///
  /// - Parameter message: Why, as the command said it; empty when it gave no reason.
  /// - Returns: "Couldn't connect: its settings file has a mistake", "Couldn't connect: the agent wasn't found", or
  ///   "Couldn't connect".
  static func notConnected(_ message: String) -> String {
    let reason =
      message
      .replacingOccurrences(
        of: "The Claude app's settings file is not valid JSON; fix it or move it aside, then try again",
        with: "its settings file has a mistake"
      )
      .replacingOccurrences(of: "settings file is not valid JSON", with: "settings file has a mistake")
      .trimmingCharacters(in: .whitespaces)
    guard !reason.isEmpty else { return "Couldn't connect" }
    let line = reason.hasSuffix(".") ? String(reason.dropLast()) : reason
    return "Couldn't connect: \(line)"
  }

  /// Whether a Connect failed on a settings file the person must mend, as `atlas` says when the file isn't valid.
  ///
  /// - Parameter message: Why, as the command said it.
  /// - Returns: Whether Show File helps.
  static func isBadSettingsFile(_ message: String) -> Bool {
    message.contains("settings file is not valid JSON")
  }

  /// Forget's name for VoiceOver.
  ///
  /// - Parameter project: The folder's name.
  /// - Returns: "Forget opie".
  static func forgetLabel(_ project: String) -> String {
    "Forget \(project)"
  }

  /// The Forget alert's title.
  ///
  /// - Parameter project: The folder's name.
  /// - Returns: "Forget “opie”?".
  static func forgetTitle(_ project: String) -> String {
    "Forget “\(project)”?"
  }

  /// The Forget alert's sentence: what leaves the project, in plain words.
  static let forgetMessage =
    "Atlas removes its section from this project's instruction files. Connected Atlas plugins keep working."
  /// The Forget alert's button.
  static let forget = "Forget"
  /// The Forget alert's button while it runs.
  static let forgetting = "Forgetting…"
  /// A project listed whose files no longer have the Atlas section.
  static let sectionGone = "No longer has the Atlas section"

  /// A project that could not be forgotten, and why.
  ///
  /// - Parameters:
  ///   - project: The folder's name.
  ///   - message: Why, in one line.
  /// - Returns: "opie wasn't forgotten: AGENTS.md has an Atlas marker without its pair."
  static func notForgotten(_ project: String, _ message: String) -> String {
    "\(project) wasn't forgotten: \(Words.sentence(message))"
  }
}
