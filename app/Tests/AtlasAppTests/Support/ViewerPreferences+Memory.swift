import AtlasUI

@testable import AtlasApp

extension ViewerPreferences {
  /// Preferences kept in memory, so a test never writes the real defaults.
  ///
  /// - Parameters:
  ///   - rate: The saved picture rate, in seconds; 0 for none.
  ///   - corner: The small screen's saved corner, by its name; nil for none.
  ///   - sidebarHidden: Whether the person left the sidebar hidden.
  ///   - windowOpen: Whether the main window was open when Atlas last stopped.
  /// - Returns: The preferences.
  static func memory(
    rate: Double = 0,
    corner: String? = nil,
    sidebarHidden: Bool = false,
    windowOpen: Bool = false
  ) -> ViewerPreferences {
    final class Store {
      var rate: Double
      var corner: String?
      var sidebarHidden: Bool
      var windowOpen: Bool

      init(rate: Double, corner: String?, sidebarHidden: Bool, windowOpen: Bool) {
        self.rate = rate
        self.corner = corner
        self.sidebarHidden = sidebarHidden
        self.windowOpen = windowOpen
      }
    }
    let store = Store(rate: rate, corner: corner, sidebarHidden: sidebarHidden, windowOpen: windowOpen)
    return ViewerPreferences(
      loadRate: { store.rate },
      saveRate: { store.rate = $0 },
      loadCorner: { store.corner },
      saveCorner: { store.corner = $0 },
      loadSidebarHidden: { store.sidebarHidden },
      saveSidebarHidden: { store.sidebarHidden = $0 },
      loadWindowOpen: { store.windowOpen },
      saveWindowOpen: { store.windowOpen = $0 }
    )
  }
}
