import Foundation

/// The main window's choices that outlast it: how often a picture is read, whether Details shows, and the corner the
/// small screen keeps to. The main window reads and writes them, and Settings writes the rate while no main window is
/// open, so the two agree. Handed to each controller, so tests keep them in memory and never write the real defaults.
struct ViewerPreferences {
  /// The defaults key of the picture rate, in seconds. Shipped, so it never changes.
  static let rateKey = "viewer.rate"
  /// The defaults key of whether Details shows; it keeps the column's first name. Shipped, so it never changes.
  static let detailsKey = "viewer.inspector"
  /// The defaults key of the small screen's corner, by its name. Shipped, so it never changes.
  static let cornerKey = "viewer.corner"

  /// The saved picture rate, in seconds; 0 when none is saved.
  var loadRate: @MainActor () -> Double
  /// Saves the picture rate, in seconds.
  var saveRate: @MainActor (Double) -> Void
  /// Whether Details was left showing; false when nothing is saved.
  var loadDetails: @MainActor () -> Bool
  /// Saves whether Details shows.
  var saveDetails: @MainActor (Bool) -> Void
  /// The small screen's saved corner, by its name; nil when none is saved.
  var loadCorner: @MainActor () -> String? = { nil }
  /// Saves the small screen's corner, by its name.
  var saveCorner: @MainActor (String) -> Void = { _ in }

  /// Preferences kept in `defaults` under the shipped keys.
  ///
  /// - Parameter defaults: Where to keep them.
  /// - Returns: The preferences.
  static func stored(in defaults: UserDefaults) -> ViewerPreferences {
    ViewerPreferences(
      loadRate: { defaults.double(forKey: rateKey) },
      saveRate: { defaults.set($0, forKey: rateKey) },
      loadDetails: { defaults.bool(forKey: detailsKey) },
      saveDetails: { defaults.set($0, forKey: detailsKey) },
      loadCorner: { defaults.string(forKey: cornerKey) },
      saveCorner: { defaults.set($0, forKey: cornerKey) }
    )
  }
}
