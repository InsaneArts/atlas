import SwiftUI

extension View {
  /// Shows a quiet line at the top of the ``CardScroll`` the view is in, saying what waits above the person scrolled
  /// down; pressing it scrolls the list to its top, where what waits comes in.
  ///
  /// - Parameter words: What waits, such as "3 new sessions", or nil while nothing does.
  /// - Returns: The view.
  func listWaiting(_ words: String?) -> some View {
    preference(key: CardScrollWaitingKey.self, value: words)
  }
}
