import AppKit
import AtlasKit

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
  /// The menu bar item and its panel.
  private var menuBar: PanelPopover?
  /// The panel's controller, which the viewer opens on.
  private var panel: PanelController?
  /// The setup window, while open.
  private var onboarding: OnboardingWindow?
  /// The viewer window, while open.
  private var viewer: ViewerWindow?

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
    super.init()
  }

  /// Sets up the menu and the menu bar panel.
  ///
  /// - Parameter notification: The app's notification.
  func applicationDidFinishLaunching(_ notification: Notification) {
    // One window of each kind: no Show Tab Bar in the Window menu.
    NSWindow.allowsAutomaticWindowTabbing = false
    NSApp.mainMenu = MainMenu.make(quit: (self, #selector(quitFromMenu(_:))))
    let controller = PanelController(
      service: service,
      commands: commands,
      loginItem: loginItem,
      gettingStarted: { [weak self] in self?.showOnboarding() },
      openViewer: { [weak self] in
        self?.menuBar?.closePanel()
        self?.showViewer()
      },
      quit: { NSApp.terminate(nil) }
    )
    menuBar = PanelPopover(controller: controller)
    panel = controller
  }

  /// A line from `atlas menubar` on standard input.
  ///
  /// - Parameter line: The line, as read.
  func received(_ line: String) {
    switch line.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "viewer": showViewer()
    default: break
    }
  }

  /// The viewer window; one at a time, brought forward when already open.
  func showViewer() {
    if let viewer, viewer.isOpen {
      viewer.show()
      return
    }
    let controller = ViewerController(
      service: service,
      preferences: .standard,
      clock: { ProcessInfo.processInfo.systemUptime },
      heartbeatInterval: ViewerController.heartbeatEvery,
      nap: ViewerController.sleep,
      logIn: { [weak self] in await self?.panel?.logIn() ?? PanelWords.helperNotFound },
      showKeys: KeysMenu.show(for:),
      doubleClickInterval: { NSEvent.doubleClickInterval }
    )
    if let panel { controller.begin(with: panel.answer, picture: panel.picture) }
    let window = ViewerWindow(controller: controller) { [weak self] closed in
      guard let self, viewer === closed else { return }
      viewer = nil
      settleDockIcon()
    }
    viewer = window
    DockIcon.settle(windowOpen: true)
    window.show()
  }

  /// A Dock icon while any window is open; none otherwise.
  private func settleDockIcon() {
    DockIcon.settle(windowOpen: viewer?.isOpen == true || onboarding?.isOpen == true)
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
    if let onboarding, onboarding.isOpen, onboarding.isKey {
      onboarding.close()
    } else if let viewer, viewer.isOpen, viewer.isKey {
      viewer.close()
    } else {
      NSApp.terminate(sender)
    }
  }

  /// The setup window; one at a time, brought forward when already open.
  func showOnboarding() {
    if let onboarding, onboarding.isOpen {
      onboarding.show()
      return
    }
    let controller = OnboardingController(dependencies: .live(commands: commands, loginItem: loginItem))
    let window = OnboardingWindow(controller: controller) { [weak self] closed in
      guard let self, onboarding === closed else { return }
      onboarding = nil
      settleDockIcon()
      // Setup may have changed Open at Login, or set up a new helper.
      panel?.readOpenAtLogin()
      Task { await self.panel?.findHelper() }
    }
    onboarding = window
    DockIcon.settle(windowOpen: true)
    window.show()
  }
}
