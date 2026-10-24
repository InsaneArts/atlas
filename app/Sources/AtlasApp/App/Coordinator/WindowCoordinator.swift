import AppKit
import AtlasUI

/// The one object that knows more than one window: it opens each window, brings an open one forward, closes the key
/// window on ⌘Q, routes the panel's alerts, and builds every window's links to the others (`WindowCoordinator+Links`).
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

  /// Creates the coordinator, with no window open.
  ///
  /// - Parameters:
  ///   - windows: How each window is made.
  ///   - viewerPreferences: The main window's saved choices.
  init(windows: WindowMaking, viewerPreferences: ViewerPreferences) {
    self.windows = windows
    self.viewerPreferences = viewerPreferences
  }

  /// The main window; brought forward when already open, on `tab` when one is given.
  ///
  /// - Parameter tab: The tab to show, or nil for the one it shows.
  func openViewer(tab: ViewerTab? = nil) {
    let controller = present(
      \.viewer,
      controller: {
        let controller = parts?.viewerController(switchToHelper: viewerSwitch())
        if let controller, let panel = parts?.panel { controller.begin(with: panel.answer, picture: panel.picture) }
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
  /// helper again, as setup may have changed either.
  func showOnboarding() {
    present(
      \.onboarding,
      controller: { parts?.onboardingController() },
      window: windows.onboarding,
      afterClose: { [weak self] in
        // Setup may have changed Open at Login, or set up a new helper.
        self?.parts?.panel?.readOpenAtLogin()
        // Unstructured, as the close callback is synchronous.
        Task { await self?.parts?.panel?.findHelper() }
      }
    )
  }

  /// Quit Atlas (⌘Q) in a window closes that window: the menu bar and the helper's service keep running. The alert
  /// window's Escape is its Cancel. Quit Atlas in the panel, logging out and the service going away all quit.
  ///
  /// - Parameter sender: The menu item.
  func quit(_ sender: Any?) {
    if let alert, alert.window.isKeyWindow {
      alert.controller.cancel()
    } else if let onboarding, onboarding.window.isPresented, onboarding.window.isKeyWindow {
      onboarding.window.dismiss()
    } else if let settings, settings.window.isPresented, settings.window.isKeyWindow {
      settings.window.dismiss()
    } else if let viewer, viewer.window.isPresented, viewer.window.isKeyWindow {
      viewer.window.dismiss()
    } else {
      parts?.terminate(sender)
    }
  }

  /// Whether the main window holds control or is asking for it now.
  var viewerHoldsControl: Bool {
    guard let controller = viewer?.controller else { return false }
    return controller.holdingControl || controller.session.acquiring
  }

  /// Ends the main window's control, if it holds it: before one of the helper's commands runs.
  func releaseViewerControl() {
    viewer?.controller.releaseControl()
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
