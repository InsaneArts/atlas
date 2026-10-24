import AppKit

/// What Atlas shows outside its windows when it needs the person: the notification and the Dock badge
/// (``AttentionSignals``), fed by the panel's readings, the Dock icon and the choices in Settings.
extension AppContainer {
  /// Makes the signals at launch. Nothing asks for consent here: Settings asks when the person turns Notify me on.
  /// Clicking a notification, or its Open Atlas, opens the main window on Screen; its Recover recovers the helper.
  func startSignals() {
    signals = AttentionSignals(
      notifier: .live(
        open: { [weak coordinator] in coordinator?.openViewer(tab: .screen) },
        recover: { [weak self] in await self?.recoverFromNotification() }
      ),
      badge: .live,
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
      status: { try? await service.status() },
      holdingControl: { [weak coordinator] in coordinator?.viewerHoldsControl ?? false },
      readStatus: { [weak self] in await self?.panel?.readStatus() },
      open: { [weak coordinator] in coordinator?.openViewer(tab: .screen) },
      tell: { [weak self] reason in self?.signals?.tell(reason) }
    )
    await recovery.run()
  }

  /// Shows or hides the Dock icon, and settles its badge with it.
  ///
  /// - Parameter shown: Whether the Dock icon shows: while any window is open.
  func showDockIcon(_ shown: Bool) {
    DockIcon.settle(windowOpen: shown)
    signals?.dockIcon(shown: shown)
  }

  /// Settings' saved choices, which also tell the signals when Notify me or the badge changes, so turning Notify me
  /// on asks for consent and the badge follows its choice at once.
  ///
  /// - Returns: The preferences.
  func watchedPreferences() -> SettingsPreferences {
    var preferences = settingsPreferences
    let saveNotify = preferences.saveNotify
    let saveBadge = preferences.saveBadge
    preferences.saveNotify = { [weak self] on in
      saveNotify(on)
      self?.signals?.notifyChanged(on)
    }
    preferences.saveBadge = { [weak self] on in
      saveBadge(on)
      self?.signals?.badgeChanged()
    }
    return preferences
  }
}
