import SwiftUI

extension View {
  /// Marks the row as one the person can select in its ``CardScroll``, which scrolls it into view clear of the small
  /// screen as they step to it (``ListSelectionFrame``).
  ///
  /// - Parameters:
  ///   - id: The row's identity, as the list's `ForEach` gives it.
  ///   - isSelected: Whether it is the selected one.
  /// - Returns: The row.
  func listSelection(_ id: String, isSelected: Bool) -> some View {
    modifier(ListSelectionFrame(id: id, isSelected: isSelected))
  }
}
