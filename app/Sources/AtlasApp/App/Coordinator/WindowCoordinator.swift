import AppKit
import AtlasUI

/// The one object that knows more than one window: it opens each window, brings an open one forward, gives the main
/// window's control back before Atlas quits, routes the panel's alerts, and builds every window's links to the others
/// (`WindowCoordinator+Links`), the Dock's menu (`WindowCoordinator+Dock`) and the Help menu's diagnostics.
/// One of each window at a time. The main window, Settings and the setup window show the Dock icon while open
/// (``DockIcon``); the alert window does not.
final class WindowCoordinator {
  /// How each window is made.
  private let windows: WindowMaking
  /// The main window's saved choices, which Settings saves the rate in while no main window is open.
  let viewerPreferences: ViewerPreferences
  /// The app's live parts the windows open on; the container owns this coordinator, so it is held weakly.
  weak var parts: (any WindowParts)?
  /// The main window, while open.
  private(set) var viewer: OpenWindow<ViewerController>?
  /// The Settings window, while open.
  private(set) var settings: OpenWindow<SettingsController>?
  /// The setup window, while open.
  private(set) var onboarding: OpenWindow<OnboardingController>?
  /// The alert window, while open: the panel's alerts while no main window is.
  private(set) var alert: OpenWindow<AlertController>?
  /// The Settings window's controller, kept for the app's life once made: an update check, an install or a Connect a
  /// closed window started runs on, and the next window waits for it rather than starting another.
  private(set) var settingsController: SettingsController?
  /// Whether Close All Windows is already waiting for control to be given back.
  private var closingWindows = false
  /// Whether setup should open once its old window's close has settled.
  private var reopeningOnboarding = false
  /// Whether termination has been requested, so late setup requests cannot reopen a window.
  private var terminating = false

  /// Reports Copy Diagnostics when its invoking window is not the viewer.
  let diagnosticsNotice: @MainActor (String) -> Void

  /// Creates the coordinator, with no window open.
  ///
  /// - Parameters:
  ///   - windows: How each window is made.
  ///   - viewerPreferences: The main window's saved choices.
  ///   - diagnosticsNotice: Shows an app-level result when no key viewer can display it.
  init(
    windows: WindowMaking,
    viewerPreferences: ViewerPreferences,
    diagnosticsNotice: @escaping @MainActor (String) -> Void = WindowCoordinator.showDiagnosticsNotice
  ) {
    self.diagnosticsNotice = diagnosticsNotice
    self.windows = windows
    self.viewerPreferences = viewerPreferences
  }

  /// The main window; brought forward when already open, on `tab` when one is given.
  ///
  /// - Parameters:
  ///   - tab: The tab to show, or nil for the one it shows.
  ///   - session: Whose work to show, every host session's or one's, or nil for the window's own choice; it is
  ///     chosen before the window shows.
  func openViewer(tab: ViewerTab? = nil, session: ViewerSessionChoice? = nil) {
    let prepare = { (controller: ViewerController) in
      if let session { controller.chooseSession(session) }
    }
    if let open = viewer, open.window.isPresented { prepare(open.controller) }
    let controller = present(
      \.viewer,
      controller: {
        let controller = parts?.viewerController(switchToHelper: viewerSwitch())
        if let controller, let panel = parts?.panel {
          controller.readingLoop.begin(with: panel.answer, picture: panel.picture)
        }
        if let controller { prepare(controller) }
        return controller
      },
      window: windows.viewer
    )
    if let tab { controller?.selectTab(tab) }
  }

  /// The Settings window; brought forward when already open. Its controller is made once and kept.
  func openSettings() {
    present(
      \.settings,
      controller: {
        let controller = settingsController ?? parts?.settingsController(links: settingsLinks())
        settingsController = controller
        return controller
      },
      window: windows.settings
    )
  }

  /// Check for Updates: Settings opens on About and checks.
  func checkForUpdates() {
    openSettings()
    settings?.controller.selectTab(.about)
    settings?.controller.send(.checkForUpdates)
  }

