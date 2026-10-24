import AppKit
import AtlasCommands
import AtlasService

/// Each window's controller over its live dependencies, as the coordinator opens the window: the main window's,
/// Settings', the setup window's and the alert window's. The system reads they need (uptime, the double-click speed,
/// the workspace that opens a page) are made here, so no controller reaches them itself. A controller that needs the
/// helper's commands waits for launch to have made them.
extension AppContainer: WindowParts {
  /// A main window's controller.
  ///
  /// - Parameter switchToHelper: Locks the screen for the helper, returning what to tell the person.
  /// - Returns: The controller, or nil before launch.
  func viewerController(switchToHelper: @escaping @MainActor () async -> String) -> ViewerController? {
    guard let actions else { return nil }
    let commands = commands
    return ViewerController(
      dependencies: ViewerDependencies(
        service: service,
        actions: actions,
        preferences: viewerPreferences,
        uptime: { ProcessInfo.processInfo.systemUptime },
        clock: clock,
        heartbeatInterval: ViewerController.heartbeatEvery,
        doubleClickInterval: { NSEvent.doubleClickInterval },
        pictures: .live,
        switchToHelper: switchToHelper,
        connectedAgents: {
          let outcome = await commands.run(.agentsStatus)
          return outcome.ok ? outcome.decode(AgentsStatus.self)?.connected : nil
        },
        loginTime: LoginRecords.loginTime(of:),
        shotOutlet: .live(NSWorkspace.shared)
      )
    )
  }

  /// The Settings window's controller, its saved choices telling the signals when they change.
  ///
  /// - Parameter links: What Settings shares with the other windows.
  /// - Returns: The controller, or nil before launch.
  func settingsController(links: SettingsLinks) -> SettingsController? {
    guard let actions else { return nil }
    return SettingsController(
      dependencies: SettingsDependencies(
        commands: commands,
        actions: actions,
        installs: installs,
        preferences: watchedPreferences(),
        agentIcons: { AgentIcons.onThisMac(NSWorkspace.shared) },
        version: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
        links: links,
        clock: clock
      )
    )
  }

  /// The setup window's controller.
  ///
  /// - Returns: The controller.
  func onboardingController() -> OnboardingController {
    OnboardingController(
      dependencies: Self.onboardingDependencies(
        commands: commands,
        loginItem: loginItem,
        installs: installs,
        clock: clock
      )
    )
  }

  /// The alert window's controller, for one alert.
  ///
  /// - Parameters:
  ///   - alert: The alert it asks first.
  ///   - links: What the alert window asks of the rest of the app.
  /// - Returns: The controller, or nil before launch.
  func alertController(_ alert: HelperAlert, links: AlertLinks) -> AlertController? {
    guard let actions else { return nil }
    return AlertController(alert: alert, actions: actions, links: links)
  }

  /// The setup window's live dependencies, for the app and for the setup window on its own (`--onboarding`).
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - loginItem: The app's login item.
  ///   - installs: Connecting agents, shared with Settings.
  ///   - clock: The clock the window's waits run on.
  /// - Returns: The dependencies.
  static func onboardingDependencies(
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
