import SwiftUI

/// A row of a ``CardScroll`` that may be its selected one: while it is, it tells the list where it stands as the list
/// scrolls, so a step to it scrolls it into view clear of the small screen
/// (``CardScrollRoom/revealAnchor(row:viewport:clearance:)``).
struct ListSelectionFrame: ViewModifier {
  /// The row's identity, which the list scrolls to.
  let id: String
  /// Whether it is the selected one.
  let isSelected: Bool
  @Environment(\.cardScrollSelection) private var selection

  /// The row, reporting where it stands while it is selected; the same view either way, so selecting it never
  /// builds it again.
  func body(content: Content) -> some View {
    content
      .onGeometryChange(for: CGRect?.self) { proxy in
        isSelected ? proxy.frame(in: .scrollView) : nil
      } action: { frame in
        if let frame { selection?.report(id, at: frame) }
      }
  }
}
