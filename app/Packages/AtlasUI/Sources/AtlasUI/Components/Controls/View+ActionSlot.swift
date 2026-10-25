import SwiftUI

extension View {
  /// Shows the control only while its action applies, growing it in and out of its place. The row around it
  /// says ``animatesActionSlots(_:)`` so its neighbours move with it.
  ///
  /// - Parameter isShown: Whether the action applies.
  /// - Returns: The control in its slot.
  func actionSlot(_ isShown: Bool) -> some View {
    modifier(ActionSlot(isShown: isShown))
  }

  /// Animates a row of controls as its slots come and go, so every control in it moves together.
  ///
  /// - Parameter isShown: Whether the slots' controls show.
  /// - Returns: The row, animated as one.
  func animatesActionSlots(_ isShown: Bool) -> some View {
    modifier(ActionSlotRow(isShown: isShown))
  }
}
