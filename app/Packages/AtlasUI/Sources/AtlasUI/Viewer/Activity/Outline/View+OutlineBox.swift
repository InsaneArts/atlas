import SwiftUI

extension View {
  /// Stands a row and its key items as one box while `isBoxed` (``OutlineBox``).
  ///
  /// - Parameter isBoxed: Whether they stand as one box.
  /// - Returns: The row and its items.
  func outlineBox(_ isBoxed: Bool) -> some View {
    modifier(OutlineBox(isBoxed: isBoxed))
  }
}
