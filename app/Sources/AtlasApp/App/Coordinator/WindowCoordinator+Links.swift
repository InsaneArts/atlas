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
      openViewer: { [weak self] place in
        self?.parts?.closePanel()
        self?.openViewer(place: place)
      },
      openSessions: { [weak self] choice in
        self?.parts?.closePanel()
        self?.openViewer(place: .activity, session: choice)
      },
      openSettings: { [weak self] in
        self?.parts?.closePanel()
        self?.openSettings()
        self?.settings?.controller.select(.helper)
      },
      openAccounts: { [weak self] in
        self?.parts?.closePanel()
        self?.openSettings()
        self?.settings?.controller.select(.accounts)
      },
      showAlert: { [weak self] alert in
        self?.parts?.closePanel()
        self?.showAlert(alert)
      },
      closePanel: { [weak self] in self?.parts?.closePanel() },
      holdingControl: { [weak self] in
        guard let viewer = self?.viewer, viewer.window.isPresented else { return false }
        return viewer.controller.holdingControl
      },
      giveBack: { [weak self] in self?.releaseViewerControl() },
      closeWindows: { [weak self] in self?.closeAllWindows() },
      quit: { [weak self] in self?.quit(nil) },
      read: { [weak self] status in self?.parts?.read(status) }
    )
  }

  /// What Settings shares with the other windows: the panel's status and Open at Login, the menu bar's glyph, the main
  /// window's picture, or the rate saved for the next one while it is closed (``SettingsLinkRules``), switching to the
  /// helper, and the setup window.
  ///
  /// - Returns: The links.
  func settingsLinks() -> SettingsLinks {
    let preferences = viewerPreferences
    return SettingsLinks(
      status: { [weak self] in
        guard let panel = self?.parts?.panel else { return (ViewerStatus(), true) }
        return (panel.status, panel.connecting)
      },
      readStatus: { [weak self] in await self?.parts?.panel?.readStatus() },
      opensAtLogin: { [weak self] in
        self?.parts?.panel?.opensAtLogin ?? OpenAtLogin.State(isAvailable: false, isEnabled: false)
      },
      setOpenAtLogin: { [weak self] on in await self?.parts?.panel?.setOpenAtLogin(on) },
      menuBarIcon: { [weak self] in self?.parts?.menuBarIcon ?? .orbit },
      chooseMenuBarIcon: { [weak self] icon in self?.parts?.useMenuBarIcon(icon) },
      picture: { [weak self] in
        SettingsLinkRules.picture(viewer: self?.openViewerController, savedRate: preferences.loadRate())
      },
      canPause: { [weak self] in self?.viewer?.window.isPresented == true },
      choosePicture: { [weak self] choice in
        SettingsLinkRules.choose(choice, viewer: self?.openViewerController, preferences: preferences)
      },
      switchToHelper: viewerSwitch(),
      gettingStarted: { [weak self] in self?.showOnboarding() },
      addAccount: { [weak self] app, name in
        guard let service = self?.parts?.panel?.service else { throw ServiceError(SettingsWords.noService) }
        try await service.addAccount(app: app, name: name)
      },
      removeAccount: { [weak self] account in
        guard let service = self?.parts?.panel?.service else { throw ServiceError(SettingsWords.noService) }
        try await service.removeAccount(account)
      },
      setLevel: { [weak self] level, account in
        guard let service = self?.parts?.panel?.service else { throw ServiceError(SettingsWords.noService) }
        try await service.setLevel(level, of: account)
      },
      startSignIn: { [weak self] purpose, site, browser in
        guard let self else { return SignInWords.noWindow }
        return await startSignIn(purpose, site: site, browser: browser)
      }
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

  /// Starts a held sign-in for Settings in the main window, which comes forward on the Screen, where the person signs
  /// in with control, only once it started. An open main window stays where it is until then; a closed one has to
  /// open to hold it, and Settings comes back in front should the start be refused, so the sheet's refusal is seen.
  ///
  /// - Parameters:
  ///   - purpose: What it is for.
  ///   - site: The site.
  ///   - browser: The browser to open; the helper's own when nil.
  /// - Returns: Why it couldn't start; nil when it did.
  private func startSignIn(_ purpose: SignInPurpose, site: String, browser: String?) async -> String? {
    let opened = openViewerController == nil
    if opened { openViewer(place: .screen) }
    guard let controller = openViewerController else { return SignInWords.noWindow }
    guard let refusal = await controller.startSignIn(purpose, site: site, browser: browser) else {
      openViewer(place: .screen)
      return nil
    }
    if opened { settings?.window.present() }
    return refusal
  }

  /// The open main window's controller; nil while none is open.
  private var openViewerController: ViewerController? {
    guard let viewer, viewer.window.isPresented else { return nil }
    return viewer.controller
  }
}
