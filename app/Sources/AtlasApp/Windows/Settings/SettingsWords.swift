import AtlasKit
import AtlasUI

/// The Settings window's words that depend on its state: the tabs, the helper's line, the agents' rows, the projects,
/// the alerts it alone asks, the update's answers and its notices. Fixed copy stays in AtlasUI's views; the helper's
/// alerts read the main window's words (``ViewerWords``), so the same question reads the same in both windows.
enum SettingsWords {
  /// The window's title, which only the Window menu shows.
  static let windowTitle = "Settings"
  /// The tab row, for VoiceOver.
  static let tabsLabel = "Settings"

  /// A tab's word.
  ///
  /// - Parameter tab: The tab.
  /// - Returns: "General", "Helper", "Agents" or "About".
  static func tab(_ tab: SettingsTab) -> String {
    switch tab {
    case .general: "General"
    case .helper: "Helper"
    case .agents: "Agents"
    case .about: "About"
    }
  }

  // MARK: General

  /// The picture's choice that reads as often as the window reads.
  static let live = "Live"
  /// The picture's choice of one every five seconds.
  static let everyFiveSeconds = "Every 5 seconds"
  /// The picture's choice of none.
  static let paused = "Paused"

  // MARK: Helper

  /// What the helper's account is, with its display when known.
  ///
  /// - Parameter display: Its display, such as "2560 × 1440", when known.
  /// - Returns: "The hidden helper login · 2560 × 1440".
  static func helperLine(display: String?) -> String {
    let line = "The hidden helper login"
    return display.map { "\(line) · \($0)" } ?? line
  }

  /// What Check Again's diagnosis found: ready, what to do first, or that it could not check.
  ///
  /// - Parameter answer: The diagnosis.
  /// - Returns: The line under the pane.
  static func diagnosis(_ answer: DoctorAnswer) -> String {
    switch answer {
    case .ready: "The helper is ready to act."
    case .needs(let steps): steps.isEmpty ? "The helper isn't ready to act." : steps.joined(separator: " ")
    case .failed(let message):
      message.isEmpty
        ? "Atlas couldn't check the helper." : "Atlas couldn't check the helper: \(Words.sentence(message))"
    }
  }

  // MARK: Agents

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

  // MARK: About

  /// The version line, or what a build with no version is.
  ///
  /// - Parameter version: The app's version, when known.
  /// - Returns: "Version 0.1.0", or "Development build".
  static func version(_ version: String?) -> String {
    version.map { "Version \($0)" } ?? "Development build"
  }

  /// The check found nothing newer.
  static let upToDate = "Atlas is up to date."
  /// The check did not answer.
  static let checkFailed = "Atlas couldn't check for updates."
  /// No release is published yet.
  static let unpublished = "Atlas has no public release yet. Check atlas.insanearts.io for news."
  /// Once the newer version is installed, in the moment before Atlas restarts itself to finish.
  static let installed = "Atlas is updated and restarts itself to finish."

  /// The check found a newer version.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Atlas 0.2.0 is available."
  static func available(_ version: String) -> String {
    "Atlas \(version) is available."
  }

  /// The Install alert's title.
  ///
  /// - Parameter version: The newer version.
  /// - Returns: "Install Atlas 0.2.0?".
  static func installTitle(_ version: String) -> String {
    "Install Atlas \(version)?"
  }

  /// The Install alert's sentence: the update restarts Atlas, closing this window.
  static let installMessage = "Atlas checks the release's signature and installs it, then restarts itself to finish."
  /// The Install alert's button.
  static let install = "Install"
  /// The Install alert's button while it runs.
  static let installing = "Installing…"
}
