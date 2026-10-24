import AppKit
import AtlasCommands
import AtlasService
import AtlasUI

/// Atlas.app once it is set up: the menu bar item, and the setup window
/// when Getting Started… brings it back. The app has no Dock icon unless
/// a window is open (``DockIcon``).
@MainActor
final class AppController: NSObject, NSApplicationDelegate {
  /// The `atlas` command line every window runs.
  let commands: any CommandRunning
  /// The viewer service the panel and viewer read.
  let service: any ViewerService
  /// The app's login item, for the panel and the setup window.
  let loginItem: OpenAtLogin
  /// The helper's own commands, which the panel and the main window share, so one of each runs at a time.
  private(set) var actions: HelperActions?
  /// Connecting agents, which the setup window and Settings share, so one install runs at a time and outlives the
  /// window that started it.
  let installs: AgentInstalls
  /// The menu bar item and its panel.
  private(set) var menuBar: PanelPopover?
  /// The panel's controller, which the viewer opens on.
  private(set) var panel: PanelController?
  /// The setup window, while open.
  private var onboarding: OnboardingWindow?
  /// The viewer window, while open.
  private(set) var viewer: ViewerWindow?
  /// The Settings window, while open.
  private var settings: SettingsWindow?
  /// The Settings window's controller, kept for the app's life once made: an update check, an install or a Connect a
  /// closed window started runs on, and the next window waits for it rather than starting another.
  private var settingsController: SettingsController?
  /// The alert window, while open: the panel's alerts while no main window is.
  var alertWindow: AlertWindow?
  /// The notification and the Dock badge for when Atlas needs the person; made at launch.
  var signals: AttentionSignals?

  /// An app that runs `commands` and reads `service`.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` command line.
  ///   - service: The viewer service.
  ///   - loginItem: The app's login item.
  init(commands: any CommandRunning, service: any ViewerService, loginItem: OpenAtLogin) {
    self.commands = commands
    self.service = service
    self.loginItem = loginItem
    installs = AgentInstalls(commands: commands)
    super.init()
  }

  /// The app is quitting: the screenshots dragged out of the main window leave no copy behind.
  ///
  /// - Parameter notification: The app's notification.
  func applicationWillTerminate(_ notification: Notification) {
    ShotDrag.emptyFolder()
  }

