import AppKit

/// What Atlas shows outside its windows when it needs the person: the notification (``AttentionSignals``), fed by
/// the panel's readings and the choices in Settings.
extension AppContainer {
  /// Makes the signals at launch. Nothing asks for consent here: Settings asks when the person turns Notify me on.
  /// Clicking a notification opens the main window on Screen; its Recover recovers the helper.
  func startSignals() {
    signals = AttentionSignals(
      notifier: .live(
        open: { [weak coordinator] in coordinator?.openViewer(place: .screen) },
        recover: { [weak self] in await self?.recoverFromNotification() },
        // On screen, not merely open: a minimised or covered main window says nothing.
        viewerShows: { [weak coordinator] in
          guard let window = coordinator?.viewer?.window as? NSWindow else { return false }
          return WindowVisibility.shows(isVisible: window.isVisible, occlusionState: window.occlusionState)
        }
      ),
      preferences: settingsPreferences
    )
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
