import SwiftUI

/// Reports where a row stands in its ``CardScroll`` as the list lays out, so rows arriving above it leave it in place
/// (``CardScrollPlace``).
struct ListPlaceFrame: ViewModifier {
  /// The row's identity.
  let id: String
  @Environment(\.cardScrollPlace) private var place

  /// The row, reporting its top in the list's visible bounds.
  ///
  /// - Parameter content: The row.
  /// - Returns: The row.
  func body(content: Content) -> some View {
    content
      .onGeometryChange(for: CGFloat.self) { proxy in
        proxy.frame(in: .scrollView).minY
      } action: { top in
        place?.report(id, top: top)
      }
      .onDisappear { place?.forget(id) }
  }
}
