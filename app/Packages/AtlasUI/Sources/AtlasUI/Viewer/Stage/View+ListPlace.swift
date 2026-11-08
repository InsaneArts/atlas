import SwiftUI

extension View {
  /// Marks the row as one its ``CardScroll`` keeps in place as rows arrive above it (``ListPlaceFrame``).
  ///
  /// - Parameter id: The row's identity.
  /// - Returns: The row.
  func listPlace(_ id: String) -> some View {
    modifier(ListPlaceFrame(id: id))
  }
}
