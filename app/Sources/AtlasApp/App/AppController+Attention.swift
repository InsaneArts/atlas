import AppKit
import AtlasKit

/// What Atlas shows outside its windows when it needs the person: the notification and the Dock badge
/// (``AttentionSignals``), fed by the panel's readings, the Dock icon and the choices in Settings.
extension AppController {
  /// Makes the signals at launch. Nothing asks for consent here: Settings asks when the person turns Notify me on.
  /// Clicking a notification opens the main window on Screen.
  func startSignals() {
    signals = AttentionSignals(
      notifier: .live { [weak self] in self?.openViewer(tab: .screen) },
      badge: .live,
      preferences: .standard
    )
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
    var preferences = SettingsPreferences.standard
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