  /// The setup window; brought forward when already open. Once it closes, the panel reads Open at Login and the
  /// helper again, as setup may have changed either. Reopening waits for the old close to clear the helper's marker.
  func showOnboarding() {
    guard !closingWindows, !terminating else { return }
    if let onboarding, !onboarding.window.isPresented {
      reopeningOnboarding = true
      return
    }
    present(
      \.onboarding,
      controller: { parts?.onboardingController() },
      window: windows.onboarding,
      afterClose: { [weak self] in
        // Setup may have changed Open at Login, or set up a new helper.
        self?.parts?.panel?.readOpenAtLogin()
        // Unstructured, as the close callback is synchronous.
        Task { await self?.parts?.panel?.findHelper() }
        if self?.reopeningOnboarding == true {
          self?.reopeningOnboarding = false
          self?.showOnboarding()
        }
      }
    )
  }

  /// 2 s: the longest Atlas waits, quitting, for the service to answer the main window's release; past it Atlas quits
  /// anyway, and a lease left unanswered ends by itself within its lifetime (``ControlBarrier/leaseLifetime``).
  static let quitWait: Duration = .seconds(2)

  /// Closes every Atlas window and the panel, keeping the app and its menu bar item running. The main window stays
  /// open until the service has answered its release; unlike termination, closing windows has no release ceiling.
  func closeAllWindows() {
    guard !closingWindows else { return }
    closingWindows = true
    reopeningOnboarding = false
    // Unstructured, as the menu's action is synchronous: the windows close once the release is answered.
    Task {
      defer { closingWindows = false }
      repeat {
        releaseViewerControl()
        _ = await controlBarrier?.settle()
      } while viewerHoldsControl
      parts?.closePanel()
      viewer?.window.dismiss()
      settings?.window.dismiss()
      onboarding?.window.dismiss()
      alert?.window.dismiss()
      alert = nil
    }
  }

  /// Quit Atlas, from the panel's Quit: the main window's control goes back first, through the
  /// release every other path uses, and Atlas quits once the service has answered it, or after ``quitWait``.
  ///
  /// - Parameter sender: The menu item.
  func quit(_ sender: Any?) {
    terminating = true
    reopeningOnboarding = false
    // Unstructured, as the menu's action is synchronous: Atlas quits once the release is answered.
    Task {
      await releaseBeforeQuitting()
      parts?.terminate(sender)
    }
  }

  /// Allows AppKit's termination request, including Dock quit, log-out and shutdown, after a bounded release wait.
  ///
  /// - Parameter reply: Answers AppKit once the release wait ends.
  /// - Returns: Now when no control work is under way; later otherwise. Never cancels termination.
  func prepareToTerminate(reply: @escaping @MainActor () -> Void) -> NSApplication.TerminateReply {
    terminating = true
    reopeningOnboarding = false
    guard holdsUpQuitting else { return .terminateNow }
    // Unstructured, as AppKit asks synchronously: termination proceeds once the release wait ends.
    Task {
      await releaseBeforeQuitting()
      reply()
    }
    return .terminateLater
  }

  /// Whether control work is under way that quitting should wait for: the main window holds or asks for control, or a
  /// release it sent is not answered yet or was refused and needs retrying.
  var holdsUpQuitting: Bool {
    viewerHoldsControl || controlBarrier?.isBusy == true || controlBarrier?.unreleased.isEmpty == false
  }

  /// Gives the main window's control back and waits, at most ``quitWait``, for every release under way to be
  /// answered, a refused one sent once more: what quitting does first, however it was asked.
  func releaseBeforeQuitting() async {
    releaseViewerControl()
    guard let barrier = controlBarrier, barrier.isBusy || !barrier.unreleased.isEmpty else { return }
    let settling = Task { _ = await barrier.settle() }
    let clock = parts?.clock
    // Unstructured, so the ceiling runs beside the wait and calls it off once it passes.
    let ceiling = Task {
      try? await clock?.sleep(for: Self.quitWait)
      settling.cancel()
    }
    await settling.value
    ceiling.cancel()
  }

