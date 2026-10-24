import AppKit
import AtlasService
import AtlasUI

/// The menu bar panel's links to the other windows, and the alerts it asks for: in the main window while it is open,
/// else in the alert window (``AlertRouting``).
extension AppController {
  /// What the panel asks of the other windows: each closes the panel first, as opening anything from a menu does.
  ///
  /// - Returns: The links.
  func panelLinks() -> PanelLinks {
    PanelLinks(
      openViewer: { [weak self] tab in
        self?.menuBar?.closePanel()
        self?.openViewer(tab: tab)
      },
      openSettings: { [weak self] in
        self?.menuBar?.closePanel()
        self?.openSettings()
      },
      showAlert: { [weak self] alert in
        self?.menuBar?.closePanel()
        self?.showAlert(alert)
      },
      holdingControl: { [weak self] in
        guard let viewer = self?.viewer, viewer.isOpen else { return false }
        return viewer.controller.holdingControl
      },
      giveBack: { [weak self] in self?.viewer?.controller.releaseControl() },
      quit: { NSApp.terminate(nil) },
      read: { [weak self] status in self?.signals?.read(status) }
    )
  }

  /// Asks before one of the helper's commands runs, for the panel: in the main window while it is open, else in the
  /// alert window, which a newer alert replaces unless its command runs.
  ///
  /// - Parameter alert: The alert.
  func showAlert(_ alert: HelperAlert) {
    let routing = AlertRouting(
      viewerOpen: { [weak self] in self?.viewer?.isOpen == true },
      inViewer: { [weak self] alert in
        guard let viewer = self?.viewer else { return }
        viewer.show()
        viewer.controller.ask(Self.viewerAlert(alert))
      },
      inWindow: { [weak self] alert in self?.showAlertWindow(alert) }
    )
    routing.show(alert)
  }

  /// The alert window, for an alert while no main window is open; one at a time.
  ///
  /// - Parameter alert: The alert.
  func showAlertWindow(_ alert: HelperAlert) {
    if let alertWindow {
      alertWindow.controller.show(alert)
      alertWindow.show()
      return
    }
    guard let actions else { return }
    let controller = AlertController(
      alert: alert,
      actions: actions,
      links: AlertLinks(
        overview: { [weak self] in self?.panel?.status.overview ?? ViewerStatus.Overview() },
        readStatus: { [weak self] in await self?.panel?.readStatus() },
        switchToHelper: { [weak self] in await self?.panel?.switchToHelper() },
        close: { [weak self] closing in
          guard let self, let window = alertWindow, window.controller === closing else { return }
          window.close()
          alertWindow = nil
        }
      )
    )
    let window = AlertWindow(controller: controller)
    alertWindow = window
    window.show()
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
