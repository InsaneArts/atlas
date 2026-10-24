import AppKit

/// Atlas.app once it is set up, as AppKit sees it: launch, the lines `atlas menubar` sends, termination, and the
/// main menu's own actions, each handed to the coordinator. Everything the app runs is the container's
/// (``AppContainer``). The app has no Dock icon unless a window is open (``DockIcon``).
final class AppDelegate: NSObject, NSApplicationDelegate {
  /// Everything the running app built.
  let container: AppContainer

  /// A delegate over the app's parts.
  ///
  /// - Parameter container: Everything the running app builds; started at launch.
  init(container: AppContainer) {
    self.container = container
    super.init()
  }

  /// Sets up the menu, then builds the menu bar panel and the rest of the app's live parts.
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
    container.start()
  }

  /// The app is quitting: the screenshots dragged out of the main window leave no copy behind.
  ///
  /// - Parameter notification: The app's notification.
  func applicationWillTerminate(_ notification: Notification) {
    ShotDrag.emptyFolder()
  }

  /// A line from `atlas menubar` on standard input: `viewer` opens the main window.
  ///
  /// - Parameter line: The line, as read.
  func received(_ line: String) {
    switch line.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "viewer": container.coordinator.openViewer()
    default: break
    }
  }

  /// Standard input closed: the service is gone, so the app goes too.
  func parentClosed() {
    NSApp.terminate(nil)
  }

  /// Quit Atlas (⌘Q): in a window it closes that window (``WindowCoordinator/quit(_:)``).
  ///
  /// - Parameter sender: The menu item.
  @objc func quitFromMenu(_ sender: Any?) {
    container.coordinator.quit(sender)
  }

  /// Settings… (⌘,) in the Atlas menu.
  ///
  /// - Parameter sender: The menu item.
  @objc func settingsFromMenu(_ sender: Any?) {
    container.coordinator.openSettings()
  }

  /// Check for Updates in the Atlas menu: Settings opens on About and checks.
  ///
  /// - Parameter sender: The menu item.
  @objc func checkForUpdatesFromMenu(_ sender: Any?) {
    container.coordinator.checkForUpdates()
  }

  /// Getting Started… in the Help menu.
  ///
  /// - Parameter sender: The menu item.
  @objc func gettingStartedFromMenu(_ sender: Any?) {
    container.coordinator.showOnboarding()
  }
}
