import SwiftUI

/// The row of controls that holds ``ActionSlot``s, animating as one: when the slots' controls come or go, every control
/// in the row moves together, so the neighbours open the gap before a slot's control fades in, and close it once it
/// has faded out.
struct ActionSlotRow: ViewModifier {
  /// Whether the slots' controls show, the value the row animates on.
  let isShown: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The row, animating its layout as the slots change.
  ///
  /// - Parameter content: The row of controls.
  /// - Returns: The row, animated on `isShown`.
  func body(content: Content) -> some View {
    content
      .animation(ActionSlot.animation(reduceMotion: reduceMotion), value: isShown)
  }
}
