import SwiftUI

/// A control that shows only while the pointer is over its row, such as Show on Screen on the window it would
/// show. It fades in with a 4-point slide over ``Motion/hover(_:)``'s 0.12 seconds and back over 0.2; under
/// Reduce Motion it only fades. It also shows while it holds the keyboard's focus, so Tab and Full Keyboard
/// Access reach it. Hidden, it takes no clicks.
struct HoverReveal: ViewModifier {
  /// Whether the pointer is over the row.
  let isRevealed: Bool
  @FocusState private var isFocused: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The control, shown or hidden.
  ///
  /// - Parameter content: The control.
  /// - Returns: The control, revealed with its row's hover.
  func body(content: Content) -> some View {
    content
      .focused($isFocused)
      .opacity(isShown ? 1 : 0)
      .offset(x: isShown || reduceMotion ? 0 : Spacing.xs)
      .allowsHitTesting(isShown)
      .animation(Motion.hover(isShown), value: isShown)
  }

  /// Whether the control shows: under the pointer, or holding the keyboard's focus.
  private var isShown: Bool {
    isRevealed || isFocused
  }
}

#if DEBUG
  #Preview("Interactive") {
    HoverRevealSample()
  }
#endif
