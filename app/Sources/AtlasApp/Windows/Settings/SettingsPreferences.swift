import Foundation

/// Settings' choices that outlast the window: the pane last shown, and whether Atlas notifies the person and badges
/// its Dock icon when it needs them, which ``AttentionSignals`` reads as it applies them. Handed to the controller, so
/// tests keep them in memory and never write the real defaults.
struct SettingsPreferences {
  /// The defaults key of the pane last shown, by its raw value. Shipped, so it never changes.
  static let tabKey = "settings.tab"
  /// The defaults key of Notify me. Shipped, so it never changes.
  static let notifyKey = "settings.notify"
  /// The defaults key of the Dock badge. Shipped, so it never changes.
  static let badgeKey = "settings.badge"

  /// The pane last shown, by its raw value; nil when none is saved.
  var loadTab: @MainActor () -> String?
  /// Saves the pane shown, by its raw value.
  var saveTab: @MainActor (String) -> Void
  /// Whether Notify me is on; off when nothing is saved, so notifications wait until the person asks for them.
  var loadNotify: @MainActor () -> Bool
  /// Saves Notify me.
  var saveNotify: @MainActor (Bool) -> Void
  /// Whether the Dock badge is on; off when nothing is saved.
  var loadBadge: @MainActor () -> Bool
  /// Saves the Dock badge.
  var saveBadge: @MainActor (Bool) -> Void

  /// The app's own, in its standard defaults.
  static var standard: SettingsPreferences { stored(in: .standard) }

  /// Preferences kept in `defaults` under the shipped keys.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  static func stored(in defaults: UserDefaults) -> SettingsPreferences {
    SettingsPreferences(
      loadTab: { defaults.string(forKey: tabKey) },
      saveTab: { defaults.set($0, forKey: tabKey) },
      loadNotify: { defaults.bool(forKey: notifyKey) },
      saveNotify: { defaults.set($0, forKey: notifyKey) },
      loadBadge: { defaults.bool(forKey: badgeKey) },
      saveBadge: { defaults.set($0, forKey: badgeKey) }
    )
  }
}
