import AppKit
import AtlasCommands

/// `--onboarding`: the setup window on its own, for Atlas.app opened before it is set up. It quits when the window
/// closes.
enum OnboardingAlone {
  /// Opens the setup window and runs until it closes.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` the window runs.
  ///   - loginItem: The app's login item.
  ///   - clock: The clock the window's waits run on.
  /// - Returns: Never; the process exits.
  static func run(commands: Commands, loginItem: OpenAtLogin, clock: any Clock<Duration>) -> Never {
    let app = NSApplication.shared
    app.mainMenu = MainMenu.make()
    let controller = OnboardingController(
      dependencies: AppContainer.onboardingDependencies(
        commands: commands,
        loginItem: loginItem,
        installs: AgentInstalls(commands: commands),
        clock: clock
      )
    )
    let window = OnboardingWindow(controller: controller) { _ in NSApp.terminate(nil) }
    DockIcon.settle(windowOpen: true)
    app.finishLaunching()
    window.present()
    withExtendedLifetime(window) { app.run() }
    exit(0)
  }
}