  /// Sets up the menu and the menu bar panel.
  ///
  /// - Parameter notification: The app's notification.
  func applicationDidFinishLaunching(_ notification: Notification) {
    // One window of each kind: no Show Tab Bar in the Window menu.
    NSWindow.allowsAutomaticWindowTabbing = false
    NSApp.mainMenu = MainMenu.make(
      quit: (self, #selector(quitFromMenu(_:))),
      settings: (self, #selector(settingsFromMenu(_:))),
      checkForUpdates: (self, #selector(checkForUpdatesFromMenu(_:))),
      gettingStarted: (self, #selector(gettingStartedFromMenu(_:))),
      viewer: true
    )
    // The app's control work outlives any one main window: a release a closed window sent is still awaited by the
    // next command, and Take Control in a reopened window waits for it. Before a switch, a log-out or a restart, the
    // main window's control ends and its release joins the barrier.
    let barrier = ControlBarrier(service: service)
    let actions = HelperActions(commands: commands, service: service, barrier: barrier) { [weak self] in
      self?.viewer?.controller.releaseControl()
    }
    self.actions = actions
    startSignals()
    let controller = PanelController(
      service: service,
      commands: commands,
      actions: actions,
      loginItem: loginItem,
      links: panelLinks()
    )
    menuBar = PanelPopover(controller: controller) { [weak self] in self?.openSettings() }
    panel = controller
  }

  /// A line from `atlas menubar` on standard input.
  ///
  /// - Parameter line: The line, as read.
  func received(_ line: String) {
    switch line.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "viewer": openViewer()
    default: break
    }
  }

  /// The viewer window; one at a time, brought forward when already open, on `tab` when one is given.
  ///
  /// - Parameter tab: The tab to show, or nil for the one it shows.
  func openViewer(tab: ViewerTab? = nil) {
    if let viewer, viewer.isOpen {
      viewer.show()
      if let tab { viewer.controller.selectTab(tab) }
      return
    }
    guard let actions else { return }
    let controller = ViewerController(
      dependencies: .live(service: service, actions: actions, commands: commands) { [weak self] in
        await self?.panel?.logIn() ?? PanelWords.helperNotFound
      }
    )
    if let panel { controller.begin(with: panel.answer, picture: panel.picture) }
    let window = ViewerWindow(controller: controller) { [weak self] closed in
      guard let self, viewer === closed else { return }
      viewer = nil
      settleDockIcon()
    }
    viewer = window
    showDockIcon(true)
    window.show()
    if let tab { controller.selectTab(tab) }
  }

  /// A Dock icon while any window is open; none otherwise.
  private func settleDockIcon() {
    showDockIcon(viewer?.isOpen == true || onboarding?.isOpen == true || settings?.isOpen == true)
  }

  /// Standard input closed: the service is gone, so the app goes too.
  func parentClosed() {
    NSApp.terminate(nil)
  }

  /// Quit Atlas (⌘Q) in a window closes that window: the menu bar and the
  /// helper's service keep running. Quit Atlas in the panel, logging out
  /// and the service going away all quit.
  ///
  /// - Parameter sender: The menu item.
  @objc func quitFromMenu(_ sender: Any?) {
    if let alertWindow, alertWindow.isKeyWindow {
      alertWindow.controller.cancel()
    } else if let onboarding, onboarding.isOpen, onboarding.isKey {
      onboarding.close()
    } else if let settings, settings.isOpen, settings.isKey {
      settings.close()
    } else if let viewer, viewer.isOpen, viewer.isKey {
      viewer.close()
    } else {
      NSApp.terminate(sender)
    }
  }

  /// Settings… (⌘,) in the Atlas menu.
  ///
  /// - Parameter sender: The menu item.
  @objc func settingsFromMenu(_ sender: Any?) {
    openSettings()
  }

  /// Check for Updates in the Atlas menu: Settings opens on About and checks.
  ///
  /// - Parameter sender: The menu item.
  @objc func checkForUpdatesFromMenu(_ sender: Any?) {
    openSettings()
    settings?.controller.selectTab(.about)
    settings?.controller.send(.checkForUpdates)
  }

  /// The Settings window; one at a time, brought forward when already open. The panel's ⋯ menu opens it too.
  func openSettings() {
    if let settings, settings.isOpen {
      settings.show()
      return
    }
    guard let actions else { return }
    var dependencies = SettingsDependencies.live(
      commands: commands,
      actions: actions,
      installs: installs,
      links: settingsLinks()
    )
    dependencies.preferences = watchedPreferences()
    let controller = settingsController ?? SettingsController(dependencies: dependencies)
    settingsController = controller
    let window = SettingsWindow(controller: controller) { [weak self] closed in
      guard let self, settings === closed else { return }
      settings = nil
      settleDockIcon()
    }
    settings = window
    showDockIcon(true)
    window.show()
  }

  /// What Settings shares with the other windows: the panel's status and Open at Login, the main window's picture,
  /// or the rate saved for the next one while it is closed, switching to the helper, and the setup window.
  ///
  /// - Returns: The links.
  private func settingsLinks() -> SettingsLinks {
    let preferences = ViewerPreferences.standard
    return SettingsLinks(
      status: { [weak self] in
        guard let panel = self?.panel else { return (ViewerStatus(), true) }
        return (panel.status, panel.answered == nil)
      },
      readStatus: { [weak self] in await self?.panel?.readStatus() },
      opensAtLogin: { [weak self] in
        self?.panel?.opensAtLogin ?? OpenAtLogin.State(isAvailable: false, isEnabled: false)
      },
      setOpenAtLogin: { [weak self] on in await self?.panel?.setOpenAtLogin(on) },
      picture: { [weak self] in
        if let viewer = self?.viewer, viewer.isOpen {
          return ViewerPresenter.pictureChoice(viewer.controller.snapshot)
        }
        let rate = ViewerPictureRate(rawValue: preferences.loadRate()) ?? .live
        return rate == .everyFiveSeconds ? .everyFiveSeconds : .live
      },
      canPause: { [weak self] in self?.viewer?.isOpen == true },
      choosePicture: { [weak self] choice in
        if let viewer = self?.viewer, viewer.isOpen {
          viewer.controller.choosePicture(choice)
        } else if choice != .paused {
          preferences.saveRate((choice == .everyFiveSeconds ? ViewerPictureRate.everyFiveSeconds : .live).seconds)
        }
      },
      switchToHelper: { [weak self] in await self?.panel?.logIn() ?? PanelWords.helperNotFound },
      gettingStarted: { [weak self] in self?.showOnboarding() }
    )
  }

  /// Getting Started… in the Help menu.
  ///
  /// - Parameter sender: The menu item.
  @objc func gettingStartedFromMenu(_ sender: Any?) {
    showOnboarding()
  }

  /// The setup window; one at a time, brought forward when already open.
  func showOnboarding() {
    if let onboarding, onboarding.isOpen {
      onboarding.show()
      return
    }
    let controller = OnboardingController(
      dependencies: .live(commands: commands, loginItem: loginItem, installs: installs)
    )
    let window = OnboardingWindow(controller: controller) { [weak self] closed in
      guard let self, onboarding === closed else { return }
      onboarding = nil
      settleDockIcon()
      // Setup may have changed Open at Login, or set up a new helper.
      panel?.readOpenAtLogin()
      Task { await self.panel?.findHelper() }
    }
    onboarding = window
    showDockIcon(true)
    window.show()
  }
}
