import AtlasCommands
import AtlasUI
import CoreGraphics
import Foundation

/// What the Settings window's controller reaches outside itself, handed in so tests can stand in for each: the
/// `atlas` command line, the helper's own commands, the saved choices, the icon a file shows, the app's version, its
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
  /// The icon a file or an app shows, by its path, or nil where nothing is there (``FileIcons`` in the app).
  var fileIcon: @MainActor (String) -> CGImage?
  /// The app's version, such as `0.1.0`; nil in a build with no bundle, such as a development one.
  var version: String?
  /// What Settings shares with the other windows.
  var links: SettingsLinks
  /// The clock any wait of the window runs on, so a test moves its time by hand.
  var clock: any Clock<Duration>

  /// The icon a file or an app shows: the agents' apps' icons come from here, never from the workspace directly (A-17).
  ///
  /// - Parameter path: The file's path, such as `/Applications/Claude.app`.
  /// - Returns: Its icon, or nil where nothing is there.
  @MainActor func icon(forFile path: String) -> CGImage? {
    fileIcon(path)
  }
}
