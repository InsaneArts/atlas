import SwiftUI

/// Shrinks a control a little while it is pressed; with Reduce Motion on it stays its size.
struct PressScale: ViewModifier {
  /// Whether the control is held down.
  let isPressed: Bool
  /// How far it shrinks while held, against its size.
  let scale: CGFloat
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Scales the control while it is held, unless Reduce Motion is on.
  ///
  /// - Parameter content: The control.
  /// - Returns: The control at its pressed or its own size.
  func body(content: Content) -> some View {
    content
      .scaleEffect(isPressed && !reduceMotion ? scale : 1)
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.pressScale)
  }
#endif
