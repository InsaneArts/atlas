import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting

@testable import AtlasApp

/// The app's parts as the coordinator sees them, standing in for the container: scripted controllers, the Dock icon's
/// changes recorded, and every window made a ``WindowStandIn``, so no test reaches a screen, the Dock or a service.
final class StandInParts: WindowParts {
  /// No panel: a test's windows open on nothing it read.
  var panel: PanelController?
  /// A clock nobody advances.
  let clock: any Clock<Duration> = ManualClock()
  /// Each Dock icon change the coordinator asked for, in order.
  private(set) var dockIcon: [Bool] = []
  /// How many main windows' controllers were made.
  private(set) var viewersMade = 0
  /// How many Settings controllers were made.
  private(set) var settingsMade = 0
  /// How often Atlas was asked to quit.
  private(set) var terminations = 0
  /// Every window made, in order.
  private(set) var windows: [WindowStandIn] = []

  /// The coordinator's window making: a stand-in for each window, kept in ``windows``.
  var making: WindowMaking {
    WindowMaking(
      viewer: { [unowned self] _, closed in standIn(closed) },
      settings: { [unowned self] _, closed in standIn(closed) },
      onboarding: { [unowned self] _, closed in standIn(closed) },
      alert: { [unowned self] _ in standIn { _ in } }
    )
  }

  /// A coordinator over these parts, with no window open.
  ///
  /// - Parameter preferences: The main window's saved choices; in memory by default.
  /// - Returns: The coordinator.
  func coordinator(preferences: ViewerPreferences = .memory()) -> WindowCoordinator {
    let coordinator = WindowCoordinator(windows: making, viewerPreferences: preferences)
    coordinator.parts = self
    return coordinator
  }

  /// A main window's controller over a scripted service.
  ///
  /// - Parameter switchToHelper: Not used.
  /// - Returns: The controller.
  func viewerController(switchToHelper: @escaping @MainActor () async -> String) -> ViewerController? {
    viewersMade += 1
    return .scripted(ScriptedService())
  }

  /// A Settings controller over a scripted command line.
  ///
  /// - Parameter links: Not used: the scripted controller's links stand still.
  /// - Returns: The controller.
  func settingsController(links: SettingsLinks) -> SettingsController? {
    settingsMade += 1
    return .scripted(commands: ScriptedCommands([:]))
  }

  /// A setup window's controller over a scripted command line.
  ///
  /// - Returns: The controller.
  func onboardingController() -> OnboardingController {
    OnboardingControllerTests.controller(ScriptedCommands([:]))
  }

  /// No alert window: the tests here open none.
  ///
  /// - Parameters:
  ///   - alert: Not used.
  ///   - links: Not used.
  /// - Returns: Nil.
  func alertController(_ alert: HelperAlert, links: AlertLinks) -> AlertController? { nil }

  /// Records the change.
  ///
  /// - Parameter shown: Whether the Dock icon shows.
  func showDockIcon(_ shown: Bool) { dockIcon.append(shown) }

  /// Nothing to close: there is no panel.
  func closePanel() {}

  /// Records the quit, which no test performs.
  ///
  /// - Parameter sender: Not used.
  func terminate(_ sender: Any?) { terminations += 1 }

  /// Nothing to read into.
  ///
  /// - Parameter status: Not used.
  func read(_ status: ViewerStatus?) {}

  /// A stand-in window, kept in ``windows``.
  ///
  /// - Parameter closed: The coordinator's close hook.
  /// - Returns: The stand-in.
  private func standIn(_ closed: @escaping @MainActor (any WindowPresenting) -> Void) -> WindowStandIn {
    let window = WindowStandIn(closed: closed)
    windows.append(window)
    return window
  }
}
