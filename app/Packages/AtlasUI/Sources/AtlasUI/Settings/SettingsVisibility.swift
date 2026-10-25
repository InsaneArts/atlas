import SwiftUI

/// Crossfades a stable surface while immediately retiring its input and accessibility ownership.
struct SettingsVisibility: ViewModifier {
  /// Whether this surface owns the current presentation.
  let active: Bool
  /// Whether this surface is the current one or the single outgoing surface.
  var retained = true
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Animates opacity alone, leaving focus, filtering and layout immediate and interruptible.
  ///
  /// - Parameter content: The stable page or results surface.
  /// - Returns: Its visible or retired presentation.
  func body(content: Content) -> some View {
    content
      .animation(reduceMotion ? Motion.crossfadeReduced : (active ? Motion.swapIn : Motion.swapOut)) {
        $0.opacity(active ? 1 : 0)
      }
      // An interrupted fade cannot leave older pages drawing behind the latest pair.
      .opacity(retained ? 1 : 0)
      .disabled(!active)
      .allowsHitTesting(active)
      .accessibilityHidden(!active)
  }
}
