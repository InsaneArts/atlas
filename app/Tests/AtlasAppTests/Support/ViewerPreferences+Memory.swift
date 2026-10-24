import AtlasUI

@testable import AtlasApp

extension ViewerPreferences {
  /// Preferences kept in memory, so a test never writes the real defaults.
  ///
  /// - Parameters:
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - details: Whether Details was left showing.
  ///   - corner: The small screen's saved corner, by its name; nil for none.
  /// - Returns: The preferences.
  static func memory(rate: Double = 0, details: Bool = false, corner: String? = nil) -> ViewerPreferences {
    final class Store {
      var rate: Double
      var details: Bool
      var corner: String?

      init(rate: Double, details: Bool, corner: String?) {
        self.rate = rate
        self.details = details
        self.corner = corner
      }
    }
    let store = Store(rate: rate, details: details, corner: corner)
    return ViewerPreferences(
      loadRate: { store.rate },
      saveRate: { store.rate = $0 },
      loadDetails: { store.details },
      saveDetails: { store.details = $0 },
      loadCorner: { store.corner },
      saveCorner: { store.corner = $0 }
    )
  }
}
