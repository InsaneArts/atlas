import AtlasUI

@testable import AtlasApp

extension ViewerPreferences {
  /// Preferences kept in memory, so a test never writes the real defaults.
  ///
  /// - Parameters:
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - sidebar: Whether the sidebar was left showing.
  /// - Returns: The preferences.
  @MainActor static func memory(rate: Double = 0, sidebar: Bool = false) -> ViewerPreferences {
    final class Store {
      var rate: Double
      var sidebar: Bool

      init(rate: Double, sidebar: Bool) {
        self.rate = rate
        self.sidebar = sidebar
      }
    }
    let store = Store(rate: rate, sidebar: sidebar)
    return ViewerPreferences(
      loadRate: { store.rate },
      saveRate: { store.rate = $0 },
      loadSidebar: { store.sidebar },
      saveSidebar: { store.sidebar = $0 }
    )
  }
}
