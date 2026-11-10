import AppKit
import AtlasStatus

/// What Atlas shows outside its windows when it needs the person: the notification (``AttentionSignals``), fed by
/// the panel's readings and the choices in Settings.
extension AppContainer {
  /// Makes the signals at launch. Nothing asks for consent here: Settings asks when the person turns Notify me on, and
  /// the questions' signals when an account joins the list.
  /// Clicking a notification opens the main window on Screen; its Recover recovers the helper.
  func startSignals() {
    signals = AttentionSignals(
      notifier: .live(
        open: { [weak coordinator] in coordinator?.openViewer(place: .screen) },
        recover: { [weak self] in await self?.recoverFromNotification() },
        // On screen, not merely open: a minimised or covered main window says nothing.
        viewerShows: { [weak self] in self?.viewerOnScreen() ?? false },
        answer: { [weak self] id, answer in await self?.actions?.asks.answer(id, with: answer) },
        askInView: { [weak self] in self?.askInView() }
      ),
      preferences: settingsPreferences
    )
    startAskSignals()
  }

  /// Makes the questions' signals at launch and starts watching the asks folders, so a question posts its
  /// notification at once with the panel closed. macOS is asked whether Atlas may notify the person only once an
  /// account joins the list (``AskSignals/statusRead(_:)``).
  func startAskSignals() {
    guard let store = actions?.asks else { return }
    let signals = AskSignals(
      notifications: .live(),
      preferences: settingsPreferences,
      store: store,
      inView: { [weak self] in self?.askInView() }
    )
    askSignals = signals
    store.changed = { [weak signals] list in signals?.asksChanged(list) }
    store.watch(.live(home: FileManager.default.homeDirectoryForCurrentUser))
    watchLookAways()
    // Unstructured, as nothing at launch can wait: the badge follows what macOS last said.
    Task { await signals.readConsent() }
  }

  /// Tells the questions' signals each time the person may have looked away from a question: the panel closing, a
  /// window minimised, covered, closing or no longer the one they type in, or Atlas no longer the app in front. The main
  /// window leaving the Screen tells them through its own dependency.
  func watchLookAways() {
    let names: [Notification.Name] = [
      NSPopover.didCloseNotification,
      NSWindow.didMiniaturizeNotification,
      NSWindow.didChangeOcclusionStateNotification,
      NSWindow.willCloseNotification,
      NSWindow.didResignKeyNotification,
      NSApplication.didResignActiveNotification,
    ]
    lookAways = names.map { name in
      NotificationCenter.default
        .addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
          // On the next turn, once the window that is closing has gone from the screen.
          Task { @MainActor in self?.askSignals?.lookedAway() }
        }
    }
  }

  /// Whether the main window is on screen: not merely open, as a minimised or covered one says nothing.
  ///
  /// - Returns: Whether it shows.
  func viewerOnScreen() -> Bool {
    guard let window = coordinator.viewer?.window as? NSWindow else { return false }
    return WindowVisibility.shows(isVisible: window.isVisible, occlusionState: window.occlusionState)
  }

  /// The question the person sees now: the oldest one up, which the open panel's card shows, and the banner over the
  /// main window's Screen while that window is the one they are in, in front and showing the Screen; nil while neither
  /// shows it, as on Activity, behind another app's window or with Atlas in the background, where a banner is easily
  /// missed and the question notifies instead.
  ///
  /// - Returns: The question's id, or nil.
  func askInView() -> String? {
    guard let store = actions?.asks, let panel else { return nil }
    let panelShows = menuBar?.popover.isShown == true
    let window = coordinator.viewer?.window as? NSWindow
    let screenShows =
      NSApp.isActive && window?.isKeyWindow == true && viewerOnScreen()
      && coordinator.viewer?.controller.session.place == .screen
    guard panelShows || screenShows else { return nil }
    return StatusReading(panel.status)
      .asks(from: store.list, at: Date())
      .first?
      .id
  }

  /// Atlas needs you's Recover: the alert's own recovery (``NotificationRecovery``) while Needs you still holds and
  /// no window holds control, else the main window opens; then the panel reads the status, and a refusal is told in a
  /// second notification.
  func recoverFromNotification() async {
    guard let actions else { return }
    let service = service
    let recovery = NotificationRecovery(
      actions: actions,
      // A status that cannot be read is nil, and the recovery then only opens Atlas.
      status: { try? await service.status() },
      holdingControl: { [weak coordinator] in coordinator?.viewerHoldsControl ?? false },
      readStatus: { [weak self] in await self?.panel?.readStatus() },
      open: { [weak coordinator] in coordinator?.openViewer(place: .screen) },
      tell: { [weak self] reason in self?.signals?.tell(reason) }
    )
    await recovery.run()
  }

  /// Shows or hides the Dock icon.
  ///
  /// - Parameter shown: Whether the Dock icon shows: while any window is open.
  func showDockIcon(_ shown: Bool) {
    DockIcon.settle(windowOpen: shown)
  }

  /// Settings' saved choices, which also tell the signals when Notify me changes, so turning it on asks for consent.
  ///
  /// - Returns: The preferences.
  func watchedPreferences() -> SettingsPreferences {
    var preferences = settingsPreferences
    let saveNotify = preferences.saveNotify
    preferences.saveNotify = { [weak self] on in
      saveNotify(on)
      self?.signals?.notifyChanged(on)
    }
    return preferences
  }
}
