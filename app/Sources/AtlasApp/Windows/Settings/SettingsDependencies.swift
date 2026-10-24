import AtlasCommands
import AtlasUI
import Foundation

/// What the Settings window's controller reaches outside itself, handed in so tests can stand in for each: the
/// `atlas` command line, the helper's own commands, the saved choices, the agents' app icons, the app's version, its
/// links to the other windows and the app's clock.
struct SettingsDependencies {
  /// The `atlas` command line: the agents and updates run through it.
  var commands: any CommandRunning
  /// The helper's own commands, shared with the panel and the main window.
  var actions: HelperActions
  /// Connecting agents, shared with the setup window.
  var installs: AgentInstalls
  /// The pane last shown, Notify me and the Dock badge.
  var preferences: SettingsPreferences
  /// The icons of the agents whose apps are on this Mac, by agent id; read once, as apps rarely come and go.
  var agentIcons: @MainActor () -> [String: IconImage]
  /// The app's version, such as `0.1.0`; nil in a build with no bundle, such as a development one.
  var version: String?
  /// What Settings shares with the other windows.
  var links: SettingsLinks
  /// The clock any wait of the window runs on, so a test moves its time by hand; none waits yet.
  var clock: any Clock<Duration>
}
