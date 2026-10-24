/// The Agents tab's words: each agent's rows and states, an agent that did not connect, and forgetting a project, with
/// its alert and what follows it.
extension SettingsWords {
  /// What Atlas does in an agent with a plugin.
  static let usesAtlas = "Uses Atlas in every project"
  /// What Atlas does in the Claude app.
  static let showsHelper = "Shows the helper in your chats"
  /// The Claude app's name.
  static let claudeApp = "Claude app"
  /// An agent before `atlas agents status` has answered.
  static let checking = "Checking…"
  /// An agent `atlas agents status` could not report.
  static let notChecked = "Couldn't check"
  /// An agent not on this Mac.
  static let notInstalled = "Not installed"

  /// An agent that did not connect, and why.
  ///
  /// - Parameters:
  ///   - agent: The agent's name.
  ///   - message: Why, in one line; empty when the command gave no reason.
  /// - Returns: "Claude app: its settings file is not valid JSON."
  static func notConnected(_ agent: String, _ message: String) -> String {
    let reason = message.isEmpty ? "it didn't connect" : message
    return "\(agent): \(Words.sentence(reason))"
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
  /// - Returns: "Forget opie?".
  static func forgetTitle(_ project: String) -> String {
    "Forget \(project)?"
  }

  /// The Forget alert's sentence: the section leaves the project's files.
  static let forgetMessage =
    "Atlas takes its section out of the project's AGENTS.md and CLAUDE.md. Agents there stop testing in the helper."
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
