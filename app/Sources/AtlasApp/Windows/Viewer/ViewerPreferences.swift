import Foundation

/// The viewer choices that outlast a window: how often a picture is read and whether the sidebar shows. Handed to
/// the controller, so tests keep them in memory and never write the real defaults.
struct ViewerPreferences {
  /// The defaults key of the picture rate, in seconds. Shipped, so it never changes.
  static let rateKey = "viewer.rate"
  /// The defaults key of whether the sidebar shows; it keeps the sidebar's old name. Shipped, so it never changes.
  static let sidebarKey = "viewer.inspector"

  /// The saved picture rate, in seconds; 0 when none is saved.
  var loadRate: @MainActor () -> Double
  /// Saves the picture rate, in seconds.
  var saveRate: @MainActor (Double) -> Void
  /// Whether the sidebar was left showing; false when nothing is saved.
  var loadSidebar: @MainActor () -> Bool
  /// Saves whether the sidebar shows.
  var saveSidebar: @MainActor (Bool) -> Void

  /// The app's own, in its standard defaults.
  @MainActor static var standard: ViewerPreferences { stored(in: .standard) }

  /// Preferences kept in `defaults` under the shipped keys.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  @MainActor static func stored(in defaults: UserDefaults) -> ViewerPreferences {
    ViewerPreferences(
      loadRate: { defaults.double(forKey: rateKey) },
      saveRate: { defaults.set($0, forKey: rateKey) },
      loadSidebar: { defaults.bool(forKey: sidebarKey) },
      saveSidebar: { defaults.set($0, forKey: sidebarKey) }
    )
  }
}
