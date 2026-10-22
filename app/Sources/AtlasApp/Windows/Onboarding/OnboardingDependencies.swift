import AppKit
import AtlasKit

/// What the setup window's controller reaches outside itself, handed in so tests can stand in for each: the `atlas`
/// command line, opening a URL, the folder chooser and the login item.
struct OnboardingDependencies {
  /// The `atlas` command line every action runs.
  var commands: any CommandRunning
  /// Opens a URL, such as a Privacy & Security page.
  var openURL: @MainActor (URL) -> Void
  /// Asks the person for project folders to teach; empty when they cancel.
  var chooseFolders: @MainActor () -> [URL]
  /// The app's login item, read for Open at Login's first value.
  var loginItem: OpenAtLogin
  /// Connecting agents, shared with Settings, so one install runs at a time from either window.
  var installs: AgentInstalls

  /// The app's own: `commands`, NSWorkspace, an open panel and the bundled login item.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - loginItem: The app's login item.
  ///   - installs: Connecting agents, shared with Settings.
  /// - Returns: The live dependencies.
  static func live(
    commands: any CommandRunning,
    loginItem: OpenAtLogin,
    installs: AgentInstalls
  ) -> OnboardingDependencies {
    OnboardingDependencies(
      commands: commands,
      openURL: { url in
        _ = NSWorkspace.shared
          .open(url)
      },
      chooseFolders: askForFolders,
      loginItem: loginItem,
      installs: installs
    )
  }

  /// Asks for project folders in an open panel.
  ///
  /// - Returns: The folders chosen, or none when the person cancels.
  @MainActor private static func askForFolders() -> [URL] {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.allowsMultipleSelection = true
    panel.prompt = "Teach"
    panel.message = "Choose a project folder. Atlas adds a short section to its AGENTS.md."
    guard panel.runModal() == .OK else { return [] }
    return panel.urls
  }
}
