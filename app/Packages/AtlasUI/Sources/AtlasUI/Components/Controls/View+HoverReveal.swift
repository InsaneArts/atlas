import SwiftUI

extension View {
  /// Shows the control only while the pointer is over its row.
  ///
  /// - Parameter isRevealed: Whether the pointer is over the row.
  /// - Returns: The control, revealed with its row's hover.
  func revealsOnHover(_ isRevealed: Bool) -> some View {
    modifier(HoverReveal(isRevealed: isRevealed))
  }
}
