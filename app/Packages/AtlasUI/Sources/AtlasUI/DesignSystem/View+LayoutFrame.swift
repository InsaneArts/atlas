import SwiftUI

extension View {
  /// Observes real geometry in debug renders; release builds draw only the original view.
  ///
  /// - Parameter id: The element's stable identity.
  /// - Returns: The original view with a debug-only geometry observation.
  func layoutFrame(_ id: String) -> some View {
    #if DEBUG
      modifier(LayoutFrame(id: id))
    #else
      self
    #endif
  }
}
