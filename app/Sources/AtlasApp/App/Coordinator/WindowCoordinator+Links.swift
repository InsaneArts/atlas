import AppKit
import AtlasService
import AtlasUI

/// Every window's links to the others, built here alone: the panel's (each closes the panel first, as opening anything
/// from a menu does), Settings', the alert window's, and the main window's switch to the helper.
extension WindowCoordinator {
  /// What the panel asks of the other windows.
  ///
  /// - Returns: The links.
  func panelLinks() -> PanelLinks {
    PanelLinks(
      openViewer: { [weak self] tab in
        self?.parts?.closePanel()
        self?.openViewer(tab: tab)
      },
      openSettings: { [weak self] in
        self?.parts?.closePanel()
        self?.openSettings()
      },
      showAlert: { [weak self] alert in
        self?.parts?.closePanel()
        self?.showAlert(alert)
      },
      holdingControl: { [weak self] in
        guard let viewer = self?.viewer, viewer.window.isPresented else { return false }
        return viewer.controller.holdingControl
      },
      giveBack: { [weak self] in self?.releaseViewerControl() },
      quit: { [weak self] in self?.parts?.terminate(nil) },
      read: { [weak self] status in self?.parts?.read(status) }
    )
  }

  /// What Settings shares with the other windows: the panel's status and Open at Login, the main window's picture, or
  /// the rate saved for the next one while it is closed (``SettingsLinkRules``), switching to the helper, and the
  /// setup window.
  ///
  /// - Returns: The links.
  func settingsLinks() -> SettingsLinks {
    let preferences = viewerPreferences
    return SettingsLinks(
      status: { [weak self] in
        guard let panel = self?.parts?.panel else { return (ViewerStatus(), true) }
        return (panel.status, panel.answered == nil)
      },
      readStatus: { [weak self] in await self?.parts?.panel?.readStatus() },
      opensAtLogin: { [weak self] in
        self?.parts?.panel?.opensAtLogin ?? OpenAtLogin.State(isAvailable: false, isEnabled: false)
      },
      setOpenAtLogin: { [weak self] on in await self?.parts?.panel?.setOpenAtLogin(on) },
      picture: { [weak self] in
        SettingsLinkRules.picture(viewer: self?.openViewerController, savedRate: preferences.loadRate())
      },
      canPause: { [weak self] in self?.viewer?.window.isPresented == true },
      choosePicture: { [weak self] choice in
        SettingsLinkRules.choose(choice, viewer: self?.openViewerController, preferences: preferences)
      },
      switchToHelper: viewerSwitch(),
      gettingStarted: { [weak self] in self?.showOnboarding() }
    )
  }

  /// What the alert window asks of the rest of the app: the panel's status, and the window's own closing.
  ///
  /// - Parameter clock: The clock the window's waits run on.
  /// - Returns: The links.
  func alertLinks(clock: any Clock<Duration>) -> AlertLinks {
    AlertLinks(
      overview: { [weak self] in self?.parts?.panel?.status.overview ?? ViewerStatus.Overview() },
      readStatus: { [weak self] in await self?.parts?.panel?.readStatus() },
      switchToHelper: { [weak self] in await self?.parts?.panel?.switchToHelper() },
      close: { [weak self] closing in self?.closeAlert(closing) },
      clock: clock
    )
  }

  /// Locks the screen so the person can log in to the helper, through the panel, returning what to tell them: the
  /// main window's and Settings' Switch to the Helper.
  ///
  /// - Returns: The switch.
  func viewerSwitch() -> @MainActor () async -> String {
    { [weak self] in await self?.parts?.panel?.logIn() ?? PanelWords.helperNotFound }
  }

  /// The open main window's controller; nil while none is open.
  private var openViewerController: ViewerController? {
    guard let viewer, viewer.window.isPresented else { return nil }
    return viewer.controller
  }
}
