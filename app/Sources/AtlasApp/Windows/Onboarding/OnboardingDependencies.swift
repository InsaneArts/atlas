import AppKit
import AtlasCommands

/// What the setup window's controller reaches outside itself, handed in so tests can stand in for each: the `atlas`
/// command line, opening a URL, the login item and the clock its waits run on.
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

  /// The app's own: `commands`, NSWorkspace, the bundled login item and the app's clock.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - loginItem: The app's login item.
  ///   - installs: Connecting agents, shared with Settings.
  ///   - clock: The app's clock.
  /// - Returns: The live dependencies.
  static func live(
    commands: any CommandRunning,
    loginItem: OpenAtLogin,
    installs: AgentInstalls,
    clock: any Clock<Duration>
  ) -> OnboardingDependencies {
    OnboardingDependencies(
      commands: commands,
      openURL: { url in
        _ = NSWorkspace.shared
          .open(url)
      },
      loginItem: loginItem,
      installs: installs,
      clock: clock
    )
  }
}
