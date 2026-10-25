import AppKit
import AtlasCommands
import AtlasUI

/// What the setup window's controller reaches outside itself, handed in so tests can stand in for each: the `atlas`
/// command line, opening a URL, the login item, the clock its waits run on, the login timing's log, the agents' and
/// the asking account's pictures, and the Finder.
struct OnboardingDependencies {
  /// The `atlas` command line every action runs.
  var commands: any CommandRunning
  /// Opens a URL, such as a Privacy & Security page.
  var openURL: @MainActor (URL) -> Void
  /// The app's login item, read for Open at Login's first value.
  var loginItem: OpenAtLogin
  /// Connecting agents, shared with Settings, so one install runs at a time from either window.
  var installs: AgentInstalls
  /// Atlas.app, which the permission panel drags into System Settings: the one running, else the one in Applications
  /// (a build run outside its app).
  var app =
    Bundle.main.bundleURL.pathExtension == "app"
    ? Bundle.main.bundleURL : URL(fileURLWithPath: "/Applications/Atlas.app")
  /// The clock every wait runs on: the pause between readings of the facts, a permission's tick before the panel
  /// moves on, and the permission panel following System Settings. A cancelled wait ends at once.
  var clock: any Clock<Duration>
  /// Notes a marker in the login timing, in the unified log.
  var mark: @MainActor (String) -> Void = LoginLog.mark
  /// The icons of the agents whose apps are on this Mac, by agent id, as Settings reads them.
  var agentIcons: @MainActor () -> [String: IconImage] = {
    AgentRowIcons.onThisMac(home: FileManager.default.homeDirectoryForCurrentUser, icon: FileIcons.icon(forFile:))
  }
  /// An account's picture as macOS shows it at login, by the account's short name.
  var accountPicture: @MainActor (String) -> IconImage? = AccountPictures.picture(of:)
  /// Shows a file selected in the Finder, such as Atlas.app when it must move.
  var reveal: @MainActor (URL) -> Void = { url in
    NSWorkspace.shared
      .activateFileViewerSelecting([url])
  }
}
