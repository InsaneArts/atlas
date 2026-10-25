import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Observation

@testable import AtlasApp

/// The app's parts as the coordinator sees them, standing in for the container: scripted controllers, the Dock icon's
/// changes recorded, and every window made a ``WindowStandIn``, so no test reaches a screen, the Dock or a service.
/// Observed, so a test can wait for Atlas to be asked to quit.
@Observable
final class StandInParts: WindowParts {
  /// No panel: a test's windows open on nothing it read.
  var panel: PanelController?
  /// A clock nobody advances.
  let clock: any Clock<Duration> = ManualClock()
  /// No control work of the app's own: a test's control work is its main window's.
  var controlBarrier: ControlBarrier? { nil }
  /// Makes each main window's controller: one over a scripted service by default.
  var makeViewer: @MainActor () -> ViewerController = { .scripted(ScriptedService()) }
  /// Makes each setup window's controller, over an empty scripted command line by default.
  var makeOnboarding: @MainActor () -> OnboardingController = { .scripted(ScriptedCommands([:])) }
  /// Whether Copy Diagnostics finds anything to copy.
  var diagnostics = true
  /// App-level diagnostic results, without presenting a native alert.
  var diagnosticNotices: [String] = []
  /// Runs as Atlas is asked to quit, before it is counted.
  var quitting: @MainActor () -> Void = {}
  /// Each Dock icon change the coordinator asked for, in order.
  private(set) var dockIcon: [Bool] = []
  /// How many main windows' controllers were made.
  private(set) var viewersMade = 0
  /// How many Settings controllers were made.
  private(set) var settingsMade = 0
  /// How often Atlas was asked to quit.
  private(set) var terminations = 0
  /// How often the panel was asked to close, leaving its controller running.
  private(set) var panelClosures = 0
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
    let coordinator = WindowCoordinator(windows: making, viewerPreferences: preferences) { [weak self] in
      self?.diagnosticNotices.append($0)
    }
    coordinator.parts = self
    return coordinator
  }

  /// A main window's controller over a scripted service.
  ///
  /// - Parameter switchToHelper: Not used.
  /// - Returns: The controller.
  func viewerController(switchToHelper: @escaping @MainActor () async -> String) -> ViewerController? {
    viewersMade += 1
    return makeViewer()
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
    makeOnboarding()
  }

  /// An alert window's controller over a scripted service and commands.
  ///
  /// - Parameters:
  ///   - alert: The alert shown.
  ///   - links: What the alert asks of the coordinator.
  /// - Returns: The controller.
  func alertController(_ alert: HelperAlert, links: AlertLinks) -> AlertController? {
    let service = ScriptedService()
    return AlertController(
      alert: alert,
      actions: HelperActions(
        commands: ScriptedCommands([:]),
        service: service,
        barrier: ControlBarrier(service: service, clock: clock)
      ),
      links: links
    )
  }

  /// Records the change.
  ///
  /// - Parameter shown: Whether the Dock icon shows.
  func showDockIcon(_ shown: Bool) { dockIcon.append(shown) }

  /// Records closing the panel, without stopping its controller.
  func closePanel() { panelClosures += 1 }

  /// Records the quit, which no test performs, after what runs as it is asked.
  ///
  /// - Parameter sender: Not used.
  func terminate(_ sender: Any?) {
    quitting()
    terminations += 1
  }

  /// Copies nothing anywhere: says whether there was anything to copy.
  ///
  /// - Returns: ``diagnostics``.
  func copyDiagnostics() async -> Bool { diagnostics }

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