  /// The app's control work, which outlives any one main window: the open window's, else the app's own.
  private var controlBarrier: ControlBarrier? {
    viewer?.controller.barrier ?? parts?.controlBarrier
  }

  /// Whether the main window holds control or is asking for it now.
  var viewerHoldsControl: Bool {
    guard let controller = viewer?.controller else { return false }
    return controller.holdingControl || controller.control.acquiring
  }

  /// Ends the main window's control, if it holds it: before one of the helper's commands runs.
  func releaseViewerControl() {
    viewer?.controller.control.releaseControl()
  }

  /// Asks before one of the helper's commands runs, for the panel: in the main window while it is open, else in the
  /// alert window, which a newer alert replaces unless its command runs.
  ///
  /// - Parameter alert: The alert.
  func showAlert(_ alert: HelperAlert) {
    let routing = AlertRouting(
      viewerOpen: { [weak self] in self?.viewer?.window.isPresented == true },
      inViewer: { [weak self] alert in
        guard let viewer = self?.viewer else { return }
        viewer.window.present()
        viewer.controller.ask(Self.viewerAlert(alert))
      },
      inWindow: { [weak self] alert in self?.showAlertWindow(alert) }
    )
    routing.show(alert)
  }

  /// The alert window, for an alert while no main window is open; one at a time.
  ///
  /// - Parameter alert: The alert.
  private func showAlertWindow(_ alert: HelperAlert) {
    if let open = self.alert {
      open.controller.show(alert)
      open.window.present()
      return
    }
    guard let parts, let controller = parts.alertController(alert, links: alertLinks(clock: parts.clock)) else {
      return
    }
    let window = windows.alert(controller)
    self.alert = OpenWindow(window: window, controller: controller)
    window.present()
  }

  /// Closes the alert window its controller asks to close.
  ///
  /// - Parameter closing: The controller asking.
  func closeAlert(_ closing: AlertController) {
    guard let open = alert, open.controller === closing else { return }
    open.window.dismiss()
    alert = nil
  }

  /// The one way a window that shows the Dock icon opens: an open one comes forward; otherwise its controller is made,
  /// then the window, which clears its slot and settles the Dock icon once it closes; the Dock icon shows, then the
  /// window.
  ///
  /// - Parameters:
  ///   - slot: Where the coordinator keeps the window while it is open.
  ///   - controller: Makes the window's controller; nil leaves the window unopened, before launch.
  ///   - window: Makes the window over its controller.
  ///   - afterClose: Runs once the window has closed and its slot is clear.
  /// - Returns: The open window's controller, or nil when it could not open.
  @discardableResult
  private func present<Controller>(
    _ slot: ReferenceWritableKeyPath<WindowCoordinator, OpenWindow<Controller>?>,
    controller: () -> Controller?,
    window: (Controller, @escaping @MainActor (any WindowPresenting) -> Void) -> any WindowPresenting,
    afterClose: @escaping @MainActor () -> Void = {}
  ) -> Controller? {
    if let open = self[keyPath: slot], open.window.isPresented {
      open.window.present()
      return open.controller
    }
    guard let made = controller() else { return nil }
    let shown = window(made) { [weak self] closed in
      guard let self, self[keyPath: slot]?.window === closed else { return }
      self[keyPath: slot] = nil
      settleDockIcon()
      afterClose()
    }
    self[keyPath: slot] = OpenWindow(window: shown, controller: made)
    parts?.showDockIcon(true)
    shown.present()
    return made
  }

  /// A Dock icon while any window is open; none otherwise.
  private func settleDockIcon() {
    let open = [viewer?.window, onboarding?.window, settings?.window].contains { $0?.isPresented == true }
    parts?.showDockIcon(open)
  }

  /// The main window's alert for one the panel asks for.
  ///
  /// - Parameter alert: The panel's alert.
  /// - Returns: The same alert as the main window names it.
  private static func viewerAlert(_ alert: HelperAlert) -> ViewerAlert {
    switch alert {
    case .recover: .recover
    case .switchToHelper: .switchToHelper
    case .logOut: .logOut
    case .logOutAnyway: .logOutAnyway
    }
  }
}
