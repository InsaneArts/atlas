import AtlasUI
import Foundation

/// The menu bar item's one choice that outlasts it: the glyph the person picked in Settings. Read once at launch, so
/// the item never shows the Orbit before the Screen it was left on, and saved on each change. Handed to the app's
/// container, so tests keep it in memory and never write the real defaults.
struct MenuBarPreferences {
  /// The defaults key of the glyph, by its raw value. Shipped, so it never changes.
  static let iconKey = "menubar.icon"

  /// The glyph picked; the Orbit when nothing, or nothing Atlas knows, is saved.
  var loadIcon: @MainActor () -> MenuBarIcon
  /// Saves the glyph picked.
  var saveIcon: @MainActor (MenuBarIcon) -> Void

  /// Preferences kept in `defaults` under the shipped key.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  static func stored(in defaults: UserDefaults) -> MenuBarPreferences {
    MenuBarPreferences(
      loadIcon: {
        defaults.string(forKey: iconKey)
          .flatMap(MenuBarIcon.init(rawValue:)) ?? .orbit
      },
      saveIcon: { defaults.set($0.rawValue, forKey: iconKey) }
    )
  }
}
