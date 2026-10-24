import AtlasService

/// What the coordinator asks of the app's live parts: the controllers it opens its windows on, and what the windows'
/// links reach outside the windows. ``AppContainer`` is the live one; tests hand in their own.
protocol WindowParts: AnyObject {
  /// The panel's controller, which the main window opens on and the links read; nil before launch has made it.
  var panel: PanelController? { get }
  /// The clock the windows' waits run on.
  var clock: any Clock<Duration> { get }
  /// The app's control work not yet answered, which quitting waits for; nil before launch has made it.
  var controlBarrier: ControlBarrier? { get }

  /// A main window's controller.
  ///
  /// - Parameter switchToHelper: Locks the screen for the helper, returning what to tell the person.
  /// - Returns: The controller, or nil before launch has made the helper's commands.
  func viewerController(switchToHelper: @escaping @MainActor () async -> String) -> ViewerController?

  /// The Settings window's controller.
  ///
  /// - Parameter links: What Settings shares with the other windows.
  /// - Returns: The controller, or nil before launch has made the helper's commands.
  func settingsController(links: SettingsLinks) -> SettingsController?

  /// The setup window's controller.
  ///
  /// - Returns: The controller.
  func onboardingController() -> OnboardingController

  /// The alert window's controller, for one alert.
  ///
  /// - Parameters:
  ///   - alert: The alert it asks first.
  ///   - links: What the alert window asks of the rest of the app.
  /// - Returns: The controller, or nil before launch has made the helper's commands.
  func alertController(_ alert: HelperAlert, links: AlertLinks) -> AlertController?

  /// Shows or hides the Dock icon: shown while any window is open.
  ///
  /// - Parameter shown: Whether it shows.
  func showDockIcon(_ shown: Bool)

  /// Closes the menu bar panel, as opening a window or an alert from it does.
  func closePanel()

  /// Quits Atlas, once the coordinator has given the main window's control back.
  ///
  /// - Parameter sender: What asked, as AppKit's `terminate(_:)` takes it.
  func terminate(_ sender: Any?)

  /// Puts what `atlas doctor` says on the clipboard, for a bug report.
  ///
  /// - Returns: Whether it was read and copied.
  func copyDiagnostics() async -> Bool

  /// Hands the panel's latest reading to the notifications.
  ///
  /// - Parameter status: The reading, or nil when it failed.
  func read(_ status: ViewerStatus?)
}
