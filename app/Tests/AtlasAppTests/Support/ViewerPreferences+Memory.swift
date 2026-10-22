import AtlasUI

@testable import AtlasApp

extension ViewerPreferences {
  /// Preferences kept in memory, so a test never writes the real defaults.
  ///
  /// - Parameters:
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - details: Whether Details was left showing.
  /// - Returns: The preferences.
  @MainActor static func memory(rate: Double = 0, details: Bool = false) -> ViewerPreferences {
    final class Store {
      var rate: Double
      var details: Bool

      init(rate: Double, details: Bool) {
        self.rate = rate
        self.details = details
      }
    }
    let store = Store(rate: rate, details: details)
    return ViewerPreferences(
      loadRate: { store.rate },
      saveRate: { store.rate = $0 },
      loadDetails: { store.details },
      saveDetails: { store.details = $0 }
    )
  }
}
